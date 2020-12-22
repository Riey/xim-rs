use std::{mem::MaybeUninit, ptr};
use x11_dl::xlib;
use xim::{
    x11rb::{ClientError, X11rbClient},
    xlib::XlibClient,
    Client,
};
use xim_parser::ForwardEventFlag;

use self::handler::ExampleHandler;

#[path = "util/handler.rs"]
mod handler;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let xlib = xlib::Xlib::open()?;

    unsafe {
        let display = (xlib.XOpenDisplay)(ptr::null());
        let root = (xlib.XDefaultRootWindow)(display);
        let window = (xlib.XCreateSimpleWindow)(display, root, 0, 0, 800, 600, 0, 0, 0);
        (xlib.XMapWindow)(display, window);
        (xlib.XFlush)(display);

        // this is not working now!
        let mut client = XlibClient::init(&xlib, display, None)?;

        log::info!("Start event loop");

        let mut handler = ExampleHandler::default();
        handler.window = window as u32;

        loop {
            let mut e = MaybeUninit::uninit();
            (xlib.XNextEvent)(display, e.as_mut_ptr());
            let e = e.assume_init();

            log::trace!("Get event: {:?}", e);

            // if client.filter_event(&e, &mut handler)? {
            //     continue;
            // } else if let Event::Error(err) = e {
            //     return Err(ClientError::X11Error(err).into());
            // } else {
            //     match e {
            //         Event::KeyPress(e) | Event::KeyRelease(e) => {
            //             if handler.connected {
            //                 client.forward_event(
            //                     handler.im_id,
            //                     handler.ic_id,
            //                     ForwardEventFlag::SYNCHRONOUS,
            //                     e,
            //                 )?;
            //             }
            //         }
            //         _ => {}
            //     }
            // }
        }
    }
}
