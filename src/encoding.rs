#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    Utf8,
    CompoundText,
}

impl Default for Encoding {
    fn default() -> Self {
        Encoding::Utf8
    }
}

impl Encoding {
    pub const ALL_ENCODINGS: [Encoding; 2] = [Encoding::Utf8, Encoding::CompoundText];

    pub fn read(self, bytes: Vec<u8>) -> Result<String, ()> {
        match self {
            Encoding::CompoundText => ctext::compound_text_to_utf8(&bytes).map(ToString::to_string),
            Encoding::Utf8 => String::from_utf8(bytes).map_err(|_| ()),
        }
    }

    pub fn write(self, text: String) -> Vec<u8> {
        match self {
            Encoding::CompoundText => ctext::utf8_to_compound_text(&text),
            Encoding::Utf8 => text.into_bytes(),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Encoding::CompoundText => "COMPOUND_TEXT",
            Encoding::Utf8 => "UTF-8",
        }
    }
}
