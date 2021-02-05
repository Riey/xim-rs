mod im_vec;

use ahash::AHashMap;
use std::collections::VecDeque;
use std::num::{NonZeroU16, NonZeroU32};
use xim_parser::{
    attrs, Attribute, AttributeName, ErrorCode, ForwardEventFlag, InputStyle, InputStyleList,
    Point, Request, XimWrite,
};

use self::im_vec::ImVec;
use crate::{
    encoding::Encoding,
    server::{Server, ServerCore, ServerError, ServerHandler},
};

struct InputContextInner {
    client_win: u32,
    app_win: Option<NonZeroU32>,
    app_focus_win: Option<NonZeroU32>,
    input_method_id: NonZeroU16,
    input_context_id: NonZeroU16,
    input_style: InputStyle,
    preedit_spot: Point,
    locale: String,
}

impl InputContextInner {
    pub fn new(
        client_win: u32,
        input_method_id: NonZeroU16,
        input_context_id: NonZeroU16,
        locale: String,
    ) -> Self {
        Self {
            client_win,
            app_win: None,
            app_focus_win: None,
            input_method_id,
            input_context_id,
            input_style: InputStyle::empty(),
            preedit_spot: Point { x: 0, y: 0 },
            locale,
        }
    }
}

pub struct InputContext<T> {
    inner: InputContextInner,
    pub user_data: T,
}

impl<T> InputContext<T> {
    fn new(inner: InputContextInner, user_data: T) -> Self {
        Self { inner, user_data }
    }

    pub fn client_win(&self) -> u32 {
        self.inner.client_win
    }

    pub fn app_win(&self) -> Option<NonZeroU32> {
        self.inner.app_win
    }

    pub fn app_focus_win(&self) -> Option<NonZeroU32> {
        self.inner.app_focus_win
    }

    pub fn preedit_spot(&self) -> Point {
        self.inner.preedit_spot.clone()
    }

    pub fn input_method_id(&self) -> NonZeroU16 {
        self.inner.input_method_id
    }

    pub fn input_context_id(&self) -> NonZeroU16 {
        self.inner.input_context_id
    }

    pub fn input_style(&self) -> InputStyle {
        self.inner.input_style
    }

    pub fn locale(&self) -> &str {
        self.inner.locale.as_str()
    }
}

fn set_ic_attrs(ic: &mut InputContextInner, ic_attributes: Vec<Attribute>) {
    for attr in ic_attributes {
        let name = if let Some(name) = attrs::get_name(attr.id) {
            name
        } else {
            log::warn!("Unknown attr id: {}", attr.id);
            continue;
        };

        match name {
            AttributeName::InputStyle => {
                if let Some(style) = xim_parser::read(&attr.value).ok() {
                    ic.input_style = style;
                }
            }
            AttributeName::ClientWindow => {
                ic.app_win = xim_parser::read(&attr.value).ok().and_then(NonZeroU32::new);
            }
            AttributeName::FocusWindow => {
                ic.app_focus_win = xim_parser::read(&attr.value).ok().and_then(NonZeroU32::new);
            }
            AttributeName::PreeditAttributes => {
                let mut b = &attr.value[..];
                while !b.is_empty() {
                    match xim_parser::read::<Attribute>(b) {
                        Ok(attr) => {
                            b = &b[attr.size()..];
                            match attrs::get_name(attr.id) {
                                Some(AttributeName::SpotLocation) => {
                                    if let Ok(spot) = xim_parser::read(&attr.value) {
                                        ic.preedit_spot = spot;
                                    }
                                }
                                name => {
                                    log::warn!("Ignore unhandled preedit attr: {:?}", name);
                                }
                            }
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
            }
            name => {
                log::warn!("Ignore unhandled attr: {:?}", name);
            }
        }
    }
}

pub struct InputMethod<T> {
    pub(crate) locale: String,
    pub(crate) input_contexts: ImVec<InputContext<T>>,
}

impl<T> InputMethod<T> {
    pub fn new(locale: String) -> Self {
        Self {
            locale,
            input_contexts: ImVec::new(),
        }
    }

    pub fn clone_locale(&self) -> String {
        self.locale.clone()
    }

    pub fn new_ic(&mut self, ic: InputContext<T>) -> (NonZeroU16, &mut InputContext<T>) {
        self.input_contexts.new_item(ic)
    }

    pub fn remove_input_context(&mut self, ic_id: u16) -> Result<InputContext<T>, ServerError> {
        self.input_contexts
            .remove_item(ic_id)
            .ok_or(ServerError::ClientNotExists)
    }

    pub fn get_input_context(&mut self, ic_id: u16) -> Result<&mut InputContext<T>, ServerError> {
        self.input_contexts
            .get_item(ic_id)
            .ok_or(ServerError::ClientNotExists)
    }
}

pub struct XimConnection<T> {
    pub(crate) client_win: u32,
    pub(crate) disconnected: bool,
    sync: bool,
    pub(crate) input_methods: ImVec<InputMethod<T>>,
    sync_queue: SyncQueue,
}

impl<T> XimConnection<T> {
    pub fn new(client_win: u32) -> Self {
        Self {
            client_win,
            disconnected: false,
            sync: false,
            input_methods: ImVec::new(),
            sync_queue: SyncQueue::default(),
        }
    }

    pub fn disconnect<S: ServerCore + Server, H: ServerHandler<S, InputContextData = T>>(
        &mut self,
        server: &mut S,
        handler: &mut H,
    ) -> Result<(), ServerError> {
        for (_id, im) in self.input_methods.drain() {
            for (_id, ic) in im.input_contexts {
                handler.handle_destory_ic(server, ic)?;
            }
        }

        self.disconnected = true;

        Ok(())
    }

    fn get_input_method(&mut self, id: u16) -> Result<&mut InputMethod<T>, ServerError> {
        self.input_methods
            .get_item(id)
            .ok_or(ServerError::ClientNotExists)
    }

    fn remove_input_method(&mut self, id: u16) -> Result<InputMethod<T>, ServerError> {
        self.input_methods
            .remove_item(id)
            .ok_or(ServerError::ClientNotExists)
    }

    pub(crate) fn process_sync_queue<S: ServerCore, H: ServerHandler<S, InputContextData = T>>(
        &mut self,
        server: &mut S,
        handler: &mut H,
    ) -> Result<(), ServerError> {
        self.sync = false;

        while let Some(req) = self.sync_queue.dequeue_read() {
            log::debug!("<o> Processs pending request {}", req.name());
            self.handle_request(server, handler, req)?;
            // Exit when blocked
            if self.sync {
                return Ok(());
            }
        }

        for write in self.sync_queue.drain_write() {
            server.send_req(self.client_win, write)?;
        }

        Ok(())
    }

    pub(crate) fn handle_request<S: ServerCore, H: ServerHandler<S, InputContextData = T>>(
        &mut self,
        server: &mut S,
        handler: &mut H,
        req: Request,
    ) -> Result<(), ServerError> {
        let is_req_sync = req.is_sync();
        if self.sync && is_req_sync {
            log::debug!("<x> Request {} is blocking", req.name());
            self.sync_queue.enqueue_read(req);
            return Ok(());
        }

        match req {
            Request::Error {
                code,
                detail,
                flag: _,
                input_method_id: _,
                input_context_id: _,
            } => {
                // TODO: handle error

                log::error!("XIM ERROR! code: {:?}, detail: {}", code, detail);
            }

            Request::Connect { .. } => {
                server.send_req(
                    self.client_win,
                    Request::ConnectReply {
                        server_major_protocol_version: 1,
                        server_minor_protocol_version: 0,
                    },
                )?;
                handler.handle_connect(server)?;
            }

            Request::Disconnect {} => {
                self.disconnect(server, handler)?;
                server.send_req(self.client_win, Request::DisconnectReply {})?;
            }

            Request::Open { locale } => {
                let (input_method_id, _im) = self.input_methods.new_item(InputMethod::new(locale));

                server.send_req(
                    self.client_win,
                    Request::OpenReply {
                        input_method_id: input_method_id.get(),
                        im_attrs: vec![attrs::QUERY_INPUT_STYLE],
                        ic_attrs: vec![
                            attrs::INPUT_STYLE,
                            attrs::CLIENTWIN,
                            attrs::FOCUSWIN,
                            attrs::FILTER_EVENTS,
                            attrs::PREEDIT_ATTRIBUTES,
                            attrs::STATUS_ATTRIBUTES,
                            attrs::FONT_SET,
                            attrs::AREA,
                            attrs::AREA_NEEDED,
                            attrs::COLOR_MAP,
                            attrs::STD_COLOR_MAP,
                            attrs::FOREGROUND,
                            attrs::BACKGROUND,
                            attrs::BACKGROUND_PIXMAP,
                            attrs::SPOT_LOCATION,
                            attrs::LINE_SPACE,
                            attrs::SEPARATOR_OF_NESTED_LIST,
                        ],
                    },
                )?;
            }

            Request::CreateIc {
                input_method_id,
                ic_attributes,
            } => {
                let client_win = self.client_win;
                let im = self.get_input_method(input_method_id)?;
                let mut ic = InputContextInner::new(
                    client_win,
                    NonZeroU16::new(input_method_id).unwrap(),
                    NonZeroU16::new(1).unwrap(),
                    im.clone_locale(),
                );
                set_ic_attrs(&mut ic, ic_attributes);
                let input_style = ic.input_style;
                let ic = InputContext::new(ic, handler.new_ic_data(server, input_style)?);
                let (input_context_id, ic) = im.new_ic(ic);
                ic.inner.input_context_id = input_context_id;

                handler.handle_create_ic(server, ic)?;

                self.sync_queue.enqueue_write(Request::CreateIcReply {
                    input_method_id,
                    input_context_id: input_context_id.get(),
                });

                // TODO: trigger key

                let mask = handler.filter_events();

                let mask = if handler.sync_mode() { !mask } else { mask };

                self.sync_queue.enqueue_write(Request::SetEventMask {
                    input_method_id,
                    input_context_id: input_context_id.get(),
                    forward_event_mask: mask,
                    synchronous_event_mask: !mask,
                });

                self.process_sync_queue(server, handler)?;
            }

            Request::DestoryIc {
                input_context_id,
                input_method_id,
            } => {
                handler.handle_destory_ic(
                    server,
                    self.get_input_method(input_method_id)?
                        .remove_input_context(input_context_id)?,
                )?;

                self.sync_queue.enqueue_write(Request::DestroyIcReply {
                    input_method_id,
                    input_context_id,
                });
                self.process_sync_queue(server, handler)?;
            }

            Request::Close { input_method_id } => {
                for (_id, ic) in self.remove_input_method(input_method_id)?.input_contexts {
                    handler.handle_destory_ic(server, ic)?;
                }

                server.send_req(self.client_win, Request::CloseReply { input_method_id })?;
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
                    .position(|e| e.starts_with(Encoding::Utf8.name()))
                    .or_else(|| {
                        encodings
                            .iter()
                            .position(|e| e.starts_with(Encoding::CompoundText.name()))
                    }) {
                    Some(pos) => {
                        let encoding = if encodings[pos].starts_with(Encoding::Utf8.name()) {
                            Encoding::Utf8
                        } else {
                            Encoding::CompoundText
                        };

                        log::info!("Set encoding :{:?}", encoding);
                        server.set_encoding(encoding);

                        server.send_req(
                            self.client_win,
                            Request::EncodingNegotiationReply {
                                input_method_id,
                                category: 0,
                                index: pos as i16,
                            },
                        )?;
                    }
                    None => {
                        server.set_encoding(Encoding::CompoundText);
                        server.send_req(
                            self.client_win,
                            Request::EncodingNegotiationReply {
                                input_method_id,
                                category: 0,
                                index: -1,
                            },
                        )?;
                    }
                }
            }
            Request::ResetIc {
                input_method_id,
                input_context_id,
            } => {
                let ic = self
                    .get_input_method(input_method_id)?
                    .get_input_context(input_context_id)?;
                let ret = handler.handle_reset_ic(server, ic)?;
                self.sync_queue.enqueue_write(Request::ResetIcReply {
                    input_method_id,
                    input_context_id,
                    preedit_string: server.encoding().write(ret),
                });
                self.process_sync_queue(server, handler)?;
            }
            Request::GetImValues {
                input_method_id,
                im_attributes,
            } => {
                let mut out = Vec::with_capacity(im_attributes.len());

                for name in im_attributes.into_iter().filter_map(attrs::get_name) {
                    match name {
                        AttributeName::QueryInputStyle => {
                            out.push(Attribute {
                                id: attrs::get_id(name),
                                value: xim_parser::write_to_vec(InputStyleList {
                                    styles: handler.input_styles().as_ref().to_vec(),
                                }),
                            });
                        }
                        _ => {
                            return server.error(
                                self.client_win,
                                ErrorCode::BadName,
                                "Unknown im attribute name".into(),
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

            Request::GetIcValues {
                input_method_id,
                input_context_id,
                ic_attributes,
            } => {
                let ic = self
                    .get_input_method(input_method_id)?
                    .get_input_context(input_context_id)?;
                let mut out = Vec::with_capacity(ic_attributes.len());

                for name in ic_attributes.into_iter().filter_map(attrs::get_name) {
                    match name {
                        AttributeName::InputStyle => out.push(Attribute {
                            id: attrs::get_id(name),
                            value: xim_parser::write_to_vec(ic.input_style()),
                        }),
                        AttributeName::ClientWindow => out.push(Attribute {
                            id: attrs::get_id(name),
                            value: xim_parser::write_to_vec(
                                ic.app_win().map_or(0, NonZeroU32::get),
                            ),
                        }),
                        AttributeName::FocusWindow => out.push(Attribute {
                            id: attrs::get_id(name),
                            value: xim_parser::write_to_vec(
                                ic.app_focus_win().map_or(0, NonZeroU32::get),
                            ),
                        }),
                        AttributeName::FilterEvents => out.push(Attribute {
                            id: attrs::get_id(name),
                            value: xim_parser::write_to_vec(handler.filter_events()),
                        }),
                        AttributeName::QueryInputStyle => {
                            return server.error(
                                self.client_win,
                                ErrorCode::BadName,
                                "Unknown ic attribute name".into(),
                                NonZeroU16::new(input_method_id),
                                None,
                            );
                        }
                        name => {
                            log::warn!("Unimplemented attribute {:?}", name);
                        }
                    }
                }

                server.send_req(
                    self.client_win,
                    Request::GetIcValuesReply {
                        ic_attributes: out,
                        input_method_id,
                        input_context_id,
                    },
                )?;
            }

            Request::SetIcValues {
                input_context_id,
                input_method_id,
                ic_attributes,
            } => {
                let ic = self
                    .get_input_method(input_method_id)?
                    .get_input_context(input_context_id)?;

                set_ic_attrs(&mut ic.inner, ic_attributes);

                server.send_req(
                    ic.client_win(),
                    Request::SetIcValuesReply {
                        input_method_id,
                        input_context_id,
                    },
                )?;

                handler.handle_set_ic_values(server, ic)?;
            }

            Request::SetIcFocus {
                input_method_id,
                input_context_id,
            } => {
                let ic = self
                    .get_input_method(input_method_id)?
                    .get_input_context(input_context_id)?;
                handler.handle_set_focus(server, ic)?;
            }

            Request::UnsetIcFocus {
                input_method_id,
                input_context_id,
            } => {
                let ic = self
                    .get_input_method(input_method_id)?
                    .get_input_context(input_context_id)?;
                handler.handle_unset_focus(server, ic)?;
            }

            Request::PreeditCaretReply {
                input_method_id,
                input_context_id,
                position,
            } => {
                let ic = self
                    .get_input_method(input_method_id)?
                    .get_input_context(input_context_id)?;

                handler.handle_caret(server, ic, position)?;
            }

            Request::PreeditStartReply {
                input_method_id,
                input_context_id,
                return_value: _,
            } => {
                let ic = self
                    .get_input_method(input_method_id)?
                    .get_input_context(input_context_id)?;

                handler.handle_preedit_start(server, ic)?;
            }

            Request::ForwardEvent {
                input_method_id,
                input_context_id,
                serial_number,
                flag: _,
                xev,
            } => {
                let ev = server.deserialize_event(&xev);
                let input_context = self
                    .get_input_method(input_method_id)?
                    .get_input_context(input_context_id)?;
                let consumed = handler.handle_forward_event(server, input_context, &ev)?;

                if !consumed {
                    let flag = if handler.sync_mode() {
                        self.sync = true;
                        ForwardEventFlag::SYNCHRONOUS
                    } else {
                        ForwardEventFlag::empty()
                    };

                    server.send_req(
                        self.client_win,
                        Request::ForwardEvent {
                            input_method_id,
                            input_context_id,
                            flag,
                            serial_number,
                            xev,
                        },
                    )?;
                }

                if is_req_sync {
                    server.send_req(
                        self.client_win,
                        Request::SyncReply {
                            input_method_id,
                            input_context_id,
                        },
                    )?;
                }
            }

            Request::Sync {
                input_method_id,
                input_context_id,
            } => {
                server.send_req(
                    self.client_win,
                    Request::SyncReply {
                        input_method_id,
                        input_context_id,
                    },
                )?;
            }
            Request::SyncReply { .. } => {
                self.process_sync_queue(server, handler)?;
            }
            _ => {
                log::warn!("Unknown request: {:?}", req);
            }
        }

        Ok(())
    }
}

#[derive(Default)]
pub struct SyncQueue {
    pending_out_req: Vec<Request>,
    pending_in_req: VecDeque<Request>,
}

impl SyncQueue {
    pub fn enqueue_write(&mut self, req: Request) {
        self.pending_out_req.push(req);
    }

    pub fn drain_write(&mut self) -> impl Iterator<Item = Request> + '_ {
        self.pending_out_req.drain(..)
    }

    pub fn enqueue_read(&mut self, req: Request) {
        self.pending_in_req.push_back(req);
    }

    pub fn dequeue_read(&mut self) -> Option<Request> {
        self.pending_in_req.pop_front()
    }
}

pub struct XimConnections<T> {
    connections: AHashMap<u32, XimConnection<T>>,
}

impl<T> XimConnections<T> {
    pub fn new() -> Self {
        Self {
            connections: AHashMap::new(),
        }
    }

    pub fn new_connection(&mut self, com_win: u32, client_win: u32) {
        self.connections
            .insert(com_win, XimConnection::new(client_win));
    }

    pub fn get_connection(&mut self, com_win: u32) -> Option<&mut XimConnection<T>> {
        self.connections.get_mut(&com_win)
    }

    pub fn remove_connection(&mut self, com_win: u32) -> Option<XimConnection<T>> {
        self.connections.remove(&com_win)
    }
}
