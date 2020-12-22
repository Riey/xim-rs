use xim::{Client, ClientHandler};
use xim_parser::{AttributeName, InputStyle, Spot};

#[derive(Default)]
pub struct ExampleHandler {
    pub im_id: u16,
    pub ic_id: u16,
    pub connected: bool,
    pub window: u32,
}

impl<C: Client> ClientHandler<C> for ExampleHandler {
    fn handle_connect(&mut self, client: &mut C) -> Result<(), C::Error> {
        log::trace!("Connected");
        client.open(b"en_US")
    }

    fn handle_open(&mut self, client: &mut C, input_method_id: u16) -> Result<(), C::Error> {
        log::trace!("Opened");
        self.im_id = input_method_id;
        let ic_attributes = client
            .build_ic_attributes()
            .push(
                AttributeName::InputStyle,
                InputStyle::PREEDITNOTHING | InputStyle::STATUSNOTHING,
            )
            .push(AttributeName::ClientWindow, self.window)
            .push(AttributeName::FocusWindow, self.window)
            .nested_list(AttributeName::PreeditAttributes, |b| {
                b.push(AttributeName::SpotLocation, Spot { x: 0, y: 0 });
            })
            .build();
        client.create_ic(input_method_id, ic_attributes)
    }

    fn handle_query_extension(
        &mut self,
        _client: &mut C,
        _extensions: &[xim_parser::Extension],
    ) -> Result<(), C::Error> {
        Ok(())
    }

    fn handle_create_ic(
        &mut self,
        _client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), C::Error> {
        self.connected = true;
        self.ic_id = input_context_id;
        log::info!("IC created {}, {}", input_method_id, input_context_id);
        Ok(())
    }

    fn handle_commit(
        &mut self,
        _client: &mut C,
        _input_method_id: u16,
        _input_context_id: u16,
        text: &str,
    ) -> Result<(), C::Error> {
        log::info!("Commited {}", text);
        Ok(())
    }

    fn handle_disconnect(&mut self) {
        log::info!("disconnected");
    }

    fn handle_close(&mut self, client: &mut C, _input_method_id: u16) -> Result<(), C::Error> {
        log::info!("closed");
        client.disconnect()
    }

    fn handle_destory_ic(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        _input_context_id: u16,
    ) -> Result<(), C::Error> {
        client.close(input_method_id)
    }

    fn handle_forward_event(
        &mut self,
        _client: &mut C,
        _input_method_id: u16,
        _input_context_id: u16,
        _flag: xim_parser::ForwardEventFlag,
        _xev: C::XEvent,
    ) -> Result<(), C::Error> {
        Ok(())
    }
}
