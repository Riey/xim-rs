use std::convert::TryInto;

use crate::Atoms;
use parser::{Attr, Attribute, XimString};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            Atom, AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt, Screen,
            SelectionRequestEvent, WindowClass, CLIENT_MESSAGE_EVENT, SELECTION_REQUEST_EVENT,
        },
        Event,
    },
    COPY_DEPTH_FROM_PARENT, CURRENT_TIME,
};
use xim_parser as parser;
use xim_parser::Request;

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
    #[error("Server Transport is not supported")]
    UnsupportedTransport,
    #[error("Invalid reply from server")]
    InvalidReply,
    #[error("Can't connect xim server")]
    NoXimServer,
}

pub struct Client<'x, C: Connection + ConnectionExt> {
    conn: &'x C,
    server_owner_window: u32,
    im_window: u32,
    server_atom: Atom,
    atoms: Atoms<Atom>,
    transport_max: usize,
    client_window: u32,
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
                100000,
            )?
            .reply()?;

        if server_reply.type_ != AtomEnum::ATOM.into() || server_reply.format != 32 {
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
                            im_window: x11rb::NONE,
                            transport_max: 20,
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
                            100000,
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

    pub fn send_req(&mut self, req: Request) -> Result<(), ClientError> {
        parser::write(&req, &mut self.buf);

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
            todo!("Property");
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
            todo!("receive by property")
        } else if msg.format == 8 {
            let data = msg.data.as_data8();
            let req = parser::read(&data)?;
            log::trace!("Get XIM message {:?}", req);
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

// impl<'x, C: Connection + ConnectionExt> Clone for Client<'x, C> {
//     fn clone(&self) -> Self {
//         Self {
//             conn: self.conn,
//             state: self.state,
//             client_window: self.client_window,
//             atoms: self.atoms,
//             im_window: self.im_window,
//             server_atom: self.server_atom,
//             server_owner_window: self.server_owner_window,
//             transport_max: self.transport_max,
//             buf: Vec::with_capacity(1024),
//         }
//     }
// }
