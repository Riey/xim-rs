use x11rb::protocol::{xproto::*, Event};
use x11rb::{connection::Connection, COPY_DEPTH_FROM_PARENT};
use xim::x11rb::{Client, ClientError};
use xim_parser::{ForwardEventFlag, Request};

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
        0,
        &CreateWindowAux::default().background_pixel(screen.black_pixel),
    )?;
    conn.map_window(window)?;
    conn.flush()?;

    let mut client = Client::init(&conn, screen, None)?;

    log::info!("Start event loop");

    let mut end = false;

    while !end {
        let e = conn.wait_for_event()?;

        log::debug!("Get event: {:?}", e);

        if client.filter_event(&e, |client, req| {
            log::trace!("Recv req: {:?}", req);

            match req {
                Request::ConnectReply {
                    server_major_protocol_version: _,
                    server_minor_protocol_version: _,
                } => client.send_req(Request::Open {
                    locale: "en_US".into(),
                }),
                Request::OpenReply {
                    input_method_id,
                    im_attrs,
                    ic_attrs,
                } => {
                    client.set_attrs(im_attrs, ic_attrs);
                    client.send_req(Request::EncodingNegotiation {
                        encodings: vec!["COMPOUND_TEXT".into()],
                        encoding_infos: vec![],
                        input_method_id,
                    })
                }
                Request::EncodingNegotiationReply {
                    category: _,
                    index: _,
                    input_method_id,
                } => client.send_req(Request::CreateIc {
                    input_method_id,
                    ic_attributes: Vec::new(),
                }),
                Request::CreateIcReply {
                    input_method_id,
                    input_context_id,
                } => {
                    log::info!(
                        "IC Created im: {}, ic: {}",
                        input_method_id,
                        input_context_id
                    );
                    let ic_attributes = client
                        .get_ic_attr("preeditAttributes")
                        .into_iter()
                        .collect();
                    client.send_req(Request::GetIcValues {
                        ic_attributes,
                        input_method_id,
                        input_context_id,
                    })
                }
                Request::GetIcValuesReply {
                    input_method_id,
                    input_context_id,
                    ic_attributes,
                } => Ok(()),
                Request::SetEventMask {
                    input_method_id: _,
                    input_context_id: _,
                    forward_event_mask,
                    synchronous_event_mask,
                } => {
                    client.set_event_mask(forward_event_mask, synchronous_event_mask);
                    Ok(())
                }
                Request::CloseReply { input_method_id: _ } => {
                    client.send_req(Request::Disconnect {})
                }
                Request::DisconnectReply {} => {
                    end = true;
                    Ok(())
                }
                Request::Error {
                    input_method_id,
                    input_context_id,
                    flag,
                    detail,
                } => {
                    log::error!("XIM ERROR: {}", detail);
                    Err(ClientError::XimError)
                }
                _ => Err(ClientError::InvalidReply),
            }
        })? {
            log::trace!("event consumed");
        } else if let Event::Error(err) = e {
            return Err(ClientError::X11Error(err).into());
        }
    }

    Ok(())
}
