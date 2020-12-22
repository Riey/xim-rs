use std::collections::HashMap;

use crate::{
    client::{ClientCore, ClientHandler},
    Atoms,
};
use thiserror::Error;
use x11_dl::xlib;
use xim_parser::{bstr::BString, AttributeName};

#[derive(Debug, Error)]
pub enum XlibError {
    #[error("Can't read xim message {0}")]
    ReadProtocol(#[from] xim_parser::ReadError),
    #[error("Server send error code: {0:?}, detail: {1}")]
    XimError(xim_parser::ErrorCode, BString),
    #[error("Server Transport is not supported")]
    UnsupportedTransport,
    #[error("Invalid reply from server")]
    InvalidReply,
    #[error("Can't connect xim server")]
    NoXimServer,
}

impl ClientCore for XlibClient {
    type Error = XlibError;
    type XEvent = xlib::XKeyEvent;

    #[inline]
    fn ic_attributes(&self) -> &HashMap<AttributeName, u16> {
        &self.ic_attributes
    }

    #[inline]
    fn im_attributes(&self) -> &HashMap<AttributeName, u16> {
        &self.im_attributes
    }

    #[inline]
    fn serialize_event(&self, xev: Self::XEvent) -> xim_parser::XEvent {
        xim_parser::XEvent {
            response_type: xev.type_ as u8,
            detail: xev.keycode as u8,
            sequence: xev.serial as _,
            time: xev.time as u32,
            root: xev.root as u32,
            event: xev.window as u32,
            child: xev.subwindow as u32,
            root_x: xev.x_root as i16,
            root_y: xev.y_root as i16,
            event_x: xev.x as i16,
            event_y: xev.y as i16,
            state: xev.state as u16,
            same_screen: xev.same_screen != 0,
        }
    }

    #[inline]
    fn deserialize_event(&self, xev: xim_parser::XEvent) -> Self::XEvent {
        xlib::XKeyEvent {
            type_: xev.response_type as _,
            keycode: xev.detail as _,
            serial: xev.sequence as _,
            time: xev.time as _,
            root: xev.root as _,
            window: xev.event as _,
            subwindow: xev.child as _,
            x_root: xev.root_x as _,
            y_root: xev.root_y as _,
            x: xev.event_x as _,
            y: xev.event_y as _,
            state: xev.state as _,
            same_screen: xev.same_screen as i32,
            display: self.display,
            send_event: 0,
        }
    }

    #[inline]
    fn send_req(&mut self, req: xim_parser::Request) -> Result<(), Self::Error> {
        todo!()
    }
}

pub struct XlibClient {
    xlib: xlib::Xlib,
    display: *mut xlib::Display,
    im_attributes: HashMap<AttributeName, u16>,
    ic_attributes: HashMap<AttributeName, u16>,
}
