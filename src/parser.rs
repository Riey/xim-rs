#![allow(unused)]

use num_traits::{cast, NumCast, Zero};
use std::convert::TryInto;
use std::marker::PhantomData;

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("End of Stream")]
    EndOfStream,
    #[error("Invalid Data {0}: {1}")]
    InvalidData(&'static str, String),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XimVec<T, Length>(pub Vec<T>, PhantomData<Length>);

impl<T, Length> XimVec<T, Length> {
    pub fn new(v: Vec<T>) -> Self {
        Self(v, PhantomData)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Pad4<T>(pub T);

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct XimString<'b, Length>(pub &'b [u8], PhantomData<Length>);

impl<'b, Length> XimString<'b, Length> {
    pub fn new(b: &'b [u8]) -> Self {
        Self(b, PhantomData)
    }
}

pub trait XimFormat<'b>: Sized {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError>;
    fn write(&self, writer: &mut Writer);
    /// byte size of format
    fn size(&self) -> usize;
}

impl<'b, T> XimFormat<'b> for Pad4<T>
where
    T: XimFormat<'b>,
{
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        let inner = T::read(reader)?;
        reader.pad4()?;

        Ok(Self(inner))
    }

    fn write(&self, writer: &mut Writer) {
        self.0.write(writer);
        writer.write_pad4();
    }

    fn size(&self) -> usize {
        let inner_size = self.0.size();
        inner_size + pad4(inner_size)
    }
}

impl<'b, T, Length> XimFormat<'b> for XimVec<T, Length>
where
    Length: XimFormat<'b> + NumCast + Zero,
    T: XimFormat<'b>,
{
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        let len: usize = cast(Length::read(reader)?).unwrap();
        let end = reader.cursor() - len;
        let mut out = Vec::new();

        while reader.cursor() > end {
            out.push(T::read(reader)?);
        }

        Ok(Self::new(out))
    }

    fn write(&self, writer: &mut Writer) {
        let len: Length = cast(self.0.iter().map(XimFormat::size).sum::<usize>()).unwrap();
        len.write(writer);
        for elem in self.0.iter() {
            elem.write(writer);
        }
    }

    fn size(&self) -> usize {
        self.0.iter().map(XimFormat::size).sum::<usize>() + Length::zero().size()
    }
}

impl<'b, Length> XimFormat<'b> for XimString<'b, Length>
where
    Length: XimFormat<'b> + NumCast + Zero,
{
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        let len = cast(Length::read(reader)?).unwrap();
        let bytes = reader.consume(len)?;
        Ok(Self::new(bytes))
    }

    fn write(&self, writer: &mut Writer) {
        let len: Length = cast(self.0.len()).unwrap();
        len.write(writer);
        writer.write(self.0);
    }

    fn size(&self) -> usize {
        self.0.len() + Length::zero().size()
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
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Copy, Debug)]
#[repr(u16)]
pub enum AttrType {
    Window = 5,
    Byte = 1,
    HotkeyTriggers = 15,
    Word = 2,
    ResetState = 19,
    XRectangle = 11,
    XPoint = 12,
    StringConversion = 17,
    XFontSet = 13,
    Char = 4,
    PreeditState = 18,
    Separator = 0,
    Style = 10,
    NestedList = 32767,
    Long = 3,
}
impl<'b> XimFormat<'b> for AttrType {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        let repr = u16::read(reader)?;
        match repr {
            5 => Ok(Self::Window),
            1 => Ok(Self::Byte),
            15 => Ok(Self::HotkeyTriggers),
            2 => Ok(Self::Word),
            19 => Ok(Self::ResetState),
            11 => Ok(Self::XRectangle),
            12 => Ok(Self::XPoint),
            17 => Ok(Self::StringConversion),
            13 => Ok(Self::XFontSet),
            4 => Ok(Self::Char),
            18 => Ok(Self::PreeditState),
            0 => Ok(Self::Separator),
            10 => Ok(Self::Style),
            32767 => Ok(Self::NestedList),
            3 => Ok(Self::Long),
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
    Connect {
        client_auth_protocol_names: XimVec<Pad4<XimString<'b, u16>>, u16>,
        client_major_protocol_version: u16,
        client_minor_protocol_version: u16,
    },
}
impl<'b> XimFormat<'b> for Request<'b> {
    fn read(reader: &mut Reader<'b>) -> Result<Self, ReadError> {
        let major_opcode = reader.u16()?;
        let minor_opcode = reader.u16()?;
        match (major_opcode, minor_opcode) {
            (1, _) => Ok(Request::Connect {
                client_auth_protocol_names: XimFormat::read(reader)?,
                client_major_protocol_version: XimFormat::read(reader)?,
                client_minor_protocol_version: XimFormat::read(reader)?,
            }),
            _ => {
                Err(reader.invalid_data("Opcode", format!("({}, {})", major_opcode, minor_opcode)))
            }
        }
    }
    fn write(&self, writer: &mut Writer) {
        match self {
            Request::Connect {
                client_auth_protocol_names,
                client_major_protocol_version,
                client_minor_protocol_version,
            } => {
                1u8.write(writer);
                0u8.write(writer);
                (((self.size() - 4) / 4) as u16).write(writer);
                client_auth_protocol_names.write(writer);
                client_major_protocol_version.write(writer);
                client_minor_protocol_version.write(writer);
            }
        }
    }
    fn size(&self) -> usize {
        let mut content_size = 0;
        match self {
            Request::Connect {
                client_auth_protocol_names,
                client_major_protocol_version,
                client_minor_protocol_version,
            } => {
                content_size += client_auth_protocol_names.size();
                content_size += client_major_protocol_version.size();
                content_size += client_minor_protocol_version.size();
            }
        }
        content_size + 4
    }
}
