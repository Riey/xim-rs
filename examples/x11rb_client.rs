use x11rb::protocol::xproto::*;
use x11rb::{connection::Connection, COPY_DEPTH_FROM_PARENT};
use xim::x11rb::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    simplelog::TermLogger::init(
        log::LevelFilter::Trace,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stderr,
    )
    .unwrap();

    let (conn, screen_num) = x11rb::connect(None).expect("Connect X");
    let screen = &conn.setup().roots[screen_num];
    let window = conn.generate_id()?;
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        window,
        screen.root,
        0,
        0,
        400,
        400,
        4,
        WindowClass::CopyFromParent,
        screen.root_visual,
        &Default::default(),
    )?;

    let mut client = Client::init(&conn, screen, window, None)?;

    log::info!("Start event loop");

    loop {
        let e = conn.wait_for_event()?;

        log::debug!("Get event: {:?}", e);

        if client.filter_event(&e)? {
            log::trace!("event consumed");
        }
    }
}
