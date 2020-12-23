mod client;
#[cfg(feature = "x11rb-client")]
pub mod x11rb;
// incomplete
#[cfg(feature = "xlib-client")]
pub mod xlib;

use std::collections::HashMap;
use std::iter;

pub use crate::client::{Client, ClientError, ClientHandler};
pub use ctext::{compound_text_to_utf8, utf8_to_compound_text};
use xim_parser::{Attribute, AttributeName, Writer, XimWrite};

pub struct NestedListBuilder<'a> {
    id_map: &'a HashMap<AttributeName, u16>,
    out: &'a mut Vec<u8>,
}

impl<'a> NestedListBuilder<'a> {
    pub fn push<V: XimWrite>(self, name: AttributeName, value: V) -> Self {
        if let Some(id) = self.id_map.get(&name).copied() {
            let mut buf = Vec::new();
            buf.resize(value.size(), 0);
            value.write(&mut Writer::new(&mut buf));
            let attr = Attribute { id, value: buf };
            let from = self.out.len();
            self.out.extend(iter::repeat(0).take(attr.size()));
            value.write(&mut Writer::new(&mut self.out[from..]));
        }

        self
    }
}

pub struct AttributeBuilder<'a> {
    id_map: &'a HashMap<AttributeName, u16>,
    out: Vec<Attribute>,
}

impl<'a> AttributeBuilder<'a> {
    pub(crate) fn new(id_map: &'a HashMap<AttributeName, u16>) -> Self {
        Self {
            id_map,
            out: Vec::new(),
        }
    }

    pub fn push<V: XimWrite>(mut self, name: AttributeName, value: V) -> Self {
        if let Some(id) = self.id_map.get(&name).copied() {
            let mut buf = Vec::new();
            buf.resize(value.size(), 0);
            value.write(&mut Writer::new(&mut buf));
            self.out.push(Attribute { id, value: buf });
        }

        self
    }

    pub fn nested_list(mut self, name: AttributeName, f: impl FnOnce(NestedListBuilder)) -> Self {
        if let Some(id) = self.id_map.get(&name).copied() {
            let mut value = Vec::new();
            f(NestedListBuilder {
                id_map: self.id_map,
                out: &mut value,
            });
            self.out.push(Attribute { id, value });
        }

        self
    }

    pub fn build(self) -> Vec<Attribute> {
        self.out
    }
}

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
