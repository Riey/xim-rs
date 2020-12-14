#![allow(unused)]

use std::convert::TryInto;
use std::fmt;

pub fn read<'a, T: XimFormat<'a>>(b: &'a [u8]) -> Result<T, ReadError> {
    T::read(&mut Reader::new(b))
}

pub fn write<'a, T: XimFormat<'a>>(data: &T, out: &mut Vec<u8>) {
    data.write(&mut Writer::new(out));
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Endian {
    Big = 0x42,
    Little = 0x6c,
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("End of Stream")]
    EndOfStream,
    #[error("Invalid Data {0}: {1}")]
    InvalidData(&'static str, String),
    #[error("Not a native endian")]
    NotNativeEndian,
}

fn pad4(len: usize) -> usize {
    (4 - (len % 4)) % 4
}

pub struct Reader<'b> {
    bytes: &'b [u8],
    start: usize,
}

impl<'b> Reader<'b> {
    pub fn new(bytes: &'b [u8]) -> Self {
        Self {
            bytes,
            start: bytes.as_ptr() as usize,
        }
    }

    fn ptr_offset(&self) -> usize {
        self.bytes.as_ptr() as usize - self.start
    }

    pub fn cursor(&self) -> usize {
        self.bytes.len()
    }

    pub fn pad4(&mut self) -> Result<(), ReadError> {
        self.consume(pad4(self.ptr_offset()))?;
        Ok(())
    }

    pub fn eos(&self) -> ReadError {
        ReadError::EndOfStream
    }

    pub fn invalid_data(&self, ty: &'static str, item: impl ToString) -> ReadError {
        ReadError::InvalidData(ty, item.to_string())
    }

    pub fn u8(&mut self) -> Result<u8, ReadError> {
        let (b, new) = self.bytes.split_first().ok_or(self.eos())?;
        self.bytes = new;
        Ok(*b)
    }

    pub fn u16(&mut self) -> Result<u16, ReadError> {
        let bytes = self.consume(2)?.try_into().unwrap();
        Ok(u16::from_ne_bytes(bytes))
    }

    pub fn u32(&mut self) -> Result<u32, ReadError> {
        let bytes = self.consume(4)?.try_into().unwrap();
        Ok(u32::from_ne_bytes(bytes))
    }

    pub fn i32(&mut self) -> Result<i32, ReadError> {
        let bytes = self.consume(4)?.try_into().unwrap();
        Ok(i32::from_ne_bytes(bytes))
    }

    pub fn consume(&mut self, len: usize) -> Result<&'b [u8], ReadError> {
        if self.bytes.len() >= len {
            let (out, new) = self.bytes.split_at(len);
            self.bytes = new;
            Ok(out)
        } else {
            Err(self.eos())
        }
    }
}

pub struct Writer<'b> {
    out: &'b mut Vec<u8>,
}

impl<'b> Writer<'b> {
    pub fn new(out: &'b mut Vec<u8>) -> Self {
        Self { out }
    }

    pub fn write_u8(&mut self, b: u8) {
        self.out.push(b);
    }

    pub fn write(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    pub fn write_pad4(&mut self) {
        let pad = pad4(self.out.len());
        self.out.extend(std::iter::repeat(0).take(pad));
    }
}

pub trait XimFormat<'b>: Sized {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError>;
    fn write(&self, writer: &mut Writer);
    /// byte size of format
    fn size(&self) -> usize;
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct XimString<'b>(pub &'b [u8]);

impl<'a> fmt::Display for XimString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(std::str::from_utf8(self.0).unwrap_or("NOT_UTF8"))
    }
}

impl<'a> fmt::Debug for XimString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(std::str::from_utf8(self.0).unwrap_or("NOT_UTF8"))
    }
}

impl<'b> XimFormat<'b> for Endian {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        let n = u8::read(reader)?;

        if n == Endian::Little as u8 && cfg!(target_endian = "little") {
            Ok(Self::Little)
        } else if n == Endian::Big as u8 && cfg!(target_endian = "big") {
            Ok(Self::Big)
        } else {
            Err(ReadError::NotNativeEndian)
        }
    }

    fn write(&self, writer: &mut Writer) {
        (*self as u8).write(writer);
    }

    fn size(&self) -> usize {
        1
    }
}

impl<'b> XimFormat<'b> for u8 {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        reader.u8()
    }

    fn write(&self, writer: &mut Writer) {
        writer.write_u8(*self)
    }

    fn size(&self) -> usize {
        1
    }
}

impl<'b> XimFormat<'b> for u16 {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        reader.u16()
    }

    fn write(&self, writer: &mut Writer) {
        writer.write(&self.to_ne_bytes())
    }

    fn size(&self) -> usize {
        2
    }
}

impl<'b> XimFormat<'b> for u32 {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        reader.u32()
    }

    fn write(&self, writer: &mut Writer) {
        writer.write(&self.to_ne_bytes())
    }

    fn size(&self) -> usize {
        4
    }
}
impl<'b> XimFormat<'b> for i32 {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        reader.i32()
    }

    fn write(&self, writer: &mut Writer) {
        writer.write(&self.to_ne_bytes())
    }

    fn size(&self) -> usize {
        4
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CaretStyle {
    Invisible = 0,
    Primary = 1,
    Secondary = 2,
}
impl<'b> XimFormat<'b> for CaretStyle {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        let repr = u32::read(reader)?;
        match repr {
            0 => Ok(Self::Invisible),
            1 => Ok(Self::Primary),
            2 => Ok(Self::Secondary),
            _ => Err(reader.invalid_data("CaretStyle", repr)),
        }
    }
    fn write(&self, writer: &mut Writer) {
        (*self as u32).write(writer);
    }
    fn size(&self) -> usize {
        std::mem::size_of::<u32>()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum AttrType {
    Byte = 1,
    Long = 3,
    Style = 10,
    Char = 4,
    ResetState = 19,
    StringConversion = 17,
    XFontSet = 13,
    Separator = 0,
    NestedList = 32767,
    XRectangle = 11,
    Word = 2,
    Window = 5,
    XPoint = 12,
    HotkeyTriggers = 15,
    PreeditState = 18,
}
impl<'b> XimFormat<'b> for AttrType {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        let repr = u16::read(reader)?;
        match repr {
            1 => Ok(Self::Byte),
            3 => Ok(Self::Long),
            10 => Ok(Self::Style),
            4 => Ok(Self::Char),
            19 => Ok(Self::ResetState),
            17 => Ok(Self::StringConversion),
            13 => Ok(Self::XFontSet),
            0 => Ok(Self::Separator),
            32767 => Ok(Self::NestedList),
            11 => Ok(Self::XRectangle),
            2 => Ok(Self::Word),
            5 => Ok(Self::Window),
            12 => Ok(Self::XPoint),
            15 => Ok(Self::HotkeyTriggers),
            18 => Ok(Self::PreeditState),
            _ => Err(reader.invalid_data("AttrType", repr)),
        }
    }
    fn write(&self, writer: &mut Writer) {
        (*self as u16).write(writer);
    }
    fn size(&self) -> usize {
        std::mem::size_of::<u16>()
    }
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Request<'b> {
    Open {
        name: XimString<'b>,
    },
    Connect {
        endian: Endian,
        client_major_protocol_version: u16,
        client_minor_protocol_version: u16,
        client_auth_protocol_names: Vec<XimString<'b>>,
    },
}
impl<'b> XimFormat<'b> for Request<'b> {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        let major_opcode = reader.u8()?;
        let minor_opcode = reader.u8()?;
        let _length = reader.u16()?;
        match (major_opcode, minor_opcode) {
            (30, _) => Ok(Request::Open {
                name: {
                    let inner = {
                        let len = u8::read(reader)?;
                        let bytes = reader.consume(len as usize)?;
                        XimString(bytes)
                    };
                    reader.pad4()?;
                    inner
                },
            }),
            (1, _) => Ok(Request::Connect {
                endian: {
                    let inner = Endian::read(reader)?;
                    u8::read(reader)?;
                    inner
                },
                client_major_protocol_version: u16::read(reader)?,
                client_minor_protocol_version: u16::read(reader)?,
                client_auth_protocol_names: {
                    let mut out = Vec::new();
                    let len = u16::read(reader)? as usize;
                    let end = reader.cursor() - len;
                    while reader.cursor() > end {
                        out.push({
                            let inner = {
                                let len = u16::read(reader)?;
                                let bytes = reader.consume(len as usize)?;
                                XimString(bytes)
                            };
                            reader.pad4()?;
                            inner
                        });
                    }
                    out
                },
            }),
            _ => {
                Err(reader.invalid_data("Opcode", format!("({}, {})", major_opcode, minor_opcode)))
            }
        }
    }
    fn write(&self, writer: &mut Writer) {
        match self {
            Request::Open { name } => {
                30u8.write(writer);
                0u8.write(writer);
                (((self.size() - 4) / 4) as u16).write(writer);
                (name.0.len() as u8).write(writer);
                writer.write(name.0);
                writer.write_pad4();
            }
            Request::Connect {
                endian,
                client_major_protocol_version,
                client_minor_protocol_version,
                client_auth_protocol_names,
            } => {
                1u8.write(writer);
                0u8.write(writer);
                (((self.size() - 4) / 4) as u16).write(writer);
                endian.write(writer);
                0u8.write(writer);
                client_major_protocol_version.write(writer);
                client_minor_protocol_version.write(writer);
                ((client_auth_protocol_names
                    .iter()
                    .map(|e| pad4(e.0.len() + 2))
                    .sum::<usize>()
                    + 2
                    - 2) as u16)
                    .write(writer);
                for elem in client_auth_protocol_names.iter() {
                    (elem.0.len() as u16).write(writer);
                    writer.write(elem.0);
                    writer.write_pad4();
                }
            }
        }
    }
    fn size(&self) -> usize {
        let mut content_size = 0;
        match self {
            Request::Open { name } => {
                content_size += pad4(name.0.len() + 1);
            }
            Request::Connect {
                endian,
                client_major_protocol_version,
                client_minor_protocol_version,
                client_auth_protocol_names,
            } => {
                content_size += endian.size() + 1;
                content_size += client_major_protocol_version.size();
                content_size += client_minor_protocol_version.size();
                content_size += client_auth_protocol_names
                    .iter()
                    .map(|e| pad4(e.0.len() + 2))
                    .sum::<usize>()
                    + 2;
            }
        }
        content_size + 4
    }
}
