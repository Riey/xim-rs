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
