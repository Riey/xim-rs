use slab::Slab;
use std::collections::HashMap;
use xim_parser::{
    bstr::BString, Attr, AttrType, Attribute, AttributeName, ErrorCode, ErrorFlag, InputStyle,
    InputStyleList, Request,
};

use crate::server::{Server, ServerCore, ServerError, ServerHandler};

pub struct InputContext {
    input_method_id: u16,
    input_style: InputStyle,
}

pub struct InputMethod {
    locale: BString,
    input_contexts: Slab<InputContext>,
}

impl InputMethod {
    pub fn new(locale: BString) -> Self {
        Self {
            locale,
            input_contexts: Slab::new(),
        }
    }
}

pub struct XimConnection {
    client_win: u32,
    input_methods: Slab<InputMethod>,
}

impl XimConnection {
    fn get_input_method(&mut self, id: u16) -> Result<&mut InputMethod, ServerError> {
        self.input_methods
            .get_mut(id as usize)
            .ok_or(ServerError::ClientNotExists)
    }

    pub(crate) fn handle_request<S: ServerCore + Server, H: ServerHandler<S>>(
        &mut self,
        server: &mut S,
        req: Request,
        handler: &mut H,
    ) -> Result<(), ServerError> {
        log::trace!("req: {:?}", req);
        match req {
            Request::Connect { .. } => {
                server.send_req(
                    self.client_win,
                    Request::ConnectReply {
                        server_major_protocol_version: 1,
                        server_minor_protocol_version: 0,
                    },
                )?;
            }
            Request::Open { locale } => {
                let input_method_id = self.input_methods.insert(InputMethod::new(locale)) as u16;

                server.send_req(
                    self.client_win,
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

            Request::CreateIc {
                input_method_id,
                ic_attributes,
            } => {
                let input_style = ic_attributes
                    .into_iter()
                    .find(|attr| attr.id == 0)
                    .and_then(|attr| xim_parser::read(&attr.value).ok())
                    .unwrap_or(InputStyle::empty());

                handler.handle_create_ic(server, self, input_method_id, input_style)?;

                server.send_req(
                    self.client_win,
                    Request::CreateIcReply {
                        input_method_id,
                        input_context_id,
                    },
                )?;
            }

            Request::QueryExtension {
                input_method_id, ..
            } => {
                // Extension not supported now
                server.send_req(
                    self.client_win,
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
                match encodings
                    .iter()
                    .position(|e| e.starts_with(b"COMPOUND_TEXT"))
                {
                    Some(pos) => {
                        server.send_req(
                            self.client_win,
                            Request::EncodingNegotiationReply {
                                input_method_id,
                                category: 0,
                                index: pos as u16,
                            },
                        )?;
                    }
                    None => {
                        server.send_req(
                            self.client_win,
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
                                self.client_win,
                                ErrorCode::BadName,
                                "Unknown im attribute id".into(),
                                Some(input_method_id),
                                None,
                            );
                        }
                    }
                }

                server.send_req(
                    self.client_win,
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
}

pub struct XimConnections {
    connections: HashMap<u32, XimConnection>,
}

impl XimConnections {
    pub fn new_connection(&mut self, com_win: u32, client_win: u32) {
        self.connections.insert(
            com_win,
            XimConnection {
                client_win,
                input_methods: Slab::new(),
            },
        );
    }

    pub fn get_connection(&mut self, com_win: u32) -> Option<&mut XimConnection> {
        self.connections.get_mut(&com_win)
    }
}
