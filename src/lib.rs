#[cfg(feature = "client")]
mod client;
#[cfg(feature = "server")]
mod server;

#[cfg(any(feature = "x11rb-server", feature = "x11rb-client"))]
mod encoding;

#[cfg(any(feature = "x11rb-server", feature = "x11rb-client"))]
pub mod x11rb;
#[cfg(feature = "xlib-client")]
pub mod xlib;

#[cfg(feature = "client")]
pub use crate::client::{Client, ClientError, ClientHandler};
#[cfg(feature = "server")]
pub use crate::server::{
    InputContext, InputMethod, Server, ServerCore, ServerError, ServerHandler, XimConnection,
    XimConnections,
};
pub use ahash::AHashMap;
pub use xim_parser::*;

#[allow(non_snake_case)]
#[derive(Copy, Clone, Debug)]
struct Atoms<Atom> {
    XIM_SERVERS: Atom,
    LOCALES: Atom,
    TRANSPORT: Atom,
    XIM_XCONNECT: Atom,
    XIM_PROTOCOL: Atom,
    DATA: Atom,
}

impl<Atom> Atoms<Atom> {
    #[allow(unused)]
    pub fn new<E, F>(f: F) -> Result<Self, E>
    where
        F: Fn(&'static str) -> Result<Atom, E>,
    {
        Ok(Self {
            XIM_SERVERS: f("XIM_SERVERS")?,
            LOCALES: f("LOCALES")?,
            TRANSPORT: f("TRANSPORT")?,
            XIM_XCONNECT: f("_XIM_XCONNECT")?,
            XIM_PROTOCOL: f("_XIM_PROTOCOL")?,
            DATA: f("XIM_RS_DATA")?,
        })
    }

    #[allow(unused)]
    pub fn new_null<E, F>(f: F) -> Result<Self, E>
    where
        F: Fn(&'static str) -> Result<Atom, E>,
    {
        Ok(Self {
            XIM_SERVERS: f("XIM_SERVERS\0")?,
            LOCALES: f("LOCALES\0")?,
            TRANSPORT: f("TRANSPORT\0")?,
            XIM_XCONNECT: f("_XIM_XCONNECT\0")?,
            XIM_PROTOCOL: f("_XIM_PROTOCOL\0")?,
            DATA: f("XIM_RS_DATA\0")?,
        })
    }
}
