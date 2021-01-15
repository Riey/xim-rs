use xim::{AHashMap, Client, ClientError, ClientHandler};
use xim_parser::{AttributeName, InputStyle, Point};

#[derive(Default)]
pub struct ExampleHandler {
    pub im_id: u16,
    pub ic_id: u16,
    pub connected: bool,
    pub window: u32,
}

impl<C: Client> ClientHandler<C> for ExampleHandler {
    fn handle_connect(&mut self, client: &mut C) -> Result<(), ClientError> {
        log::trace!("Connected");
        client.open("en_US")
    }

    fn handle_open(&mut self, client: &mut C, input_method_id: u16) -> Result<(), ClientError> {
        log::trace!("Opened");
        self.im_id = input_method_id;

        client.get_im_values(input_method_id, &[AttributeName::QueryInputStyle])
    }

    fn handle_get_im_values(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        mut attributes: AHashMap<AttributeName, Vec<u8>>,
    ) -> Result<(), ClientError> {
        log::trace!(
            "Query: {:?}",
            xim_parser::write_to_vec(xim_parser::Request::GetImValuesReply {
                input_method_id,
                im_attributes: vec![xim_parser::Attribute {
                    id: 0,
                    value: attributes.remove(&AttributeName::QueryInputStyle).unwrap(),
                }]
            })
        );
        let ic_attributes = client
            .build_ic_attributes()
            .push(
                AttributeName::InputStyle,
                InputStyle::PREEDIT_NOTHING | InputStyle::STATUS_NOTHING,
            )
            .push(AttributeName::ClientWindow, self.window)
            .push(AttributeName::FocusWindow, self.window)
            .nested_list(AttributeName::PreeditAttributes, |b| {
                b.push(AttributeName::SpotLocation, Point { x: 0, y: 0 });
            })
            .build();
        client.create_ic(input_method_id, ic_attributes)
    }

    fn handle_query_extension(
        &mut self,
        _client: &mut C,
        _extensions: &[xim_parser::Extension],
    ) -> Result<(), ClientError> {
        Ok(())
    }

    fn handle_create_ic(
        &mut self,
        _client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
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
    ) -> Result<(), ClientError> {
        log::info!("Commited {}", text);
        Ok(())
    }

    fn handle_disconnect(&mut self) {
        log::info!("disconnected");
    }

    fn handle_close(&mut self, client: &mut C, _input_method_id: u16) -> Result<(), ClientError> {
        log::info!("closed");
        client.disconnect()
    }

    fn handle_destory_ic(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        _input_context_id: u16,
    ) -> Result<(), ClientError> {
        client.close(input_method_id)
    }

    fn handle_forward_event(
        &mut self,
        _client: &mut C,
        _input_method_id: u16,
        _input_context_id: u16,
        _flag: xim_parser::ForwardEventFlag,
        _xev: C::XEvent,
    ) -> Result<(), ClientError> {
        Ok(())
    }

    fn handle_set_ic_values(
        &mut self,
        _client: &mut C,
        _input_method_id: u16,
        _input_context_id: u16,
    ) -> Result<(), ClientError> {
        Ok(())
    }

    fn handle_set_event_mask(
        &mut self,
        _client: &mut C,
        _input_method_id: u16,
        _input_context_id: u16,
        _forward_event_mask: u32,
        _synchronous_event_mask: u32,
    ) -> Result<(), ClientError> {
        Ok(())
    }
}
