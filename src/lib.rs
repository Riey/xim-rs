pub mod x11rb;

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum InputStlye {
    Invalid = 0,
    OverTheSpot = 1,
    RootWindow = 2,
    OffTheSpot = 3,
    OnTheSpot = 4,
}

impl InputStlye {
    pub fn to_vec(self) -> Vec<u8> {
        (self as u32).to_ne_bytes().to_vec()
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
