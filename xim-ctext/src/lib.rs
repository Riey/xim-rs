//! Currently only support utf8 mode

use std::fmt;
use std::io::{self, Write};

const UTF8_START: &[u8] = &[0x1B, 0x25, 0x47];
const UTF8_END: &[u8] = &[0x1B, 0x25, 0x40];

/// Wrapper for reduce allocation
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct CText<'s> {
    utf8: &'s str,
}

impl<'s> fmt::Debug for CText<'s> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.utf8)
    }
}

impl<'s> fmt::Display for CText<'s> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.utf8)
    }
}

impl<'s> CText<'s> {
    pub const fn new(utf8: &'s str) -> Self {
        Self {
            utf8
        }
    }

    pub const fn len(self) -> usize {
        self.utf8.len() + UTF8_START.len() + UTF8_END.len()
    }

    pub fn write(self, mut out: impl Write) -> io::Result<usize> {
        let mut writed = 0;
        writed += out.write(UTF8_START)?;
        writed += out.write(self.utf8.as_bytes())?;
        writed += out.write(UTF8_END)?;
        Ok(writed)
    }
}

/// Encoding utf8 to COMPOUND_TEXT with utf8 escape
pub fn utf8_to_compound_text(text: &str) -> Vec<u8> {
    let mut ret = Vec::with_capacity(text.len() + 6);
    ret.extend_from_slice(UTF8_START);
    ret.extend_from_slice(text.as_bytes());
    ret.extend_from_slice(UTF8_END);
    ret
}

/// Decoding COMPOUND_TEXT to utf8 only works with utf8 escaped text
pub fn compound_text_to_utf8(bytes: &[u8]) -> Result<&str, ()> {
    if bytes.starts_with(UTF8_START) && bytes.ends_with(UTF8_END) {
        std::str::from_utf8(&bytes[3..bytes.len() - 3]).map_err(|_| ())
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn korean() {
        const UTF8: &str = "가나다";
        const COMP: &[u8] = &[
            27, 37, 71, 234, 176, 128, 235, 130, 152, 235, 139, 164, 27, 37, 64,
        ];
        assert_eq!(crate::utf8_to_compound_text(UTF8), COMP);
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }
}
