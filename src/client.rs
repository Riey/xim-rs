mod attribute_builder;

pub use self::attribute_builder::AttributeBuilder;
use ahash::AHashMap;
use xim_parser::{
    Attr, Attribute, AttributeName, CommitData, Extension, ForwardEventFlag, Request,
};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Can't read xim message {0}")]
    ReadProtocol(#[from] xim_parser::ReadError),
    #[error("Server send error code: {0:?}, detail: {1}")]
    XimError(xim_parser::ErrorCode, String),
    #[error("Server Transport is not supported")]
    UnsupportedTransport,
    #[error("Invalid reply from server")]
    InvalidReply,
    #[error("Can't connect xim server")]
    NoXimServer,
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

pub fn handle_request<C: ClientCore>(
    client: &mut C,
    handler: &mut impl ClientHandler<C>,
    req: Request,
) -> Result<(), ClientError> {
    log::debug!("<-: {}", req.name());

    match req {
        Request::ConnectReply {
            server_major_protocol_version: _,
            server_minor_protocol_version: _,
        } => handler.handle_connect(client),
        Request::OpenReply {
            input_method_id,
            im_attrs,
            ic_attrs,
        } => {
            log::debug!("im_attrs: {:#?}", im_attrs);
            log::debug!("ic_attrs: {:#?}", ic_attrs);
            client.set_attrs(im_attrs, ic_attrs);
            // Require for uim
            client.send_req(Request::EncodingNegotiation {
                encodings: vec!["COMPOUND_TEXT".into()],
                encoding_infos: vec![],
                input_method_id,
            })
        }
        Request::EncodingNegotiationReply {
            input_method_id,
            index: _,
            category: _,
        } => handler.handle_open(client, input_method_id),
        Request::QueryExtensionReply {
            input_method_id: _,
            extensions,
        } => handler.handle_query_extension(client, &extensions),
        Request::GetImValuesReply {
            input_method_id,
            im_attributes,
        } => handler.handle_get_im_values(
            client,
            input_method_id,
            im_attributes
                .into_iter()
                .filter_map(|attr| {
                    client
                        .im_attributes()
                        .iter()
                        .find(|(_, v)| **v == attr.id)
                        .map(|(n, _)| (*n, attr.value))
                })
                .collect(),
        ),
        Request::SetIcValuesReply {
            input_method_id,
            input_context_id,
        } => handler.handle_set_ic_values(client, input_method_id, input_context_id),
        Request::CreateIcReply {
            input_method_id,
            input_context_id,
        } => handler.handle_create_ic(client, input_method_id, input_context_id),
        Request::SetEventMask {
            input_method_id,
            input_context_id,
            forward_event_mask,
            synchronous_event_mask,
        } => handler.handle_set_event_mask(
            client,
            input_method_id,
            input_context_id,
            forward_event_mask,
            synchronous_event_mask,
        ),
        Request::CloseReply { input_method_id } => handler.handle_close(client, input_method_id),
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
        } => {
            handler.handle_forward_event(
                client,
                input_method_id,
                input_context_id,
                flag,
                client.deserialize_event(&xev),
            )?;

            if flag.contains(ForwardEventFlag::SYNCHRONOUS) {
                client.send_req(Request::SyncReply {
                    input_method_id,
                    input_context_id,
                })?;
            }

            Ok(())
        }
        Request::Commit {
            input_method_id,
            input_context_id,
            data,
        } => match data {
            CommitData::Keysym { keysym: _, .. } => {
                todo!()
            }
            CommitData::Chars {
                commited,
                syncronous,
            } => {
                handler.handle_commit(
                    client,
                    input_method_id,
                    input_context_id,
                    xim_ctext::compound_text_to_utf8(&commited).expect("Encoding Error"),
                )?;

                if syncronous {
                    client.send_req(Request::SyncReply {
                        input_method_id,
                        input_context_id,
                    })?;
                }

                Ok(())
            }
            _ => todo!(),
        },
        Request::Sync {
            input_method_id,
            input_context_id,
        } => client.send_req(Request::SyncReply {
            input_method_id,
            input_context_id,
        }),
        Request::SyncReply { .. } => {
            // Nothing to do
            Ok(())
        }
        _ => {
            log::warn!("Unknown request {:?}", req);
            Ok(())
        }
    }
}

pub trait ClientCore {
    type XEvent;

    fn set_attrs(&mut self, ic_attrs: Vec<Attr>, im_attrs: Vec<Attr>);
    fn ic_attributes(&self) -> &AHashMap<AttributeName, u16>;
    fn im_attributes(&self) -> &AHashMap<AttributeName, u16>;
    fn serialize_event(&self, xev: &Self::XEvent) -> xim_parser::XEvent;
    fn deserialize_event(&self, xev: &xim_parser::XEvent) -> Self::XEvent;
    fn send_req(&mut self, req: Request) -> Result<(), ClientError>;
}

pub trait Client {
    type XEvent;

    fn build_ic_attributes(&self) -> AttributeBuilder;
    fn build_im_attributes(&self) -> AttributeBuilder;

    fn disconnect(&mut self) -> Result<(), ClientError>;
    fn open(&mut self, locale: &str) -> Result<(), ClientError>;
    fn close(&mut self, input_method_id: u16) -> Result<(), ClientError>;
    fn quert_extension(
        &mut self,
        input_method_id: u16,
        extensions: &[&str],
    ) -> Result<(), ClientError>;
    fn get_im_values(
        &mut self,
        input_method_id: u16,
        names: &[AttributeName],
    ) -> Result<(), ClientError>;
    fn set_ic_values(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), ClientError>;
    fn create_ic(
        &mut self,
        input_method_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), ClientError>;
    fn destory_ic(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError>;
    fn forward_event(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        flag: ForwardEventFlag,
        xev: &Self::XEvent,
    ) -> Result<(), ClientError>;
    fn set_focus(&mut self, input_method_id: u16, input_context_id: u16)
        -> Result<(), ClientError>;
    fn unset_focus(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError>;
}

impl<C> Client for C
where
    C: ClientCore,
{
    type XEvent = C::XEvent;

    fn build_ic_attributes(&self) -> AttributeBuilder {
        AttributeBuilder::new(self.ic_attributes())
    }

    fn build_im_attributes(&self) -> AttributeBuilder {
        AttributeBuilder::new(self.im_attributes())
    }

    fn open(&mut self, locale: &str) -> Result<(), ClientError> {
        self.send_req(Request::Open {
            locale: locale.into(),
        })
    }

    fn quert_extension(
        &mut self,
        input_method_id: u16,
        extensions: &[&str],
    ) -> Result<(), ClientError> {
        self.send_req(Request::QueryExtension {
            input_method_id,
            extensions: extensions.iter().map(|&e| e.into()).collect(),
        })
    }

    fn get_im_values(
        &mut self,
        input_method_id: u16,
        names: &[AttributeName],
    ) -> Result<(), ClientError> {
        self.send_req(Request::GetImValues {
            input_method_id,
            im_attributes: names
                .iter()
                .filter_map(|name| self.im_attributes().get(name).copied())
                .collect(),
        })
    }

    fn set_ic_values(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), ClientError> {
        self.send_req(Request::SetIcValues {
            input_method_id,
            input_context_id,
            ic_attributes,
        })
    }

    fn create_ic(
        &mut self,
        input_method_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), ClientError> {
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
        xev: &Self::XEvent,
    ) -> Result<(), ClientError> {
        let ev = self.serialize_event(&xev);
        self.send_req(Request::ForwardEvent {
            input_method_id,
            input_context_id,
            flag,
            serial_number: ev.sequence,
            xev: ev,
        })
    }

    fn disconnect(&mut self) -> Result<(), ClientError> {
        self.send_req(Request::Disconnect {})
    }

    fn close(&mut self, input_method_id: u16) -> Result<(), ClientError> {
        self.send_req(Request::Close { input_method_id })
    }

    fn destory_ic(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        self.send_req(Request::DestoryIc {
            input_method_id,
            input_context_id,
        })
    }

    fn set_focus(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        self.send_req(Request::SetIcFocus {
            input_method_id,
            input_context_id,
        })
    }
    fn unset_focus(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        self.send_req(Request::UnsetIcFocus {
            input_method_id,
            input_context_id,
        })
    }
}

pub trait ClientHandler<C: Client> {
    fn handle_connect(&mut self, client: &mut C) -> Result<(), ClientError>;
    fn handle_disconnect(&mut self);
    fn handle_open(&mut self, client: &mut C, input_method_id: u16) -> Result<(), ClientError>;
    fn handle_close(&mut self, client: &mut C, input_method_id: u16) -> Result<(), ClientError>;
    fn handle_query_extension(
        &mut self,
        client: &mut C,
        extensions: &[Extension],
    ) -> Result<(), ClientError>;
    fn handle_get_im_values(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        attributes: AHashMap<AttributeName, Vec<u8>>,
    ) -> Result<(), ClientError>;
    fn handle_set_ic_values(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError>;
    fn handle_create_ic(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError>;
    fn handle_destory_ic(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError>;
    fn handle_commit(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        text: &str,
    ) -> Result<(), ClientError>;
    fn handle_forward_event(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        flag: ForwardEventFlag,
        xev: C::XEvent,
    ) -> Result<(), ClientError>;
    fn handle_set_event_mask(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        forward_event_mask: u32,
        synchronous_event_mask: u32,
    ) -> Result<(), ClientError>;
}
