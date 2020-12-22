use crate::AttributeBuilder;
use std::collections::HashMap;
use xim_parser::{Attribute, AttributeName, Extension, ForwardEventFlag, Request};

pub trait ClientCore {
    type Error: std::error::Error;
    type XEvent;

    fn ic_attributes(&self) -> &HashMap<AttributeName, u16>;
    fn im_attributes(&self) -> &HashMap<AttributeName, u16>;
    fn serialize_event(&self, xev: Self::XEvent) -> xim_parser::XEvent;
    fn deserialize_event(&self, xev: xim_parser::XEvent) -> Self::XEvent;
    fn send_req(&mut self, req: Request) -> Result<(), Self::Error>;
}

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
        xev: Self::XEvent,
    ) -> Result<(), Self::Error>;
}

impl<C> Client for C
where
    C: ClientCore,
{
    type Error = C::Error;
    type XEvent = C::XEvent;

    fn build_ic_attributes(&self) -> AttributeBuilder {
        AttributeBuilder::new(self.ic_attributes())
    }

    fn build_im_attributes(&self) -> AttributeBuilder {
        AttributeBuilder::new(self.im_attributes())
    }

    fn open(&mut self, locale: &[u8]) -> Result<(), Self::Error> {
        self.send_req(Request::Open {
            locale: locale.into(),
        })
    }

    fn quert_extension(
        &mut self,
        input_method_id: u16,
        extensions: &[&str],
    ) -> Result<(), Self::Error> {
        self.send_req(Request::QueryExtension {
            input_method_id,
            extensions: extensions.iter().map(|&e| e.into()).collect(),
        })
    }

    fn create_ic(
        &mut self,
        input_method_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), Self::Error> {
        self.send_req(Request::CreateIc {
            input_method_id,
            ic_attributes,
        })
    }

    fn forward_event(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        flag: ForwardEventFlag,
        xev: Self::XEvent,
    ) -> Result<(), Self::Error> {
        let ev = self.serialize_event(xev);
        self.send_req(Request::ForwardEvent {
            input_method_id,
            input_context_id,
            flag,
            serial_number: ev.sequence,
            xev: ev,
        })
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.send_req(Request::Disconnect {})
    }

    fn close(&mut self, input_method_id: u16) -> Result<(), Self::Error> {
        self.send_req(Request::Close { input_method_id })
    }

    fn destory_ic(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), Self::Error> {
        self.send_req(Request::DestoryIc {
            input_method_id,
            input_context_id,
        })
    }
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
        xev: C::XEvent,
    ) -> Result<(), C::Error>;
}
