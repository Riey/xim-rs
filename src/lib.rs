pub mod x11rb;

use std::collections::HashMap;

use xim_parser::{Attribute, Writer, XimFormat};

pub struct AttributeBuilder<'a> {
    id_map: &'a HashMap<String, u16>,
    out: Vec<Attribute>,
}

impl<'a> AttributeBuilder<'a> {
    pub fn push<V: XimFormat>(mut self, name: &str, value: &V) -> Self {
        if let Some(id) = self.id_map.get(name).copied() {
            let mut buf = Vec::with_capacity(value.size());
            value.write(&mut Writer::new(&mut buf));
            self.out.push(Attribute { id, value: buf });
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
}
