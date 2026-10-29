//! A parser for the compound text encoding used by the X Input Method protocol.
//!
//! This is intended to be used as a building block for higher level libraries. See the [`xim`] crate for an example.
//!
//! [xim]: https://crates.io/crates/xim

#![no_std]
#![allow(clippy::uninlined_format_args)]
#![forbid(unsafe_code, future_incompatible)]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "std")]
use std::io::{self, Write};

const UTF8_START: &[u8] = &[0x1B, 0x25, 0x47];
const UTF8_END: &[u8] = &[0x1B, 0x25, 0x40];

fn decode_jis_roman(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len());
    for byte in bytes {
        output.push(match *byte {
            b'\\' => '¥',
            b'~' => '‾',
            other => char::from(other),
        });
    }
    output
}

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

    pub const fn is_empty(self) -> bool {
        self.utf8.is_empty()
    }

    #[cfg(feature = "std")]
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

#[derive(Debug, Clone)]
pub enum DecodeError {
    InvalidEncoding,
    UnsupportedEncoding,
    Utf8Error(alloc::string::FromUtf8Error),
}

impl From<alloc::string::FromUtf8Error> for DecodeError {
    fn from(err: alloc::string::FromUtf8Error) -> Self {
        DecodeError::Utf8Error(err)
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEncoding => write!(f, "Invalid compound text"),
            Self::UnsupportedEncoding => write!(f, "This encoding is not supported yet"),
            Self::Utf8Error(e) => write!(f, "Not a valid utf8 {}", e),
        }
    }
}

macro_rules! decode {
    ($decoder:expr, $out:expr, $bytes:expr, $last:expr) => {
        let mut _current_bytes: &[u8] = $bytes;
        loop {
            let (ret, nread, _) = $decoder.decode_to_string(_current_bytes, $out, $last);

            match ret {
                encoding_rs::CoderResult::InputEmpty => break,
                encoding_rs::CoderResult::OutputFull => {
                    $out.reserve(
                        $decoder
                            .max_utf8_buffer_length($bytes.len())
                            .unwrap_or_default(),
                    );
                    _current_bytes = &_current_bytes[nread..];
                }
            }
        }
    };
}

#[derive(Clone, Copy)]
enum GlDesignation {
    Ascii,
    Roman,
    JisX0208,
    Gb2312,
    KsC5601,
}

#[derive(Clone, Copy)]
enum GrDesignation {
    Latin1,
    Encoded(&'static encoding_rs::Encoding),
}

#[derive(Clone, Copy)]
struct DesignationPair {
    gl: GlDesignation,
    gr: GrDesignation,
}

impl DesignationPair {
    const fn initial() -> Self {
        Self {
            gl: GlDesignation::Ascii,
            gr: GrDesignation::Latin1,
        }
    }
}

fn decode_gl_run(bytes: &[u8], designation: GlDesignation, output: &mut String) {
    match designation {
        GlDesignation::Ascii => {
            for &byte in bytes {
                output.push(char::from(byte));
            }
        }
        GlDesignation::Roman => output.push_str(&decode_jis_roman(bytes)),
        GlDesignation::JisX0208 => {
            let mut decoder = encoding_rs::ISO_2022_JP.new_decoder_without_bom_handling();
            decode!(decoder, output, &[0x1B, 0x24, 0x42], false);
            decode!(decoder, output, bytes, true);
        }
        GlDesignation::Gb2312 => {
            let high_bytes: Vec<u8> = bytes.iter().map(|&byte| byte + 0x80).collect();
            let (decoded, _) = encoding_rs::GBK.decode_without_bom_handling(&high_bytes);
            output.push_str(&decoded);
        }
        GlDesignation::KsC5601 => {
            let high_bytes: Vec<u8> = bytes.iter().map(|&byte| byte + 0x80).collect();
            let (decoded, _) = encoding_rs::EUC_KR.decode_with_bom_removal(&high_bytes);
            output.push_str(&decoded);
        }
    }
}

fn decode_gr_run(bytes: &[u8], designation: GrDesignation, output: &mut String) {
    match designation {
        GrDesignation::Latin1 => output.push_str(&encoding_rs::mem::decode_latin1(bytes)),
        GrDesignation::Encoded(encoding) => {
            let decoded = encoding.decode_without_bom_handling(bytes).0;
            output.push_str(&decoded);
        }
    }
}

fn decode_designated_tail(bytes: &[u8], designations: DesignationPair, output: &mut String) {
    let mut start = 0;
    while start < bytes.len() {
        let byte = bytes[start];
        if byte <= 0x20 || byte == 0x7f {
            output.push(char::from(byte));
            start += 1;
            continue;
        }

        let is_gl = byte <= 0x7e;
        let mut end = start + 1;
        while end < bytes.len() {
            let next = bytes[end];
            if next <= 0x20 || next == 0x7f || (next <= 0x7e) != is_gl {
                break;
            }
            end += 1;
        }

        if is_gl {
            decode_gl_run(&bytes[start..end], designations.gl, output);
        } else {
            decode_gr_run(&bytes[start..end], designations.gr, output);
        }
        start = end;
    }
}

pub fn compound_text_to_utf8(bytes: &[u8]) -> Result<String, DecodeError> {
    let split = bytes.split(|&b| b == 0x1b);

    let mut result = String::new();
    let mut designations = DesignationPair::initial();
    let mut before_utf8 = None;

    for (chunk_index, chunk) in split.enumerate() {
        if chunk_index == 0 {
            result.push_str(&encoding_rs::mem::decode_latin1(chunk));
            continue;
        }

        let mut iter = chunk.iter();
        match (iter.next(), iter.next()) {
            // UTF-8
            (Some(0x25), Some(0x47)) => {
                before_utf8 = Some(designations);
                let left = iter.as_slice().to_vec();
                match String::from_utf8(left) {
                    Ok(out) => result.push_str(&out),
                    Err(e) => return Err(DecodeError::from(e)),
                };
            }
            // UTF-8 End
            (Some(0x25), Some(0x40)) => {
                if let Some(saved) = before_utf8.take() {
                    designations = saved;
                    decode_designated_tail(iter.as_slice(), saved, &mut result);
                }
            }
            // ISO-2022 return to ASCII (ESC ( B) or JIS X 0201 Roman (ESC ( J).
            // Mixed CJK+ASCII commits (e.g. Sogou date phrases) insert these between
            // charset segments; without handling them, "(B" / "(J" leaks into UTF-8.
            (Some(0x28), Some(0x42)) if chunk_index > 0 => {
                designations.gl = GlDesignation::Ascii;
                let left = iter.as_slice();
                let out = encoding_rs::mem::decode_latin1(left);
                result.push_str(&out);
            }
            (Some(0x28), Some(0x4a)) if chunk_index > 0 => {
                designations.gl = GlDesignation::Roman;
                result.push_str(&decode_jis_roman(iter.as_slice()));
            }
            // 94N
            (Some(0x24), Some(0x28)) => match iter.next() {
                // JP
                Some(0x42) => {
                    designations.gl = GlDesignation::JisX0208;
                    let left = iter.as_slice();
                    let mut decoder = encoding_rs::ISO_2022_JP.new_decoder_without_bom_handling();
                    let mut out = String::new();
                    decode!(decoder, &mut out, &[0x1B, 0x24, 0x42], false);
                    decode!(decoder, &mut out, &left, true);

                    result.push_str(&out);
                }

                // CN (GB2312)
                Some(0x41) => {
                    designations.gl = GlDesignation::Gb2312;
                    let left: Vec<u8> = iter.map(|&b| b + 0x80).collect();
                    let (out, _) = encoding_rs::GBK.decode_without_bom_handling(&left);
                    result.push_str(&out);
                }

                // KR (KS C 5601)
                Some(0x43) => {
                    designations.gl = GlDesignation::KsC5601;
                    let left: Vec<u8> = iter.map(|&b| b + 0x80).collect();
                    let (out, _) = encoding_rs::EUC_KR.decode_with_bom_removal(&left);
                    result.push_str(&out);
                }
                // Invalid encode
                _ => return Err(DecodeError::InvalidEncoding),
            },
            // ISO-8859-1
            (Some(0x2d), Some(0x41)) => {
                designations.gr = GrDesignation::Latin1;
                let left = iter.as_slice();
                let out = encoding_rs::mem::decode_latin1(left);
                result.push_str(&out);
            }
            // ISO-8859-2
            (Some(0x2d), Some(0x42)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_2);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_2.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-3
            (Some(0x2d), Some(0x43)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_3);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_3.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-4
            (Some(0x2d), Some(0x44)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_4);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_4.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-7
            (Some(0x2d), Some(0x46)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_7);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_7.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-6
            (Some(0x2d), Some(0x47)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_6);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_6.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-8
            (Some(0x2d), Some(0x48)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_8);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_8.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-5
            (Some(0x2d), Some(0x4c)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_5);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_5.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-9
            (Some(0x2d), Some(0x4d)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::WINDOWS_1254);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::WINDOWS_1254.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-10
            (Some(0x2d), Some(0x56)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_10);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_10.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-13
            (Some(0x2d), Some(0x59)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_13);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_13.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-14
            (Some(0x2d), Some(0x5f)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_14);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_14.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-15
            (Some(0x2d), Some(0x62)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_15);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_15.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // ISO-8859-16
            (Some(0x2d), Some(0x66)) => {
                designations.gr = GrDesignation::Encoded(encoding_rs::ISO_8859_16);
                let left = iter.as_slice();
                let (out, _) = encoding_rs::ISO_8859_16.decode_without_bom_handling(left);
                result.push_str(&out);
            }
            // defaults to ISO-8859-1
            _ => {
                let out = encoding_rs::mem::decode_latin1(chunk);
                result.push_str(&out);
            }
        };
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn preserves_initial_latin1_and_only_decodes_escaped_utf8() -> Result<(), crate::DecodeError> {
        for (input, expected) in [
            (&[0xe9][..], "é"),
            (&[0xc3, 0xa9][..], "Ã©"),
            (b"prefix\xe9".as_slice(), "prefixé"),
            (b"prefix\xc3\xa9".as_slice(), "prefixÃ©"),
            (b"%Gtext".as_slice(), "%Gtext"),
            (b"%@tail".as_slice(), "%@tail"),
            (b"$(B literal".as_slice(), "$(B literal"),
            (b"-Atext".as_slice(), "-Atext"),
        ] {
            assert_eq!(crate::compound_text_to_utf8(input)?, expected);
        }
        assert_eq!(
            crate::compound_text_to_utf8(b"\x1b%G\xe6\x97\xa5\xe6\x9c\xac\x1b%@")?,
            "日本"
        );
        Ok(())
    }

    #[test]
    fn jis_roman_maps_yen_and_overline() -> Result<(), crate::DecodeError> {
        assert_eq!(
            crate::compound_text_to_utf8(b"\x1b$(B\x46\x7c\x1b(J\\~")?,
            "日¥‾"
        );
        Ok(())
    }

    #[test]
    fn preserves_unescaped_ascii_charset_prefixes() -> Result<(), crate::DecodeError> {
        for input in ["(B)", "(J)", "(B", "(J"] {
            assert_eq!(crate::compound_text_to_utf8(input.as_bytes())?, input);
        }
        Ok(())
    }

    #[test]
    fn decodes_escaped_ascii_charset_designators() -> Result<(), crate::DecodeError> {
        assert_eq!(crate::compound_text_to_utf8(b"\x1b(B07")?, "07");
        assert_eq!(crate::compound_text_to_utf8(b"\x1b(JABC")?, "ABC");
        Ok(())
    }

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
    fn iso_2022_jp() {
        const UTF8: &str = "東京";
        const COMP: &[u8] = &[27, 36, 40, 66, 69, 108, 53, 126];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn iso_2022_jp_long() {
        const UTF8: &str = "知ってるつもり";
        const COMP: &[u8] = &[
            27, 36, 40, 66, 67, 78, 36, 67, 36, 70, 36, 107, 36, 68, 36, 98, 36, 106, 27, 40, 66,
        ];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn gb2312_cn_mixed_ascii_digits() {
        // "2026年07月16日" as COMPOUND_TEXT with ESC ( B between GB2312 and ASCII.
        const UTF8: &str = "2026年07月16日";
        #[rustfmt::skip]
        const COMP: &[u8] = &[
            b'2', b'0', b'2', b'6',
            0x1b, 0x24, 0x28, 0x41, 0x44, 0x6a, // 年
            0x1b, 0x28, 0x42, b'0', b'7',
            0x1b, 0x24, 0x28, 0x41, 0x54, 0x42, // 月
            0x1b, 0x28, 0x42, b'1', b'6',
            0x1b, 0x24, 0x28, 0x41, 0x48, 0x55, // 日
        ];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn gb2312_cn() {
        const UTF8: &str = "很高兴认识你";
        const COMP: &[u8] = &[
            0x1b, 0x24, 0x28, 0x41, 0x3a, 0x5c, 0x38, 0x5f, 0x50, 0x4b, 0x48, 0x4f, 0x4a, 0x36,
            0x44, 0x63,
        ];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn gb2312_cn_mixed() {
        const UTF8: &str = "炸哦你";
        const COMP: &[u8] = &[
            0x1b, 0x24, 0x28, 0x42, 0x5f, 0x5a, 0x53, 0x28, 0x1b, 0x24, 0x28, 0x41, 0x44, 0x63,
        ];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn ks_c_5601() {
        const UTF8: &str = "넌최고야";
        const COMP: &[u8] = &[
            0x1b, 0x24, 0x28, 0x43, 0x33, 0x4d, 0x43, 0x56, 0x30, 0x6d, 0x3e, 0x5f,
        ];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn iso_8859_1() {
        const UTF8: &str = "¡¸ÀÑâó";
        const COMP: &[u8] = &[0x1b, 0x2d, 0x41, 0xa1, 0xb8, 0xc0, 0xd1, 0xe2, 0xf3];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn iso_8859_2() {
        const UTF8: &str = "ĄŁĽŚŠŤ";
        const COMP: &[u8] = &[0x1b, 0x2d, 0x42, 0xa1, 0xa3, 0xa5, 0xa6, 0xa9, 0xab];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn utf8_return_restores_existing_gl_designations() -> Result<(), crate::DecodeError> {
        let cases: &[(&str, &[u8], &str)] = &[
            (
                "JIS X 0208",
                &b"\x1b$(B\x46\x7c\x1b%G\xf0\x9f\x99\x82\x1b%@\x4b\x5c"[..],
                "日🙂本",
            ),
            (
                "GB2312",
                &b"\x1b$(A\x56\x50\x1b%G\xf0\x9f\x99\x82\x1b%@\x4e\x44"[..],
                "中🙂文",
            ),
            (
                "KS C 5601",
                &b"\x1b$(C\x30\x21\x1b%G\xf0\x9f\x99\x82\x1b%@\x33\x2a"[..],
                "가🙂나",
            ),
            (
                "ASCII",
                &b"\x1b(BA\x1b%G\xf0\x9f\x99\x82\x1b%@Z"[..],
                "A🙂Z",
            ),
            (
                "JIS X 0201 Roman",
                &b"\x1b(JA\x1b%G\xf0\x9f\x99\x82\x1b%@\x5c\x7e"[..],
                "A🙂¥‾",
            ),
        ];

        for (name, input, expected) in cases {
            let actual = crate::compound_text_to_utf8(input)?;
            assert_eq!(actual.as_str(), *expected, "designation: {name}");
        }
        Ok(())
    }

    #[test]
    fn utf8_return_preserves_default_latin1_and_controls() -> Result<(), crate::DecodeError> {
        assert_eq!(
            crate::compound_text_to_utf8(b"\x1b%G\xf0\x9f\x99\x82\x1b%@\xc3\xa9")?,
            "🙂Ã©"
        );
        assert_eq!(
            crate::compound_text_to_utf8(
                b"\x1b$(B\x46\x7c\x1b%G\xf0\x9f\x99\x82\x1b%@ \t\x4b\x5c\n"
            )?,
            "日🙂 \t本\n"
        );
        Ok(())
    }

    #[test]
    fn utf8_return_restores_every_existing_iso8859_gr_mapping() -> Result<(), crate::DecodeError> {
        const CASES: &[(u8, u8, &str)] = &[
            (0x41, 0xa1, "¡"),
            (0x42, 0xa1, "Ą"),
            (0x43, 0xa1, "Ħ"),
            (0x44, 0xa1, "Ą"),
            (0x46, 0xa1, "‘"),
            (0x47, 0xc7, "ا"),
            (0x48, 0xe0, "א"),
            (0x4c, 0xa1, "Ё"),
            (0x4d, 0xd0, "Ğ"),
            (0x56, 0xa1, "Ą"),
            (0x59, 0xa1, "”"),
            (0x5f, 0xa1, "Ḃ"),
            (0x62, 0xa4, "€"),
            (0x66, 0xa1, "Ą"),
        ];

        for &(final_byte, gr_byte, expected_gr) in CASES {
            let mut input = alloc::vec::Vec::new();
            input.extend_from_slice(b"\x1b(BA\x1b-");
            input.push(final_byte);
            input.extend_from_slice(b"\x1b%G\xf0\x9f\x99\x82\x1b%@A");
            input.push(gr_byte);

            let mut expected = alloc::string::String::from("A🙂A");
            expected.push_str(expected_gr);
            let actual = crate::compound_text_to_utf8(&input)?;
            assert_eq!(actual, expected, "ISO-8859 final: {}", final_byte as char);
        }
        Ok(())
    }

    #[test]
    fn utf8_return_restores_changed_gr_and_independent_gl_gr_pair() -> Result<(), crate::DecodeError>
    {
        assert_eq!(
            crate::compound_text_to_utf8(b"\x1b-B\xa1\x1b-A\x1b%G\xf0\x9f\x99\x82\x1b%@\xa1")?,
            "Ą🙂¡"
        );
        assert_eq!(
            crate::compound_text_to_utf8(b"\x1b$(B\x1b-B\x1b%G\xf0\x9f\x99\x82\x1b%@\x46\x7c\xa1")?,
            "🙂日Ą"
        );
        Ok(())
    }

    #[test]
    fn utf8_return_handles_multiple_segments_empty_tail_and_orphan_end()
    -> Result<(), crate::DecodeError> {
        assert_eq!(
            crate::compound_text_to_utf8(
                b"\x1b$(B\x46\x7c\x1b%G\xf0\x9f\x99\x82\x1b%@\x4b\x5c\x1b%G\xf0\x9f\x99\x82\x1b%@\x46\x7c"
            )?,
            "日🙂本🙂日"
        );
        assert_eq!(
            crate::compound_text_to_utf8(b"\x1b$(B\x46\x7c\x1b%G\xf0\x9f\x99\x82\x1b%@")?,
            "日🙂"
        );
        assert_eq!(crate::compound_text_to_utf8(b"\x1b%@tail")?, "");
        assert_eq!(crate::compound_text_to_utf8(b"\x1b%Xtail")?, "%Xtail");
        assert!(matches!(
            crate::compound_text_to_utf8(b"\x1b%G\xff"),
            Err(crate::DecodeError::Utf8Error(_))
        ));
        Ok(())
    }
}
