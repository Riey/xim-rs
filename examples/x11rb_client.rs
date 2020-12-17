use x11rb::protocol::xproto::*;
use x11rb::{connection::Connection, COPY_DEPTH_FROM_PARENT};
use xim::x11rb::{Client, ClientError};
use xim_parser::{Request, XimString};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    simplelog::TermLogger::init(
        log::LevelFilter::Trace,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stderr,
    )
    .unwrap();

    let (conn, screen_num) = x11rb::connect(None).expect("Connect X");
    let screen = &conn.setup().roots[screen_num];

    let mut client = Client::init(&conn, screen, None)?;

    log::info!("Start event loop");

    let mut end = false;

    while !end {
        let e = conn.wait_for_event()?;

        log::debug!("Get event: {:?}", e);

        if client.filter_event(&e, |client, req| match req {
            Request::ConnectReply {
                server_major_protocol_version: _,
                server_minor_protocol_version: _,
            } => client.send_req(Request::Open {
                locale: XimString(b"kr"),
            }),
            Request::OpenReply {
                input_method_id,
                im_attrs: _,
                ic_attrs: _,
            } => client.send_req(Request::Close { input_method_id }),
            Request::CloseReply { input_method_id: _ } => client.send_req(Request::Disconnect {}),
            Request::DisconnectReply {} => {
                end = true;
                Ok(())
            }
            _ => Err(ClientError::InvalidReply),
        })? {
            log::trace!("event consumed");
        }
    }

    Ok(())
}
