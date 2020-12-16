use std::convert::TryInto;

use crate::Atoms;
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            Atom, AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt, Screen,
            SelectionRequestEvent, CLIENT_MESSAGE_EVENT, SELECTION_REQUEST_EVENT,
        },
        Event,
    },
    CURRENT_TIME,
};
use xim_parser as parser;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Connect error: {0}")]
    ConnectError(#[from] x11rb::errors::ConnectError),
    #[error("Reply error: {0}")]
    ReplyError(#[from] x11rb::errors::ReplyError),
    #[error("Connection error: {0}")]
    ConnectionError(#[from] x11rb::errors::ConnectionError),
    #[error("ReplyOrId error: {0}")]
    ReplyOrIdError(#[from] x11rb::errors::ReplyOrIdError),
    #[error("Invalid reply from server")]
    InvalidReply,
    #[error("Can't connect xim server")]
    NoXimServer,
}

pub struct Client<'x, C: Connection + ConnectionExt> {
    conn: &'x C,
    screen: &'x Screen,
    server_atom: Atom,
    atoms: Atoms<Atom>,
    im_window: u32,
    client_window: u32,
    buf: Vec<u8>,
}

impl<'x, C: Connection + ConnectionExt> Client<'x, C> {
    pub fn init(
        conn: &'x C,
        screen: &'x Screen,
        window: u32,
        im_name: Option<&str>,
    ) -> Result<Self, ClientError> {
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
                        // conn.convert_selection(
                        //     window,
                        //     server_atom,
                        //     atoms.TRANSPORT,
                        //     atoms.TRANSPORT,
                        //     CURRENT_TIME,
                        // )?;

                        // conn.flush()?;

                        return Ok(Self {
                            conn,
                            screen,
                            atoms,
                            server_atom,
                            im_window: server_owner,
                            client_window: window,
                            buf: Vec::with_capacity(1024),
                        });
                    }
                }
            }

            Err(ClientError::NoXimServer)
        }
    }

    fn send_cm(&mut self, req: parser::Request) -> Result<(), ClientError> {
        parser::write(&req, &mut self.buf);
        let buf: [u8; 20] = self.buf.as_slice().try_into().unwrap();
        let mut data = ClientMessageData::from(buf);
        self.buf.clear();
        Ok(())
    }

    pub fn connect(&mut self) -> Result<(), ClientError> {
        self.conn.send_event(
            false,
            self.im_window,
            0u32,
            ClientMessageEvent {
                data: ClientMessageData::from([self.client_window, 0, 0, 0, 0]),
                format: 32,
                response_type: CLIENT_MESSAGE_EVENT,
                sequence: 0,
                type_: self.atoms.XIM_XCONNECT,
                window: self.client_window,
            },
        )?;

        self.conn.flush()?;

        Ok(())
    }

    pub fn filter_event(&mut self, e: &Event) -> Result<bool, ClientError> {
        match e {
            Event::SelectionNotify(e) => {
                if e.property == self.atoms.LOCALES {
                    // TODO: set locale

                    self.connect()?;

                    Ok(true)
                } else if e.property == self.atoms.TRANSPORT {
                    // TODO: set transport

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
            _ => Ok(false),
        }
    }
}
