use std::collections::HashMap;
use std::ffi::CStr;
use std::mem::MaybeUninit;

use crate::{
    client::{ClientCore, ClientHandler},
    Atoms,
};
use thiserror::Error;
use x11_dl::xlib;
use xim_parser::{bstr::BString, AttributeName};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Can't Intern atom")]
    InternAtomError,
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

impl<X: XlibRef> ClientCore for XlibClient<X> {
    type Error = ClientError;
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

impl<'a> XlibRef for &'a xlib::Xlib {
    fn xlib(&self) -> &xlib::Xlib {
        self
    }
}

impl XlibRef for xlib::Xlib {
    fn xlib(&self) -> &xlib::Xlib {
        self
    }
}

pub trait XlibRef {
    fn xlib(&self) -> &xlib::Xlib;
}

pub struct XlibClient<X: XlibRef> {
    x: X,
    display: *mut xlib::Display,
    im_window: xlib::Window,
    server_owner_window: xlib::Window,
    server_atom: xlib::Atom,
    atoms: Atoms<xlib::Atom>,
    transport_max: usize,
    client_window: xlib::Window,
    im_attributes: HashMap<AttributeName, u16>,
    ic_attributes: HashMap<AttributeName, u16>,
    forward_event_mask: u32,
    synchronous_event_mask: u32,
    buf: Vec<u8>,
}

impl<X: XlibRef> XlibClient<X> {
    pub unsafe fn init(
        x: X,
        display: *mut xlib::Display,
        im_name: Option<&str>,
    ) -> Result<Self, ClientError> {
        let xlib = x.xlib();
        let root = (xlib.XDefaultRootWindow)(display);
        let client_window = (xlib.XCreateSimpleWindow)(display, root, 0, 0, 1, 1, 0, 0, 0);

        let var = std::env::var("XMODIFIERS").ok();
        let var = var.as_ref().and_then(|n| n.strip_prefix("@im="));
        let im_name = im_name.or(var).ok_or(ClientError::NoXimServer)?;

        let atoms = Atoms::new_null::<ClientError, _>(|name| {
            let atom = (xlib.XInternAtom)(display, name.as_ptr() as *const _, 0);
            if atom == 0 {
                Err(ClientError::InternAtomError)
            } else {
                Ok(atom)
            }
        })?;

        let mut act_ty = MaybeUninit::uninit();
        let mut act_format = MaybeUninit::uninit();
        let mut items = MaybeUninit::uninit();
        let mut bytes = MaybeUninit::uninit();
        let mut prop = MaybeUninit::uninit();

        let code = (xlib.XGetWindowProperty)(
            display,
            root,
            atoms.XIM_SERVERS,
            0,
            8196,
            xlib::False,
            xlib::XA_ATOM,
            act_ty.as_mut_ptr(),
            act_format.as_mut_ptr(),
            items.as_mut_ptr(),
            bytes.as_mut_ptr(),
            prop.as_mut_ptr(),
        );

        if code != 0 {
            return Err(ClientError::InvalidReply);
        }

        let ty = act_ty.assume_init();
        let format = act_format.assume_init();
        let items = items.assume_init();
        let bytes = bytes.assume_init();
        let prop = prop.assume_init() as *mut u32;

        if ty != xlib::XA_ATOM || format != 32 {
            Err(ClientError::InvalidReply)
        } else {
            for i in 0..items {
                let server_atom = prop.add(i as usize).read() as xlib::Atom;
                let server_owner = (xlib.XGetSelectionOwner)(display, server_atom);
                let name_ptr = (xlib.XGetAtomName)(display, server_atom);
                let name = CStr::from_ptr(name_ptr);
                let name = match name.to_str() {
                    Ok(s) => s,
                    _ => continue,
                };

                if let Some(name) = name.strip_prefix("@server=") {
                    if name == im_name {
                        (xlib.XConvertSelection)(
                            display,
                            server_atom,
                            atoms.TRANSPORT,
                            atoms.TRANSPORT,
                            client_window,
                            xlib::CurrentTime,
                        );
                        (xlib.XFlush)(display);
                        (xlib.XFree)(name_ptr as _);
                        (xlib.XFree)(prop as _);

                        return Ok(Self {
                            atoms,
                            client_window,
                            server_atom,
                            server_owner_window: server_owner,
                            im_window: 0,
                            forward_event_mask: 0,
                            synchronous_event_mask: 0,
                            transport_max: 0,
                            display,
                            x,
                            ic_attributes: HashMap::new(),
                            im_attributes: HashMap::new(),
                            buf: Vec::with_capacity(1024),
                        });
                    }
                } else {
                    (xlib.XFree)(name_ptr as _);
                }
            }

            (xlib.XFree)(prop as _);

            Err(ClientError::NoXimServer)
        }
    }
}
