use x11rb::connection::Connection;
use x11rb::protocol::{xproto::*, Event};
use xim::{
    x11rb::{ClientError, X11rbClient},
    Client, ClientHandler,
};
use xim_parser::{AttributeName, ForwardEventFlag, InputStyle, Spot};

#[derive(Default)]
struct ExampleHandler {
    im_id: u16,
    ic_id: u16,
    connected: bool,
    window: u32,
}

impl<C: Client> ClientHandler<C> for ExampleHandler {
    fn handle_connect(&mut self, client: &mut C) -> Result<(), C::Error> {
        log::trace!("Connected");
        self.connected = true;
        client.open(b"en_US")
    }

    fn handle_open(&mut self, client: &mut C, input_method_id: u16) -> Result<(), C::Error> {
        log::trace!("Opened");
        self.im_id = input_method_id;
        let ic_attributes = client
            .build_ic_attributes()
            .push(
                AttributeName::InputStyle,
                InputStyle::PREEDITPOSITION | InputStyle::STATUSNOTHING,
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
        _xev: xim_parser::RawXEvent,
    ) -> Result<(), C::Error> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let (conn, screen_num) = x11rb::connect(None).expect("Connect X");
    let screen = &conn.setup().roots[screen_num];
    let window = conn.generate_id()?;
    conn.create_window(
        screen.root_depth,
        window,
        screen.root,
        0,
        0,
        800,
        600,
        0,
        WindowClass::InputOutput,
        screen.root_visual,
        &CreateWindowAux::default()
            .background_pixel(screen.black_pixel)
            .event_mask(EventMask::KeyPress | EventMask::KeyRelease),
    )?;
    conn.map_window(window)?;
    conn.flush()?;

    let mut client = X11rbClient::init(&conn, screen, None)?;

    log::info!("Start event loop");

    let mut handler = ExampleHandler::default();
    handler.window = window;

    loop {
        let e = conn.wait_for_event()?;

        if client.filter_event(&e, &mut handler)? {
            log::trace!("event consumed");
        } else if let Event::Error(err) = e {
            return Err(ClientError::X11Error(err).into());
        } else {
            match e {
                Event::KeyPress(e) | Event::KeyRelease(e) => {
                    if handler.connected {
                        client.forward_event(
                            handler.im_id,
                            handler.ic_id,
                            ForwardEventFlag::empty(),
                            e,
                        )?;
                    }
                }
                _ => {}
            }
        }
    }
}
