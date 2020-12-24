use xim_parser::{
    bstr::BString, Attr, AttrType, Attribute, AttributeName, CommitData, ErrorCode, ErrorFlag,
    InputStyle, InputStyleList, Request,
};

pub fn handle_request<S: ServerCore + Server, H: ServerHandler<S>>(
    server: &mut S,
    com_win: u32,
    req: Request,
    handler: &mut H,
) -> Result<(), ServerError> {
    log::trace!("req: {:?}", req);
    match req {
        Request::Connect { .. } => {
            server.send_req(
                handler.get_client_window(com_win)?,
                Request::ConnectReply {
                    server_major_protocol_version: 1,
                    server_minor_protocol_version: 0,
                },
            )?;
        }
        Request::Open { locale } => {
            let (client_win, input_method_id) = handler.handle_open(server, com_win, locale)?;
            server.send_req(
                client_win,
                Request::OpenReply {
                    input_method_id,
                    im_attrs: vec![Attr {
                        id: 0,
                        name: AttributeName::QueryInputStyle,
                        ty: AttrType::Style,
                    }],
                    ic_attrs: vec![Attr {
                        id: 0,
                        name: AttributeName::InputStyle,
                        ty: AttrType::Long,
                    }],
                },
            )?;
        }
        Request::QueryExtension {
            input_method_id, ..
        } => {
            // Extension not supported now
            server.send_req(
                handler.get_client_window(com_win)?,
                Request::QueryExtensionReply {
                    input_method_id,
                    extensions: Vec::new(),
                },
            )?;
        }
        Request::EncodingNegotiation {
            input_method_id,
            encodings,
            ..
        } => {
            let client_win = handler.get_client_window(com_win)?;

            match encodings
                .iter()
                .position(|e| e.starts_with(b"COMPOUND_TEXT"))
            {
                Some(pos) => {
                    server.send_req(
                        client_win,
                        Request::EncodingNegotiationReply {
                            input_method_id,
                            category: 0,
                            index: pos as u16,
                        },
                    )?;
                }
                None => {
                    server.send_req(
                        client_win,
                        Request::Error {
                            input_method_id,
                            input_context_id: 0,
                            flag: ErrorFlag::INPUTMETHODIDVALID,
                            code: ErrorCode::BadName,
                            detail: "Only COMPOUND_TEXT encoding is supported".into(),
                        },
                    )?;
                }
            }
        }
        Request::GetImValues {
            input_method_id,
            im_attributes,
        } => {
            let client_win = handler.get_client_window(com_win)?;

            let mut out = Vec::with_capacity(im_attributes.len());

            for id in im_attributes {
                match id {
                    0 => {
                        out.push(Attribute {
                            id,
                            value: xim_parser::write_to_vec(InputStyleList {
                                styles: handler.input_styles().as_ref().to_vec(),
                            }),
                        });
                    }
                    _ => {
                        return server.error(
                            client_win,
                            ErrorCode::BadName,
                            "Unknown im attribute id".into(),
                            Some(input_method_id),
                            None,
                        );
                    }
                }
            }

            server.send_req(
                client_win,
                Request::GetImValuesReply {
                    input_method_id,
                    im_attributes: out,
                },
            )?;
        }
        _ => {
            log::warn!("Unknown request: {:?}", req);
        }
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Client doesn't exists")]
    ClientNotExists,
    #[error("Can't read xim message {0}")]
    ReadProtocol(#[from] xim_parser::ReadError),
    #[error("Client send error code: {0:?}, detail: {1}")]
    XimError(xim_parser::ErrorCode, BString),
    #[error("Invalid reply from client")]
    InvalidReply,
    #[error("Another instance is running")]
    AlreadyRunning,
    #[error(transparent)]
    Other(Box<dyn std::error::Error>),
}

pub trait ServerHandler<S: Server> {
    type InputStyleArray: AsRef<[InputStyle]>;
    fn input_styles(&self) -> Self::InputStyleArray;

    fn get_client_window(&self, com_win: u32) -> Result<u32, ServerError>;

    fn handle_xconnect(
        &mut self,
        server: &mut S,
        com_win: u32,
        client_win: u32,
    ) -> Result<(), ServerError>;
    /// Return (client_win, input_method_id)
    fn handle_open(
        &mut self,
        server: &mut S,
        com_win: u32,
        locale: BString,
    ) -> Result<(u32, u16), ServerError>;
}

pub trait Server {
    fn error(
        &mut self,
        client_win: u32,
        code: ErrorCode,
        detail: BString,
        input_method_id: Option<u16>,
        input_context_id: Option<u16>,
    ) -> Result<(), ServerError>;

    fn commit(
        &mut self,
        client_win: u32,
        input_method_id: u16,
        input_context_id: u16,
        s: &str,
    ) -> Result<(), ServerError>;

    fn set_event_mask(
        &mut self,
        client_win: u32,
        input_method_id: u16,
        input_context_id: u16,
        forward_event_mask: u32,
        synchronous_event_mask: u32,
    ) -> Result<(), ServerError>;
}

impl<S: ServerCore> Server for S {
    fn error(
        &mut self,
        client_win: u32,
        code: ErrorCode,
        detail: BString,
        input_method_id: Option<u16>,
        input_context_id: Option<u16>,
    ) -> Result<(), ServerError> {
        let mut flag = ErrorFlag::empty();

        let input_method_id = if let Some(id) = input_method_id {
            flag |= ErrorFlag::INPUTMETHODIDVALID;
            id
        } else {
            0
        };

        let input_context_id = if let Some(id) = input_context_id {
            flag |= ErrorFlag::INPUTCONTEXTIDVALID;
            id
        } else {
            0
        };

        self.send_req(
            client_win,
            Request::Error {
                input_method_id,
                input_context_id,
                code,
                detail,
                flag,
            },
        )
    }

    fn commit(
        &mut self,
        client_win: u32,
        input_method_id: u16,
        input_context_id: u16,
        s: &str,
    ) -> Result<(), ServerError> {
        self.send_req(
            client_win,
            Request::Commit {
                input_method_id,
                input_context_id,
                data: CommitData::Chars {
                    commited: ctext::utf8_to_compound_text(s),
                    syncronous: false,
                },
            },
        )
    }

    fn set_event_mask(
        &mut self,
        client_win: u32,
        input_method_id: u16,
        input_context_id: u16,
        forward_event_mask: u32,
        synchronous_event_mask: u32,
    ) -> Result<(), ServerError> {
        self.send_req(
            client_win,
            Request::SetEventMask {
                input_method_id,
                input_context_id,
                forward_event_mask,
                synchronous_event_mask,
            },
        )
    }
}

pub trait ServerCore {
    fn send_req(&mut self, client_win: u32, req: Request) -> Result<(), ServerError>;
}
