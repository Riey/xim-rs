use std::collections::HashMap;

use x11rb::connection::Connection;
use xim::{x11rb::X11rbServer, Server, ServerError, ServerHandler};
use xim_parser::InputStyle;

struct InputContext {}

impl InputContext {
    pub fn new() -> Self {
        Self {}
    }
}

struct ExampleConnection {
    #[allow(unused)]
    com_win: u32,
    client_win: u32,
    next: u16,
    input_methods: HashMap<u16, InputContext>,
}

impl ExampleConnection {
    pub fn new(com_win: u32, client_win: u32) -> Self {
        Self {
            com_win,
            client_win,
            next: 0,
            input_methods: HashMap::new(),
        }
    }

    pub fn open_im(&mut self) -> u16 {
        let id = self.next;
        self.next += 1;

        self.input_methods.insert(id, InputContext::new());

        id
    }
}

#[derive(Default)]
struct Handler {
    connections: HashMap<u32, ExampleConnection>,
}

impl Handler {
    pub fn get_connection(&self, com_win: u32) -> Result<&ExampleConnection, ServerError> {
        self.connections
            .get(&com_win)
            .ok_or(ServerError::ClientNotExists)
    }

    pub fn get_connection_mut(
        &mut self,
        com_win: u32,
    ) -> Result<&mut ExampleConnection, ServerError> {
        self.connections
            .get_mut(&com_win)
            .ok_or(ServerError::ClientNotExists)
    }
}

impl<S: Server> ServerHandler<S> for Handler {
    type InputStyleArray = [InputStyle; 1];

    fn input_styles(&self) -> Self::InputStyleArray {
        [InputStyle::PREEDITNOTHING | InputStyle::STATUSNOTHING]
    }

    fn handle_xconnect(
        &mut self,
        _server: &mut S,
        com_win: u32,
        client_win: u32,
    ) -> Result<(), ServerError> {
        log::info!("XConnected");
        self.connections
            .insert(com_win, ExampleConnection::new(com_win, client_win));
        Ok(())
    }

    fn handle_open(
        &mut self,
        _server: &mut S,
        com_win: u32,
        _locale: xim_parser::bstr::BString,
    ) -> Result<(u32, u16), ServerError> {
        let connection = self.get_connection_mut(com_win)?;
        Ok((connection.client_win, connection.open_im()))
    }

    fn get_client_window(&self, com_win: u32) -> Result<u32, ServerError> {
        self.get_connection(com_win).map(|c| c.client_win)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let (conn, screen_num) = x11rb::rust_connection::RustConnection::connect(None)?;
    let screen = &conn.setup().roots[screen_num];

    let mut server = X11rbServer::init(&conn, screen, "test_server")?;
    let mut handler = Handler::default();

    loop {
        let e = conn.wait_for_event()?;
        log::trace!("event: {:?}", e);
        server.filter_event(&e, &mut handler)?;
    }
}
