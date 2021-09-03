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
        Self { utf8 }
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

#[derive(Debug, Clone, thiserror::Error)]
pub enum DecodeError {
    #[error("Invalid compound text")]
    InvalidEncoding,
    #[error("This encoding is not supported yet")]
    UnsupportedEncoding,
    #[error("Not a valid utf8 {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),
}

macro_rules! decode {
    ($decoder:expr, $out:expr, $bytes:expr, $last:expr) => {
        loop {
            let (ret, _, _) = $decoder.decode_to_string($bytes, $out, $last);

            match ret {
                encoding_rs::CoderResult::InputEmpty => break,
                encoding_rs::CoderResult::OutputFull => {
                    $out.reserve(
                        $decoder
                            .max_utf8_buffer_length($bytes.len())
                            .unwrap_or_default(),
                    );
                }
            }
        }
    };
}

pub fn compound_text_to_utf8(bytes: &[u8]) -> Result<String, DecodeError> {
    let mut iter = bytes.iter();

    match iter.next() {
        None => Ok(String::new()),
        Some(0x1B) => {
            match (iter.next(), iter.next()) {
                // UTF-8
                (Some(0x25), Some(0x47)) => {
                    let left = iter.as_slice();
                    Ok(String::from_utf8(left.split_at(left.len() - 3).0.to_vec())?)
                }
                // 94N
                (Some(0x24), Some(0x28)) => match iter.next() {
                    // JP
                    Some(0x42) => {
                        let left = iter.as_slice();
                        let mut decoder =
                            encoding_rs::ISO_2022_JP.new_decoder_without_bom_handling();
                        let mut out = String::new();

                        decode!(decoder, &mut out, &[0x1B, 0x24, 0x42], false);
                        decode!(decoder, &mut out, left, true);

                        Ok(out)
                    }

                    // CN
                    Some(0x41) => Err(DecodeError::UnsupportedEncoding),

                    // KR
                    Some(0x43) => Err(DecodeError::UnsupportedEncoding),

                    _ => Err(DecodeError::InvalidEncoding),
                },
                // Invalid encode
                _ => Err(DecodeError::InvalidEncoding),
            }
        }
        // unescaped string
        Some(_) => Ok(String::from_utf8(bytes.to_vec())?),
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

    #[test]
    fn iso_2011_jp() {
        const UTF8: &str = "東京";
        const COMP: &[u8] = &[27, 36, 40, 66, 69, 108, 53, 126];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }
}
