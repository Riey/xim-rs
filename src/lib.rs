pub mod x11rb;

use std::collections::HashMap;
use std::iter;

use xim_parser::{
    Attribute, AttributeName, Extension, ForwardEventFlag, RawXEvent, Writer, XimWrite,
};

pub trait Client {
    type Error: std::error::Error;
    type XEvent;

    fn build_ic_attributes(&self) -> AttributeBuilder;
    fn build_im_attributes(&self) -> AttributeBuilder;

    fn disconnect(&mut self) -> Result<(), Self::Error>;
    fn open(&mut self, locale: &[u8]) -> Result<(), Self::Error>;
    fn close(&mut self, input_method_id: u16) -> Result<(), Self::Error>;
    fn quert_extension(
        &mut self,
        input_method_id: u16,
        extensions: &[&str],
    ) -> Result<(), Self::Error>;
    fn create_ic(
        &mut self,
        input_method_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), Self::Error>;
    fn destory_ic(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), Self::Error>;
    fn forward_event(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        flag: ForwardEventFlag,
        sequence: u16,
        xev: Self::XEvent,
    ) -> Result<(), Self::Error>;
}

pub trait ClientHandler<C: Client> {
    fn handle_connect(&mut self, client: &mut C) -> Result<(), C::Error>;
    fn handle_disconnect(&mut self);
    fn handle_open(&mut self, client: &mut C, input_method_id: u16) -> Result<(), C::Error>;
    fn handle_close(&mut self, client: &mut C, input_method_id: u16) -> Result<(), C::Error>;
    fn handle_query_extension(
        &mut self,
        client: &mut C,
        extensions: &[Extension],
    ) -> Result<(), C::Error>;
    fn handle_create_ic(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), C::Error>;
    fn handle_destory_ic(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), C::Error>;
    fn handle_commit(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        text: &str,
    ) -> Result<(), C::Error>;
    fn handle_forward_event(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        flag: ForwardEventFlag,
        xev: RawXEvent,
    ) -> Result<(), C::Error>;
}

pub use ctext::{compound_text_to_utf8, utf8_to_compound_text};

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
}
