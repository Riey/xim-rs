use std::{collections::HashMap, convert::TryInto};

use crate::{Atoms, AttributeBuilder, ClientHandler};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            Atom, AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt, PropMode, Screen,
            WindowClass, CLIENT_MESSAGE_EVENT,
        },
        Event,
    },
    x11_utils::X11Error,
    COPY_DEPTH_FROM_PARENT, CURRENT_TIME,
};
use xim_parser::{
    bstr::BString, Attr, Attribute, AttributeName, CommitData, ForwardEventFlag, Request, XimWrite,
};

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
    ReadProtocol(#[from] xim_parser::ReadError),
    #[error("Server send error code: {0:?}, detail: {1}")]
    XimError(xim_parser::ErrorCode, BString),
    #[error("X11 error {0:?}")]
    X11Error(X11Error),
    #[error("Server Transport is not supported")]
    UnsupportedTransport,
    #[error("Invalid reply from server")]
    InvalidReply,
    #[error("Can't connect xim server")]
    NoXimServer,
}

impl<'x, C: Connection + ConnectionExt> crate::Client for X11rbClient<'x, C> {
    type Error = ClientError;

    fn build_ic_attributes(&self) -> AttributeBuilder {
        AttributeBuilder {
            id_map: &self.ic_attributes,
            out: Vec::new(),
        }
    }

    fn build_im_attributes(&self) -> AttributeBuilder {
        AttributeBuilder {
            id_map: &self.im_attributes,
            out: Vec::new(),
        }
    }

    fn open(&mut self, locale: &[u8]) -> Result<(), Self::Error> {
        self.send_req(Request::Open {
            locale: locale.into(),
        })
    }

    fn quert_extension(
        &mut self,
        input_method_id: u16,
        extensions: &[&str],
    ) -> Result<(), Self::Error> {
        self.send_req(Request::QueryExtension {
            input_method_id,
            extensions: extensions.iter().map(|&e| e.into()).collect(),
        })
    }

    fn create_ic(
        &mut self,
        input_method_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), Self::Error> {
        self.send_req(Request::CreateIc {
            input_method_id,
            ic_attributes,
        })
    }

    fn forward_event(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        flag: ForwardEventFlag,
        sequence: u16,
        xev: xim_parser::RawXEvent,
    ) -> Result<(), Self::Error> {
        self.send_req(Request::ForwardEvent {
            input_method_id,
            input_context_id,
            flag,
            serial_number: sequence,
            xev,
        })
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.send_req(Request::Disconnect {})
    }

    fn close(&mut self, input_method_id: u16) -> Result<(), Self::Error> {
        self.send_req(Request::Close { input_method_id })
    }

    fn destory_ic(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), Self::Error> {
        self.send_req(Request::DestoryIc {
            input_method_id,
            input_context_id,
        })
    }
}

pub struct X11rbClient<'x, C: Connection + ConnectionExt> {
    conn: &'x C,
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

impl<'x, C: Connection + ConnectionExt> X11rbClient<'x, C> {
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

        log::info!("Try connect {}", im_name);

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

    fn set_attrs(&mut self, im_attrs: Vec<Attr>, ic_attrs: Vec<Attr>) {
        for im_attr in im_attrs {
            self.im_attributes.insert(im_attr.name, im_attr.id);
        }

        for ic_attr in ic_attrs {
            self.ic_attributes.insert(ic_attr.name, ic_attr.id);
        }
    }

    pub fn filter_event(
        &mut self,
        e: &Event,
        handler: &mut impl ClientHandler<Self>,
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
                        endian: xim_parser::Endian::Native,
                        client_auth_protocol_names: Vec::new(),
                    })?;
                    Ok(true)
                } else if msg.type_ == self.atoms.XIM_PROTOCOL {
                    self.handle_xim_protocol(msg, handler)?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }

    fn set_event_mask(&mut self, forward_event_mask: u32, synchronous_event_mask: u32) {
        self.forward_event_mask = forward_event_mask;
        self.synchronous_event_mask = synchronous_event_mask;
    }

    fn send_req(&mut self, req: Request) -> Result<(), ClientError> {
        self.buf.resize(req.size(), 0);
        xim_parser::write(&req, &mut self.buf);

        if self.buf.len() < self.transport_max {
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
        handler: &mut impl ClientHandler<Self>,
    ) -> Result<(), ClientError> {
        if msg.format == 32 {
            let [length, atom, ..] = msg.data.as_data32();
            let data = self
                .conn
                .get_property(true, msg.window, atom, AtomEnum::Any, 0, length)?
                .reply()?
                .value;
            let req = xim_parser::read(&data)?;
            self.handle_request(req, handler)?;
        } else if msg.format == 8 {
            let data = msg.data.as_data8();
            let req = xim_parser::read(&data)?;
            self.handle_request(req, handler)?;
        }

        Ok(())
    }

    fn handle_request(
        &mut self,
        req: Request,
        handler: &mut impl ClientHandler<Self>,
    ) -> Result<(), ClientError> {
        log::trace!("Recv: {:?}", req);
        match req {
            Request::ConnectReply {
                server_major_protocol_version: _,
                server_minor_protocol_version: _,
            } => handler.handle_connect(self),
            Request::OpenReply {
                input_method_id,
                im_attrs,
                ic_attrs,
            } => {
                self.set_attrs(im_attrs, ic_attrs);
                handler.handle_open(self, input_method_id)
            }
            Request::QueryExtensionReply {
                input_method_id: _,
                extensions,
            } => handler.handle_query_extension(self, &extensions),
            Request::CreateIcReply {
                input_method_id,
                input_context_id,
            } => handler.handle_create_ic(self, input_method_id, input_context_id),
            Request::SetEventMask {
                input_method_id: _,
                input_context_id: _,
                forward_event_mask,
                synchronous_event_mask,
            } => {
                self.set_event_mask(forward_event_mask, synchronous_event_mask);
                Ok(())
            }
            Request::CloseReply { input_method_id } => handler.handle_close(self, input_method_id),
            Request::DisconnectReply {} => {
                handler.handle_disconnect();
                Ok(())
            }
            Request::Error { code, detail, .. } => Err(ClientError::XimError(code, detail)),
            Request::ForwardEvent {
                xev,
                input_method_id,
                input_context_id,
                flag,
                ..
            } => handler.handle_forward_event(self, input_method_id, input_context_id, flag, xev),
            Request::Commit {
                input_method_id,
                input_context_id,
                data,
            } => match data {
                CommitData::Keysym { keysym: _, .. } => {
                    todo!()
                }
                CommitData::Chars { commited, .. } => handler.handle_commit(
                    self,
                    input_method_id,
                    input_context_id,
                    crate::compound_text_to_utf8(&commited).unwrap(),
                ),
                _ => todo!(),
            },
            _ => Err(ClientError::InvalidReply),
        }
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
