use x11rb::connection::Connection;
use xim::{
    x11rb::X11rbServer, InputContext, Server, ServerCore, ServerError, ServerHandler,
    XimConnections,
};
use xim_parser::InputStyle;

#[derive(Default)]
struct Handler {}

impl Handler {}

impl<S: Server + ServerCore> ServerHandler<S> for Handler {
    type InputContextData = ();
    type InputStyleArray = [InputStyle; 1];

    fn new_ic_data(&mut self) -> Self::InputContextData {
        ()
    }

    fn input_styles(&self) -> Self::InputStyleArray {
        [InputStyle::PREEDITNOTHING | InputStyle::STATUSNOTHING]
    }

    fn handle_connect(&mut self, _server: &mut S) -> Result<(), ServerError> {
        log::info!("Connected!");
        Ok(())
    }

    fn handle_create_ic(
        &mut self,
        server: &mut S,
        input_context: &mut InputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        server.commit(input_context, "가나다")
    }

    fn handle_forward_event(
        &mut self,
        _server: &mut S,
        _input_context: &mut InputContext<Self::InputContextData>,
        _xev: &S::XEvent,
    ) -> Result<bool, ServerError> {
        Ok(true)
    }

    fn handle_destory_ic(&mut self, _input_context: InputContext<Self::InputContextData>) {}

    fn handle_preedit_start(
        &mut self,
        _server: &mut S,
        _input_context: &mut InputContext<Self::InputContextData>,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let (conn, screen_num) = x11rb::rust_connection::RustConnection::connect(None)?;
    let screen = &conn.setup().roots[screen_num];

    let mut server = X11rbServer::init(&conn, screen, "test_server")?;
    let mut connections = XimConnections::new();
    let mut handler = Handler::default();

    loop {
        let e = conn.wait_for_event()?;
        log::trace!("event: {:?}", e);
        server.filter_event(&e, &mut connections, &mut handler)?;
    }
}
