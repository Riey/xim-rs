use std::{collections::HashMap, convert::TryInto};

use crate::{Atoms, AttributeBuilder};
use parser::ForwardEventFlag;
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            Atom, AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt, KeyPressEvent,
            PropMode, Screen, WindowClass, CLIENT_MESSAGE_EVENT,
        },
        Event,
    },
    x11_utils::X11Error,
    COPY_DEPTH_FROM_PARENT, CURRENT_TIME,
};
use xim_parser as parser;
use xim_parser::{bstr::BString, Attr, AttributeName, Request, XimWrite};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Connect error: {0}")]
    Connect(#[from] x11rb::errors::ConnectError),
    #[error("Reply error: {0}")]
    Reply(#[from] x11rb::errors::ReplyError),
    #[error("Connection error: {0}")]
    Connection(#[from] x11rb::errors::ConnectionError),
    #[error("ReplyOrId error: {0}")]
    ReplyOrId(#[from] x11rb::errors::ReplyOrIdError),
    #[error("Can't read xim message {0}")]
    ReadProtocol(#[from] parser::ReadError),
    #[error("Server send error code: {0:?}, detail: {1}")]
    XimError(parser::ErrorCode, BString),
    #[error("X11 error {0:?}")]
    X11Error(X11Error),
    #[error("Server Transport is not supported")]
    UnsupportedTransport,
    #[error("Invalid reply from server")]
    InvalidReply,
    #[error("Can't connect xim server")]
    NoXimServer,
}

pub struct Client<'x, C: Connection + ConnectionExt> {
    conn: &'x C,
    connected: bool,
    server_owner_window: u32,
    im_window: u32,
    server_atom: Atom,
    atoms: Atoms<Atom>,
    transport_max: usize,
    client_window: u32,
    im_attributes: HashMap<AttributeName, u16>,
    ic_attributes: HashMap<AttributeName, u16>,
    forward_event_mask: u32,
    synchronous_event_mask: u32,
    buf: Vec<u8>,
}

impl<'x, C: Connection + ConnectionExt> Client<'x, C> {
    pub fn init(
        conn: &'x C,
        screen: &'x Screen,
        im_name: Option<&str>,
    ) -> Result<Self, ClientError> {
        let client_window = conn.generate_id()?;

        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            client_window,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::CopyFromParent,
            screen.root_visual,
            &Default::default(),
        )?;

        let var = std::env::var("XMODIFIERS").ok();
        let var = var.as_ref().and_then(|n| n.strip_prefix("@im="));
        let im_name = im_name.or(var).ok_or(ClientError::NoXimServer)?;
        let atoms = Atoms::new::<ClientError, _>(|name| {
            Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
        })?;
        let server_reply = conn
            .get_property(
                false,
                screen.root,
                atoms.XIM_SERVERS,
                AtomEnum::ATOM,
                0,
                u32::MAX,
            )?
            .reply()?;

        if server_reply.type_ != u32::from(AtomEnum::ATOM) || server_reply.format != 32 {
            Err(ClientError::InvalidReply)
        } else {
            for server_atom in server_reply.value32().ok_or(ClientError::InvalidReply)? {
                let server_owner = conn.get_selection_owner(server_atom)?.reply()?.owner;
                let name = conn.get_atom_name(server_atom)?.reply()?.name;

                let name = match String::from_utf8(name) {
                    Ok(name) => name,
                    _ => continue,
                };

                if let Some(name) = name.strip_prefix("@server=") {
                    if name == im_name {
                        conn.convert_selection(
                            client_window,
                            server_atom,
                            atoms.TRANSPORT,
                            atoms.TRANSPORT,
                            CURRENT_TIME,
                        )?;

                        conn.flush()?;

                        return Ok(Self {
                            conn,
                            connected: false,
                            atoms,
                            server_atom,
                            server_owner_window: server_owner,
                            im_attributes: HashMap::new(),
                            ic_attributes: HashMap::new(),
                            im_window: x11rb::NONE,
                            transport_max: 20,
                            forward_event_mask: 0,
                            synchronous_event_mask: 0,
                            client_window,
                            buf: Vec::with_capacity(1024),
                        });
                    }
                }
            }

            Err(ClientError::NoXimServer)
        }
    }

    pub fn conn(&self) -> &'x C {
        self.conn
    }

    pub fn im_window(&self) -> u32 {
        self.im_window
    }

    pub fn set_attrs(&mut self, im_attrs: Vec<Attr>, ic_attrs: Vec<Attr>) {
        for im_attr in im_attrs {
            self.im_attributes.insert(im_attr.name, im_attr.id);
        }

        for ic_attr in ic_attrs {
            self.ic_attributes.insert(ic_attr.name, ic_attr.id);
        }
    }

    pub fn get_im_attrs(&self, names: &[AttributeName]) -> Vec<u16> {
        names
            .iter()
            .filter_map(|name| self.ic_attributes.get(name).copied())
            .collect()
    }

    pub fn get_ic_attrs(&self, names: &[AttributeName]) -> Vec<u16> {
        names
            .iter()
            .filter_map(|name| self.ic_attributes.get(name).copied())
            .collect()
    }

    pub fn build_ic_attributes(&self) -> AttributeBuilder {
        AttributeBuilder {
            id_map: &self.ic_attributes,
            out: Vec::new(),
        }
    }

    pub fn build_im_attributes(&self) -> AttributeBuilder {
        AttributeBuilder {
            id_map: &self.im_attributes,
            out: Vec::new(),
        }
    }

    pub fn filter_event(
        &mut self,
        e: &Event,
        cb: impl FnOnce(&mut Self, Request) -> Result<(), ClientError>,
    ) -> Result<bool, ClientError> {
        match e {
            Event::SelectionNotify(e) if e.requestor == self.client_window => {
                if e.property == self.atoms.LOCALES {
                    // TODO: set locale

                    self.xconnect()?;

                    Ok(true)
                } else if e.property == self.atoms.TRANSPORT {
                    let transport = self
                        .conn
                        .get_property(
                            true,
                            self.client_window,
                            self.atoms.TRANSPORT,
                            self.atoms.TRANSPORT,
                            0,
                            u32::MAX,
                        )?
                        .reply()?;

                    if !transport.value.starts_with(b"@transport=X/") {
                        return Err(ClientError::UnsupportedTransport);
                    }

                    self.conn.convert_selection(
                        self.client_window,
                        self.server_atom,
                        self.atoms.LOCALES,
                        self.atoms.LOCALES,
                        CURRENT_TIME,
                    )?;

                    self.conn.flush()?;

                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Event::ClientMessage(msg) if msg.window == self.client_window => {
                if msg.type_ == self.atoms.XIM_XCONNECT {
                    let [im_window, major, minor, max, _] = msg.data.as_data32();
                    log::info!(
                        "XConnected server on {}, transport version: {}.{}, TRANSPORT_MAX: {}",
                        im_window,
                        major,
                        minor,
                        max
                    );
                    self.im_window = im_window;
                    self.transport_max = max as usize;
                    self.send_req(Request::Connect {
                        client_major_protocol_version: 1,
                        client_minor_protocol_version: 0,
                        endian: parser::Endian::Native,
                        client_auth_protocol_names: Vec::new(),
                    })?;
                    Ok(true)
                } else if msg.type_ == self.atoms.XIM_PROTOCOL {
                    self.handle_xim_protocol(msg, cb)?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }

    pub fn forward_key_press(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        e: &KeyPressEvent,
    ) -> Result<(), ClientError> {
        self.send_req(Request::ForwardEvent {
            input_method_id,
            input_context_id,
            flag: ForwardEventFlag::SYNCHRONOUS,
            serial_number: e.sequence,
        })
    }

    pub fn set_event_mask(&mut self, forward_event_mask: u32, synchronous_event_mask: u32) {
        self.connected = true;
        self.forward_event_mask = forward_event_mask;
        self.synchronous_event_mask = synchronous_event_mask;
    }

    pub fn send_req(&mut self, req: Request) -> Result<(), ClientError> {
        self.buf.resize(req.size(), 0);
        parser::write(&req, &mut self.buf);

        if self.buf.len() < self.transport_max {
            log::trace!("Send {} by CM", req.name());
            if self.buf.len() > 20 {
                todo!("multi-CM");
            }
            self.buf.resize(20, 0);
            let buf: [u8; 20] = self.buf.as_slice().try_into().unwrap();
            self.conn.send_event(
                false,
                self.im_window,
                0u32,
                ClientMessageEvent {
                    response_type: CLIENT_MESSAGE_EVENT,
                    data: buf.into(),
                    format: 8,
                    sequence: 0,
                    type_: self.atoms.XIM_PROTOCOL,
                    window: self.im_window,
                },
            )?;
        } else {
            log::trace!("Send {} by property", req.name());
            self.conn.change_property(
                PropMode::Append,
                self.im_window,
                self.atoms.DATA,
                AtomEnum::STRING,
                8,
                self.buf.len() as u32,
                &self.buf,
            )?;
            self.conn.send_event(
                false,
                self.im_window,
                0u32,
                ClientMessageEvent {
                    data: [self.buf.len() as u32, self.atoms.DATA, 0, 0, 0].into(),
                    format: 32,
                    sequence: 0,
                    response_type: CLIENT_MESSAGE_EVENT,
                    type_: self.atoms.XIM_PROTOCOL,
                    window: self.im_window,
                },
            )?;
        }
        self.conn.flush()?;
        self.buf.clear();
        Ok(())
    }

    fn handle_xim_protocol(
        &mut self,
        msg: &ClientMessageEvent,
        cb: impl FnOnce(&mut Self, Request) -> Result<(), ClientError>,
    ) -> Result<(), ClientError> {
        if msg.format == 32 {
            let [length, atom, ..] = msg.data.as_data32();
            let data = self
                .conn
                .get_property(true, msg.window, atom, AtomEnum::Any, 0, length)?
                .reply()?
                .value;
            let req = parser::read(&data)?;
            cb(self, req)?;
        } else if msg.format == 8 {
            let data = msg.data.as_data8();
            let req = parser::read(&data)?;
            cb(self, req)?;
        }

        Ok(())
    }

    fn xconnect(&mut self) -> Result<(), ClientError> {
        self.conn.send_event(
            false,
            self.server_owner_window,
            0u32,
            ClientMessageEvent {
                data: ClientMessageData::from([self.client_window, 0, 0, 0, 0]),
                format: 32,
                response_type: CLIENT_MESSAGE_EVENT,
                sequence: 0,
                type_: self.atoms.XIM_XCONNECT,
                window: self.server_owner_window,
            },
        )?;

        self.conn.flush()?;

        Ok(())
    }
}
