use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::{collections::HashMap, convert::TryInto, os::raw::c_long};

use crate::{
    client::{handle_request, ClientCore, ClientHandler},
    Atoms,
};
use thiserror::Error;
use x11_dl::xlib;
use xim_parser::{bstr::BString, AttributeName, Request, XimWrite};

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
        self.send_req_impl(req)
    }

    fn xim_error(&self, code: xim_parser::ErrorCode, detail: BString) -> Self::Error {
        ClientError::XimError(code, detail)
    }

    fn set_attrs(&mut self, ic_attrs: Vec<xim_parser::Attr>, im_attrs: Vec<xim_parser::Attr>) {
        for im_attr in im_attrs {
            self.im_attributes.insert(im_attr.name, im_attr.id);
        }

        for ic_attr in ic_attrs {
            self.ic_attributes.insert(ic_attr.name, ic_attr.id);
        }
    }

    fn set_event_mask(&mut self, forward_event_mask: u32, synchronous_event_mask: u32) {
        self.forward_event_mask = forward_event_mask;
        self.synchronous_event_mask = synchronous_event_mask;
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

        let mut ty = MaybeUninit::uninit();
        let mut format = MaybeUninit::uninit();
        let mut items = MaybeUninit::uninit();
        let mut bytes = MaybeUninit::uninit();
        let mut prop = MaybeUninit::uninit();

        let code = (xlib.XGetWindowProperty)(
            display,
            root,
            atoms.XIM_SERVERS,
            0,
            i64::MAX,
            xlib::False,
            xlib::XA_ATOM,
            ty.as_mut_ptr(),
            format.as_mut_ptr(),
            items.as_mut_ptr(),
            bytes.as_mut_ptr(),
            prop.as_mut_ptr(),
        );

        if code != 0 {
            return Err(ClientError::InvalidReply);
        }

        let ty = ty.assume_init();
        let format = format.assume_init();
        let items = items.assume_init();
        let bytes = bytes.assume_init();
        let prop = prop.assume_init() as *mut xlib::Atom;

        if ty != xlib::XA_ATOM || format != 32 {
            Err(ClientError::InvalidReply)
        } else {
            for i in 0..items {
                let server_atom = prop.add(i as usize).read();
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

    pub unsafe fn filter_event(
        &mut self,
        e: &xlib::XEvent,
        handler: &mut impl ClientHandler<Self>,
    ) -> Result<bool, ClientError> {
        match e.get_type() {
            xlib::SelectionNotify if e.selection.requestor == self.client_window => {
                let mut ty = MaybeUninit::uninit();
                let mut format = MaybeUninit::uninit();
                let mut items = MaybeUninit::uninit();
                let mut bytes = MaybeUninit::uninit();
                let mut prop = MaybeUninit::uninit();
                (self.x.xlib().XGetWindowProperty)(
                    self.display,
                    self.client_window,
                    self.atoms.TRANSPORT,
                    0,
                    i64::MAX,
                    xlib::True,
                    self.atoms.TRANSPORT,
                    ty.as_mut_ptr(),
                    format.as_mut_ptr(),
                    items.as_mut_ptr(),
                    bytes.as_mut_ptr(),
                    prop.as_mut_ptr(),
                );

                let _ty = ty.assume_init();
                let _format = format.assume_init();
                let items = items.assume_init();
                let _bytes = bytes.assume_init();
                let prop = prop.assume_init();

                if e.selection.property == dbg!(self.atoms.LOCALES) {
                    log::trace!("Get LOCALES");
                    // TODO: set locale
                    self.xconnect()?;
                } else if e.selection.property == self.atoms.TRANSPORT {
                    log::trace!("Get TRANSPORT");

                    let transport = std::slice::from_raw_parts(prop, items as usize);

                    if !transport.starts_with(b"@transport=X/") {
                        return Err(ClientError::UnsupportedTransport);
                    }

                    (self.x.xlib().XConvertSelection)(
                        self.display,
                        self.server_atom,
                        self.atoms.LOCALES,
                        self.atoms.LOCALES,
                        self.client_window,
                        xlib::CurrentTime,
                    );
                }

                (self.x.xlib().XFree)(prop as _);

                Ok(true)
            }
            xlib::ClientMessage if e.client_message.window == self.client_window => {
                if e.client_message.message_type == self.atoms.XIM_XCONNECT as _ {
                    let [im_window, major, minor, max, _]: [c_long; 5] =
                        e.client_message.data.as_longs().try_into().unwrap();

                    log::info!(
                        "XConnected server on {}, transport version: {}.{}, TRANSPORT_MAX: {}",
                        im_window,
                        major,
                        minor,
                        max
                    );

                    self.im_window = im_window as xlib::Window;
                    self.transport_max = max as usize;
                    self.send_req(Request::Connect {
                        client_major_protocol_version: 1,
                        client_minor_protocol_version: 0,
                        endian: xim_parser::Endian::Native,
                        client_auth_protocol_names: Vec::new(),
                    })?;

                    Ok(true)
                } else if e.client_message.message_type == self.atoms.XIM_PROTOCOL {
                    self.handle_xim_protocol(&e.client_message, handler)?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }

    fn handle_xim_protocol(
        &mut self,
        msg: &xlib::XClientMessageEvent,
        handler: &mut impl ClientHandler<Self>,
    ) -> Result<(), ClientError> {
        if msg.format == 32 {
            let length = msg.data.get_long(0);
            let atom = msg.data.get_long(1);

            let mut ty = MaybeUninit::uninit();
            let mut format = MaybeUninit::uninit();
            let mut items = MaybeUninit::uninit();
            let mut bytes = MaybeUninit::uninit();
            let mut prop = MaybeUninit::uninit();

            unsafe {
                let code = (self.x.xlib().XGetWindowProperty)(
                    self.display,
                    msg.window,
                    atom as _,
                    0,
                    length,
                    xlib::True,
                    0,
                    ty.as_mut_ptr(),
                    format.as_mut_ptr(),
                    items.as_mut_ptr(),
                    bytes.as_mut_ptr(),
                    prop.as_mut_ptr(),
                );

                if code != 0 {
                    return Err(ClientError::InvalidReply);
                }

                let _ty = ty.assume_init();
                let _format = format.assume_init();
                let items = items.assume_init();
                let _bytes = bytes.assume_init();
                let prop = prop.assume_init();

                let data = std::slice::from_raw_parts(prop, items as usize);

                let req = xim_parser::read(data)?;

                handle_request(self, handler, req)?;

                (self.x.xlib().XFree)(prop as _);
            }
        } else if msg.format == 8 {
            let bytes = msg.data.as_bytes();
            let data: &[u8] = unsafe { std::slice::from_raw_parts(bytes.as_ptr() as _, bytes.len()) };
            let req = xim_parser::read(data)?;
            handle_request(self, handler, req)?;
        }

        Ok(())
    }

    fn xconnect(&mut self) -> Result<(), ClientError> {
        let mut ev = xlib::XClientMessageEvent {
            display: self.display,
            data: [self.client_window, 0, 0, 0, 0].into(),
            format: 32,
            message_type: self.atoms.XIM_XCONNECT,
            serial: 0,
            type_: xlib::ClientMessage,
            send_event: xlib::True,
            window: self.server_owner_window,
        }
        .into();

        unsafe {
            log::trace!("Send event");
            (self.x.xlib().XSendEvent)(
                self.display,
                self.server_owner_window,
                xlib::False,
                xlib::NoEventMask,
                &mut ev,
            );

            (self.x.xlib().XFlush)(self.display);
        }
        Ok(())
    }

    fn send_req_impl(&mut self, req: Request) -> Result<(), ClientError> {
        self.buf.resize(req.size(), 0);
        xim_parser::write(&req, &mut self.buf);

        if self.buf.len() < self.transport_max {
            if self.buf.len() > 20 {
                todo!("multi-CM");
            }
            self.buf.resize(20, 0);
            let buf: [u8; 20] = self.buf.as_slice().try_into().unwrap();
            let mut ev = xlib::XClientMessageEvent {
                type_: xlib::ClientMessage,
                display: self.display,
                message_type: self.atoms.XIM_PROTOCOL,
                data: buf.into(),
                format: 8,
                serial: 0,
                send_event: xlib::True,
                window: self.im_window,
            }
            .into();
            unsafe {
                (self.x.xlib().XSendEvent)(
                    self.display,
                    self.im_window,
                    xlib::False,
                    xlib::NoEventMask,
                    &mut ev,
                );
            }
        } else {
            unsafe {
                (self.x.xlib().XChangeProperty)(
                    self.display,
                    self.im_window,
                    self.atoms.DATA,
                    xlib::XA_STRING,
                    8,
                    xlib::PropModeAppend,
                    self.buf.as_ptr(),
                    self.buf.len() as _,
                );
            }
            let mut ev = xlib::XClientMessageEvent {
                type_: xlib::ClientMessage,
                display: self.display,
                message_type: self.atoms.XIM_PROTOCOL,
                data: [self.buf.len() as _, self.atoms.DATA, 0, 0, 0].into(),
                format: 32,
                serial: 0,
                send_event: xlib::True,
                window: self.im_window,
            }
            .into();
            unsafe {
                (self.x.xlib().XSendEvent)(
                    self.display,
                    self.im_window,
                    xlib::False,
                    xlib::NoEventMask,
                    &mut ev,
                );
            }
        }
        self.buf.clear();

        Ok(())
    }
}
