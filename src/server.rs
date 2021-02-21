mod connection;

use std::num::NonZeroU16;

use xim_parser::{
    CaretDirection, CaretStyle, CommitData, ErrorCode, ErrorFlag, InputStyle, PreeditDrawStatus,
    Request,
};

pub use self::connection::{
    InputContext, InputMethod, UserInputContext, XimConnection, XimConnections,
};

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Client doesn't exists")]
    ClientNotExists,
    #[error("Can't read xim message {0}")]
    ReadProtocol(#[from] xim_parser::ReadError),
    #[error("Client send error code: {0:?}, detail: {1}")]
    XimError(xim_parser::ErrorCode, String),
    #[error("Invalid reply from client")]
    InvalidReply,
    #[error("Internal error: {0}")]
    Internal(String),
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

pub trait ServerHandler<S: Server> {
    type InputStyleArray: AsRef<[InputStyle]>;
    type InputContextData;

    fn new_ic_data(
        &mut self,
        server: &mut S,
        input_style: InputStyle,
    ) -> Result<Self::InputContextData, ServerError>;

    fn input_styles(&self) -> Self::InputStyleArray;
    fn filter_events(&self) -> u32;

    fn handle_connect(&mut self, server: &mut S) -> Result<(), ServerError>;

    fn handle_create_ic(
        &mut self,
        server: &mut S,
        user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError>;

    fn handle_destory_ic(
        &mut self,
        server: &mut S,
        user_ic: UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError>;
    fn handle_reset_ic(
        &mut self,
        server: &mut S,
        user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<String, ServerError>;

    fn handle_set_focus(
        &mut self,
        server: &mut S,
        user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError>;

    fn handle_unset_focus(
        &mut self,
        server: &mut S,
        user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError>;

    fn handle_set_ic_values(
        &mut self,
        server: &mut S,
        user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError>;

    fn handle_caret(
        &mut self,
        server: &mut S,
        user_ic: &mut UserInputContext<Self::InputContextData>,
        position: i32,
    ) -> Result<(), ServerError>;

    fn handle_preedit_start(
        &mut self,
        server: &mut S,
        user_ic: &mut UserInputContext<Self::InputContextData>,
    ) -> Result<(), ServerError>;

    /// return `false` when event back to client
    /// if return `true` it consumed and don't back to client
    fn handle_forward_event(
        &mut self,
        server: &mut S,
        user_ic: &mut UserInputContext<Self::InputContextData>,
        xev: &S::XEvent,
    ) -> Result<bool, ServerError>;
}

pub trait Server {
    type XEvent;

    fn error(
        &mut self,
        client_win: u32,
        code: ErrorCode,
        detail: String,
        input_method_id: Option<NonZeroU16>,
        user_ic_id: Option<NonZeroU16>,
    ) -> Result<(), ServerError>;

    fn preedit_caret(
        &mut self,
        ic: &InputContext,
        position: i32,
        direction: CaretDirection,
        style: CaretStyle,
    ) -> Result<(), ServerError>;
    fn preedit_start(&mut self, ic: &InputContext) -> Result<(), ServerError>;
    fn preedit_draw(&mut self, ic: &InputContext, s: &str) -> Result<(), ServerError>;
    fn preedit_done(&mut self, ic: &InputContext) -> Result<(), ServerError>;
    fn commit(&mut self, ic: &InputContext, s: &str) -> Result<(), ServerError>;

    fn set_event_mask(
        &mut self,
        ic: &InputContext,
        forward_event_mask: u32,
        synchronous_event_mask: u32,
    ) -> Result<(), ServerError>;
}

impl<S: ServerCore> Server for S {
    type XEvent = S::XEvent;

    fn error(
        &mut self,
        client_win: u32,
        code: ErrorCode,
        detail: String,
        input_method_id: Option<NonZeroU16>,
        user_ic_id: Option<NonZeroU16>,
    ) -> Result<(), ServerError> {
        let mut flag = ErrorFlag::empty();

        let input_method_id = if let Some(id) = input_method_id {
            flag |= ErrorFlag::INPUT_METHOD_ID_VALID;
            id.get()
        } else {
            0
        };

        let input_context_id = if let Some(id) = user_ic_id {
            flag |= ErrorFlag::INPUT_CONTEXT_ID_VALID;
            id.get()
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

    fn preedit_caret(
        &mut self,
        ic: &InputContext,
        position: i32,
        direction: CaretDirection,
        style: CaretStyle,
    ) -> Result<(), ServerError> {
        self.send_req(
            ic.client_win(),
            Request::PreeditCaret {
                input_method_id: ic.input_method_id().get(),
                input_context_id: ic.input_context_id().get(),
                direction,
                position,
                style,
            },
        )
    }

    fn preedit_start(&mut self, ic: &InputContext) -> Result<(), ServerError> {
        self.send_req(
            ic.client_win(),
            Request::PreeditStart {
                input_method_id: ic.input_method_id().get(),
                input_context_id: ic.input_context_id().get(),
            },
        )
    }

    fn preedit_done(&mut self, ic: &InputContext) -> Result<(), ServerError> {
        self.send_req(
            ic.client_win(),
            Request::PreeditDone {
                input_method_id: ic.input_method_id().get(),
                input_context_id: ic.input_context_id().get(),
            },
        )
    }

    fn preedit_draw(&mut self, ic: &InputContext, s: &str) -> Result<(), ServerError> {
        let preedit = ctext::utf8_to_compound_text(s);

        self.send_req(
            ic.client_win(),
            Request::PreeditDraw {
                input_method_id: ic.input_method_id().get(),
                input_context_id: ic.input_context_id().get(),
                chg_first: 0,
                chg_length: s.chars().count() as i32,
                caret: 0,
                preedit_string: preedit,
                status: PreeditDrawStatus::NO_FEEDBACK,
                feedbacks: Vec::new(),
            },
        )
    }

    fn commit(&mut self, ic: &InputContext, s: &str) -> Result<(), ServerError> {
        self.send_req(
            ic.client_win(),
            Request::Commit {
                input_method_id: ic.input_method_id().get(),
                input_context_id: ic.input_context_id().get(),
                data: CommitData::Chars {
                    commited: ctext::utf8_to_compound_text(s),
                    syncronous: false,
                },
            },
        )
    }

    fn set_event_mask(
        &mut self,
        ic: &InputContext,
        forward_event_mask: u32,
        synchronous_event_mask: u32,
    ) -> Result<(), ServerError> {
        self.send_req(
            ic.client_win(),
            Request::SetEventMask {
                input_method_id: ic.input_method_id().get(),
                input_context_id: ic.input_context_id().get(),
                forward_event_mask,
                synchronous_event_mask,
            },
        )
    }
}

pub trait ServerCore {
    type XEvent;

    fn deserialize_event(&self, ev: &xim_parser::XEvent) -> Self::XEvent;
    fn send_req(&mut self, client_win: u32, req: Request) -> Result<(), ServerError>;
}
