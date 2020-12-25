mod im_vec;

use ahash::AHashMap;
use std::num::{NonZeroU16, NonZeroU32};
use xim_parser::{
    bstr::{BStr, BString},
    Attr, AttrType, Attribute, AttributeName, ErrorCode, ErrorFlag, ForwardEventFlag, InputStyle,
    InputStyleList, Request,
};

use self::im_vec::ImVec;
use crate::server::{Server, ServerCore, ServerError, ServerHandler};

pub struct InputContext<T: Default> {
    client_win: u32,
    app_win: Option<NonZeroU32>,
    app_focus_win: Option<NonZeroU32>,
    input_method_id: NonZeroU16,
    input_context_id: NonZeroU16,
    input_style: InputStyle,
    locale: BString,
    pub user_data: T,
}

impl<T: Default> InputContext<T> {
    pub fn new(
        client_win: u32,
        app_win: Option<NonZeroU32>,
        app_focus_win: Option<NonZeroU32>,
        input_method_id: NonZeroU16,
        input_context_id: NonZeroU16,
        input_style: InputStyle,
        locale: BString,
    ) -> Self {
        Self {
            client_win,
            app_win,
            app_focus_win,
            input_method_id,
            input_context_id,
            input_style,
            locale,
            user_data: T::default(),
        }
    }

    pub fn client_win(&self) -> u32 {
        self.client_win
    }

    pub fn app_win(&self) -> Option<NonZeroU32> {
        self.app_win
    }

    pub fn app_focus_win(&self) -> Option<NonZeroU32> {
        self.app_focus_win
    }

    pub fn input_method_id(&self) -> NonZeroU16 {
        self.input_method_id
    }

    pub fn input_context_id(&self) -> NonZeroU16 {
        self.input_context_id
    }

    pub fn input_style(&self) -> InputStyle {
        self.input_style
    }

    pub fn locale(&self) -> &BStr {
        self.locale.as_ref()
    }
}

pub struct InputMethod<T: Default> {
    locale: BString,
    input_contexts: ImVec<InputContext<T>>,
}

impl<T: Default> InputMethod<T> {
    pub fn new(locale: BString) -> Self {
        Self {
            locale,
            input_contexts: ImVec::new(),
        }
    }

    pub fn clone_locale(&self) -> BString {
        self.locale.clone()
    }

    pub fn new_ic(&mut self, ic: InputContext<T>) -> (NonZeroU16, &mut InputContext<T>) {
        self.input_contexts.new_item(ic)
    }

    pub fn get_input_context(
        &mut self,
        ic_id: NonZeroU16,
    ) -> Result<&mut InputContext<T>, ServerError> {
        self.input_contexts
            .get_item(ic_id)
            .ok_or(ServerError::ClientNotExists)
    }
}

const IC_INPUTSTYLE: u16 = 0;
const IC_CLIENTWIN: u16 = 1;
const IC_FOCUSWIN: u16 = 2;

pub struct XimConnection<T: Default> {
    client_win: u32,
    input_methods: ImVec<InputMethod<T>>,
}

impl<T: Default> XimConnection<T> {
    pub fn new(client_win: u32) -> Self {
        Self {
            client_win,
            input_methods: ImVec::new(),
        }
    }

    fn get_input_method(&mut self, id: NonZeroU16) -> Result<&mut InputMethod<T>, ServerError> {
        self.input_methods
            .get_item(id)
            .ok_or(ServerError::ClientNotExists)
    }

    pub(crate) fn handle_request<
        S: ServerCore + Server,
        H: ServerHandler<S, InputContextData = T>,
    >(
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
                let (input_method_id, _im) = self.input_methods.new_item(InputMethod::new(locale));

                server.send_req(
                    self.client_win,
                    Request::OpenReply {
                        input_method_id: input_method_id.get(),
                        im_attrs: vec![Attr {
                            id: 0,
                            name: AttributeName::QueryInputStyle,
                            ty: AttrType::Style,
                        }],
                        ic_attrs: vec![
                            Attr {
                                id: IC_INPUTSTYLE,
                                name: AttributeName::InputStyle,
                                ty: AttrType::Long,
                            },
                            Attr {
                                id: IC_CLIENTWIN,
                                name: AttributeName::ClientWindow,
                                ty: AttrType::Window,
                            },
                            Attr {
                                id: IC_FOCUSWIN,
                                name: AttributeName::FocusWindow,
                                ty: AttrType::Window,
                            },
                        ],
                    },
                )?;
            }

            Request::CreateIc {
                input_method_id,
                ic_attributes,
            } => {
                let input_method_id =
                    NonZeroU16::new(input_method_id).ok_or(ServerError::ClientNotExists)?;

                let mut input_style = InputStyle::empty();
                let mut app_win = None;
                let mut app_focus_win = None;

                for attr in ic_attributes {
                    match attr.id {
                        IC_INPUTSTYLE => {
                            if let Some(style) = xim_parser::read(&attr.value).ok() {
                                input_style = style;
                            }
                        }
                        IC_CLIENTWIN => {
                            app_win = xim_parser::read(&attr.value).ok().and_then(NonZeroU32::new);
                        }
                        IC_FOCUSWIN => {
                            app_focus_win =
                                xim_parser::read(&attr.value).ok().and_then(NonZeroU32::new);
                        }
                        _ => {}
                    }
                }

                let client_win = self.client_win;
                let im = self.get_input_method(input_method_id)?;
                let ic = InputContext::new(
                    client_win,
                    app_win,
                    app_focus_win,
                    input_method_id,
                    NonZeroU16::new(1).unwrap(),
                    input_style,
                    im.clone_locale(),
                );
                let (input_context_id, ic) = im.new_ic(ic);
                ic.input_context_id = input_context_id;

                server.send_req(
                    ic.client_win(),
                    Request::CreateIcReply {
                        input_method_id: input_method_id.get(),
                        input_context_id: input_context_id.get(),
                    },
                )?;

                handler.handle_create_ic(server, ic)?;
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
                                NonZeroU16::new(input_method_id),
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

            Request::ForwardEvent {
                input_method_id,
                input_context_id,
                serial_number,
                flag,
                xev,
            } => {
                match (
                    NonZeroU16::new(input_method_id),
                    NonZeroU16::new(input_context_id),
                ) {
                    (Some(input_method_id), Some(input_context_id)) => {
                        let ev = server.deserialize_event(&xev);
                        let input_context = self
                            .get_input_method(input_method_id)?
                            .get_input_context(input_context_id)?;
                        let consumed = handler.handle_forward_event(server, input_context, &ev)?;

                        if !consumed {
                            server.send_req(
                                self.client_win,
                                Request::ForwardEvent {
                                    input_method_id: input_method_id.get(),
                                    input_context_id: input_context_id.get(),
                                    serial_number,
                                    flag: ForwardEventFlag::empty(),
                                    xev,
                                },
                            )?;
                        }
                    }
                    (im, ic) => {
                        return server.error(
                            self.client_win,
                            ErrorCode::BadSomething,
                            "Not exists".into(),
                            im,
                            ic,
                        );
                    }
                }

                if flag.contains(ForwardEventFlag::SYNCHRONOUS) {
                    server.send_req(
                        self.client_win,
                        Request::SyncReply {
                            input_method_id,
                            input_context_id,
                        },
                    )?;
                }
            }
            _ => {
                log::warn!("Unknown request: {:?}", req);
            }
        }

        Ok(())
    }
}

pub struct XimConnections<T: Default> {
    connections: AHashMap<u32, XimConnection<T>>,
}

impl<T: Default> XimConnections<T> {
    pub fn new() -> Self {
        Self {
            connections: AHashMap::new(),
        }
    }

    pub fn new_connection(&mut self, com_win: u32, client_win: u32) {
        self.connections.insert(
            com_win,
            XimConnection {
                client_win,
                input_methods: ImVec::new(),
            },
        );
    }

    pub fn get_connection(&mut self, com_win: u32) -> Option<&mut XimConnection<T>> {
        self.connections.get_mut(&com_win)
    }
}
