use x11rb::connection::Connection;
use xim::{x11rb::X11rbServer, InputContext, Server, ServerError, ServerHandler, XimConnections};
use xim_parser::InputStyle;

#[derive(Default)]
struct Handler {}

impl Handler {}

impl<S: Server> ServerHandler<S> for Handler {
    type InputContextData = ();
    type InputStyleArray = [InputStyle; 3];

    fn new_ic_data(
        &mut self,
        _server: &mut S,
        _style: InputStyle,
    ) -> Result<Self::InputContextData, ServerError> {
        Ok(())
    }

    fn input_styles(&self) -> Self::InputStyleArray {
        [
            // over-spot
            InputStyle::PREEDIT_NOTHING | InputStyle::STATUS_NOTHING,
            InputStyle::PREEDIT_POSITION | InputStyle::STATUS_NOTHING,
            InputStyle::PREEDIT_POSITION | InputStyle::STATUS_NONE,
        ]
    }

    fn filter_events(&self) -> u32 {
        1
    }

    fn sync_mode(&self) -> bool {
        true
    }

    fn handle_connect(&mut self, _server: &mut S) -> Result<(), ServerError> {
        log::info!("Connected!");
        Ok(())
    }

    fn handle_create_ic(
        &mut self,
        _server: &mut S,
        _input_context: &mut InputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    fn handle_forward_event(
        &mut self,
        server: &mut S,
        input_context: &mut InputContext<Self::InputContextData>,
        _xev: &S::XEvent,
    ) -> Result<bool, ServerError> {
        server.commit(input_context, "가".into())?;
        Ok(true)
    }

    fn handle_destory_ic(
        &mut self,
        _server: &mut S,
        _input_context: InputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    fn handle_preedit_start(
        &mut self,
        _server: &mut S,
        _input_context: &mut InputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    fn handle_caret(
        &mut self,
        _server: &mut S,
        _input_context: &mut InputContext<Self::InputContextData>,
        _position: i32,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    fn handle_reset_ic(
        &mut self,
        _server: &mut S,
        _input_context: &mut InputContext<Self::InputContextData>,
    ) -> Result<String, ServerError> {
        Ok(String::new())
    }

    fn handle_set_ic_values(
        &mut self,
        _server: &mut S,
        _input_context: &mut InputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    fn handle_set_focus(
        &mut self,
        _server: &mut S,
        _input_context: &mut InputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        Ok(())
    }

    fn handle_unset_focus(
        &mut self,
        _server: &mut S,
        _input_context: &mut InputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init_custom_env("XIM_RS_LOG");

    let (conn, screen_num) = x11rb::rust_connection::RustConnection::connect(None)?;
    let mut server = X11rbServer::init(&conn, screen_num, "test_server", xim::ALL_LOCALES)?;
    let mut connections = XimConnections::new();
    let mut handler = Handler::default();

    loop {
        let e = conn.wait_for_event()?;
        server.filter_event(&e, &mut connections, &mut handler)?;
    }
}
