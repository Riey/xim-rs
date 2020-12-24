use std::collections::HashMap;

use x11rb::connection::Connection;
use xim::{x11rb::X11rbServer, Server, ServerError, ServerHandler};
use xim_parser::InputStyle;

struct InputMethod {
    next: u16,
}

impl InputMethod {
    pub fn new() -> Self {
        Self {}
    }
}

struct ExampleConnection {
    client_win: u32,
    next: u16,
    input_methods: HashMap<u16, InputMethod>,
}

impl ExampleConnection {
    pub fn new(client_win: u32) -> Self {
        Self {
            client_win,
            next: 0,
            input_methods: HashMap::new(),
        }
    }

    pub fn new_im(&mut self) -> u16 {
        let id = self.next;
        self.next += 1;

        self.input_methods.insert(id, InputContext::new());

        id
    }

    pub fn get_im(&mut self, im_id: u16) -> Result<&mut InputMethod, ServerError> {
        self.input_methods
            .get_mut(&im_id)
            .ok_or(ServerError::ClientNotExists)
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
            .insert(com_win, ExampleConnection::new(client_win));
        Ok(())
    }

    fn handle_open(
        &mut self,
        _server: &mut S,
        com_win: u32,
        _locale: xim_parser::bstr::BString,
    ) -> Result<(u32, u16), ServerError> {
        let connection = self.get_connection_mut(com_win)?;
        Ok((connection.client_win, connection.new_im()))
    }

    fn get_client_window(&self, com_win: u32) -> Result<u32, ServerError> {
        self.get_connection(com_win).map(|c| c.client_win)
    }

    fn handle_create_ic(
        &mut self,
        server: &mut S,
        com_win: u32,
        input_method_id: u16,
        input_style: InputStyle,
    ) -> Result<(u32, u16), ServerError> {
        let connection = self.get_connection_mut(com_win)?;
        let ic = connection.get_im(input_method_id)?;
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
