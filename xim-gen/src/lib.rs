use crate::format_type::Field;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

mod format_type;

#[derive(Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, Eq, PartialEq))]
struct EnumFormat {
    repr: String,
    #[serde(default)]
    bitflag: bool,
    variants: BTreeMap<String, usize>,
}

impl EnumFormat {
    pub fn write(&self, name: &str, out: &mut impl Write) -> io::Result<()> {
        // reorder variants for variant value
        let mut variants = self.variants.iter().collect::<Vec<_>>();
        variants.sort_unstable_by(|l, r| l.1.cmp(&r.1));

        if self.bitflag {
            writeln!(out, "bitflags::bitflags! {{")?;

            writeln!(out, "pub struct {}: {} {{", name, self.repr)?;
            for (name, variant) in variants.iter() {
                writeln!(out, "const {} = {};", name.to_ascii_uppercase(), variant)?;
            }
            writeln!(out, "}}")?;

            writeln!(out, "}}")?;
        } else {
            writeln!(out, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]")?;
            writeln!(out, "#[repr({})]", self.repr)?;
            writeln!(out, "pub enum {} {{", name)?;

            for (name, variant) in variants.iter() {
                writeln!(out, "{} = {},", name, variant)?;
            }
            writeln!(out, "}}")?;
        }

        writeln!(out, "impl XimRead for {} {{", name)?;

        writeln!(
            out,
            "fn read(reader: &mut Reader) -> Result<Self, ReadError> {{ let repr = {}::read(reader)?;", self.repr)?;

        if self.bitflag {
            writeln!(
                out,
                "Self::from_bits(repr).ok_or(reader.invalid_data(\"{}\", repr))",
                name
            )?;
        } else {
            writeln!(out, "match repr {{")?;
            for (name, variants) in variants.iter() {
                writeln!(out, "{v} => Ok(Self::{n}),", v = variants, n = name)?;
            }

            writeln!(
                out,
                "_ => Err(reader.invalid_data(\"{n}\", repr)),",
                n = name
            )?;

            writeln!(out, "}}")?;
        }

        writeln!(out, "}}")?;

        // impl XimRead
        writeln!(out, "}}")?;

        writeln!(out, "impl XimWrite for {} {{", name)?;

        writeln!(out, "fn write(&self, writer: &mut Writer) {{")?;

        if self.bitflag {
            writeln!(out, "self.bits().write(writer);")?;
        } else {
            writeln!(out, "(*self as {}).write(writer);", self.repr)?;
        }

        writeln!(out, "}}")?;

        writeln!(
            out,
            "fn size(&self) -> usize {{ std::mem::size_of::<{}>() }}",
            self.repr
        )?;

        // impl XimWrite
        writeln!(out, "}}")?;

        Ok(())
    }
}

#[derive(Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, Eq, PartialEq))]
struct RequestFormat {
    major_opcode: u8,
    minor_opcode: Option<u8>,
    body: Vec<Field>,
}

#[derive(Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, Eq, PartialEq))]
#[serde(transparent)]
struct StructFormat {
    body: Vec<Field>,
}

impl StructFormat {
    pub fn write(&self, name: &str, out: &mut impl Write) -> io::Result<()> {
        writeln!(out, "#[derive(Clone, Debug, Eq, PartialEq)]")?;
        write!(out, "pub struct {}", name)?;
        writeln!(out, "{{")?;

        for field in self.body.iter() {
            writeln!(out, "pub {}: {},", field.name, field.ty)?;
        }

        writeln!(out, "}}")?;

        writeln!(out, "impl XimRead for {} {{", name)?;

        writeln!(
            out,
            "fn read(reader: &mut Reader) -> Result<Self, ReadError> {{"
        )?;

        writeln!(out, "Ok(Self {{")?;
        for field in self.body.iter() {
            write!(out, "{}: ", field.name)?;
            field.ty.read(out)?;
            write!(out, ",")?;
        }
        writeln!(out, "}})")?;

        // fn read
        writeln!(out, "}}")?;
        // impl XimRead
        writeln!(out, "}}")?;

        writeln!(out, "impl XimWrite for {} {{", name)?;
        writeln!(out, "fn write(&self, writer: &mut Writer) {{")?;
        for field in self.body.iter() {
            field.ty.write(&format!("self.{}", field.name), out)?;
        }
        // fn write
        writeln!(out, "}}")?;

        writeln!(out, "fn size(&self) -> usize {{")?;
        writeln!(out, "let mut content_size = 0;")?;

        for field in self.body.iter() {
            write!(out, "content_size += ")?;
            field.ty.size(&format!("self.{}", field.name), out)?;
            writeln!(out, ";")?;
        }

        writeln!(out, "content_size")?;

        // fn size
        writeln!(out, "}}")?;

        // end impl
        writeln!(out, "}}")?;

        Ok(())
    }
}

#[derive(Deserialize)]
#[cfg_attr(debug_assertions, derive(Debug, Eq, PartialEq))]
struct XimFormat {
    #[serde(rename = "Enums")]
    enums: BTreeMap<String, EnumFormat>,
    #[serde(rename = "AttributeNames")]
    attribute_names: BTreeMap<String, String>,
    #[serde(rename = "Structs")]
    structs: BTreeMap<String, StructFormat>,
    #[serde(rename = "Requests")]
    requests: BTreeMap<String, RequestFormat>,
}

impl XimFormat {
    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        for (name, em) in self.enums.iter() {
            em.write(name, out)?;
        }

        for (name, st) in self.structs.iter() {
            st.write(name, out)?;
        }

        writeln!(
            out,
            "#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]"
        )?;
        writeln!(out, "pub enum AttributeName {{")?;
        for (key, _value) in self.attribute_names.iter() {
            writeln!(out, "{},", key)?;
        }
        writeln!(out, "}}")?;

        writeln!(out, "impl AttributeName {{")?;
        writeln!(out, "pub fn name(self) -> &'static str {{")?;
        writeln!(out, "match self {{")?;
        for (key, value) in self.attribute_names.iter() {
            writeln!(out, "Self::{} => \"{}\",", key, value)?;
        }
        // match
        writeln!(out, "}}")?;
        // fn name
        writeln!(out, "}}")?;
        // impl AttributeName
        writeln!(out, "}}")?;

        writeln!(out, "impl XimRead for AttributeName {{")?;
        writeln!(
            out,
            "fn read(reader: &mut Reader) -> Result<Self, ReadError> {{"
        )?;
        writeln!(
            out,
            "let len = u16::read(reader)?; match reader.consume(len as usize)? {{"
        )?;
        for (key, value) in self.attribute_names.iter() {
            writeln!(out, "b\"{}\" => Ok(Self::{}),", value, key)?;
        }
        writeln!(out, "bytes => Err(reader.invalid_data(\"AttributeName\", std::str::from_utf8(bytes).unwrap_or(\"NOT_UTF8\"))),")?;
        // match
        writeln!(out, "}}")?;
        // fn read
        writeln!(out, "}}")?;
        // impl XimRead
        writeln!(out, "}}")?;

        writeln!(out, "impl XimWrite for AttributeName {{")?;

        writeln!(out, "fn write(&self, writer: &mut Writer) {{")?;
        writeln!(out, "let name = self.name(); (name.len() as u16).write(writer); writer.write(name.as_bytes());")?;
        // fn write
        writeln!(out, "}}")?;

        writeln!(out, "fn size(&self) -> usize {{")?;
        writeln!(out, "self.name().len() + 2")?;
        // fn size
        writeln!(out, "}}")?;

        // impl XimWrite
        writeln!(out, "}}")?;

        writeln!(out, "#[derive(Debug, Clone, Eq, PartialEq)]")?;
        writeln!(out, "pub enum Request {{")?;

        for (name, req) in self.requests.iter() {
            writeln!(out, "{} {{", name)?;
            for field in req.body.iter() {
                writeln!(out, "{}: {},", field.name, field.ty)?;
            }
            writeln!(out, "}},")?;
        }

        writeln!(out, "}}")?;

        writeln!(out, "impl Request {{")?;
        writeln!(out, "pub fn name(&self) -> &'static str {{")?;
        writeln!(out, "match self {{")?;
        for (name, _req) in self.requests.iter() {
            writeln!(out, "Request::{} {{ .. }} => \"{}\",", name, name)?;
        }
        // match
        writeln!(out, "}}")?;
        // fn name
        writeln!(out, "}}")?;
        // impl Request
        writeln!(out, "}}")?;

        writeln!(out, "impl XimRead for Request {{")?;

        writeln!(
            out,
            "fn read(reader: &mut Reader) -> Result<Self, ReadError> {{"
        )?;

        writeln!(
            out,
            "let major_opcode = reader.u8()?; let minor_opcode = reader.u8()?; let _length = reader.u16()?;"
        )?;

        writeln!(out, "match (major_opcode, minor_opcode) {{")?;

        for (name, req) in self.requests.iter() {
            write!(out, "({}, ", req.major_opcode)?;

            if let Some(minor) = req.minor_opcode {
                write!(out, "{}", minor)?;
            } else {
                write!(out, "_")?;
            }

            writeln!(out, ") => Ok(Request::{} {{", name)?;
            for field in req.body.iter() {
                write!(out, "{}: ", field.name)?;
                field.ty.read(out)?;
                write!(out, ",")?;
            }
            writeln!(out, "}}),")?;
        }

        writeln!(out, "_ => Err(reader.invalid_data(\"Opcode\", format!(\"({{}}, {{}})\", major_opcode, minor_opcode))),")?;

        // match
        writeln!(out, "}}")?;

        // fn read
        writeln!(out, "}}")?;

        // impl XimRead
        writeln!(out, "}}")?;

        writeln!(out, "impl XimWrite for Request {{")?;

        writeln!(out, "fn write(&self, writer: &mut Writer) {{")?;

        writeln!(out, "match self {{")?;

        for (name, req) in self.requests.iter() {
            writeln!(out, "Request::{} {{", name)?;
            for field in req.body.iter() {
                write!(out, "{}, ", field.name)?;
            }
            writeln!(out, "}} => {{")?;

            writeln!(out, "{}u8.write(writer);", req.major_opcode)?;
            writeln!(out, "{}u8.write(writer);", req.minor_opcode.unwrap_or(0))?;
            writeln!(out, "(((self.size() - 4) / 4) as u16).write(writer);")?;

            for field in req.body.iter() {
                field.ty.write(&field.name, out)?;
            }

            writeln!(out, "}}")?;
        }

        // match
        writeln!(out, "}}")?;

        // fn write
        writeln!(out, "}}")?;

        writeln!(out, "fn size(&self) -> usize {{")?;
        writeln!(out, "let mut content_size = 0;")?;

        writeln!(out, "match self {{")?;

        for (name, req) in self.requests.iter() {
            writeln!(out, "Request::{} {{", name)?;
            for field in req.body.iter() {
                write!(out, "{}, ", field.name)?;
            }
            writeln!(out, "}} => {{")?;

            for field in req.body.iter() {
                write!(out, "content_size += ")?;
                field.ty.size(&field.name, out)?;
                writeln!(out, ";")?;
            }

            writeln!(out, "}}")?;
        }

        // match
        writeln!(out, "}}")?;
        writeln!(out, "content_size + 4")?;

        // fn size
        writeln!(out, "}}")?;

        // impl XimWrite
        writeln!(out, "}}")?;

        Ok(())
    }
}

pub fn write_format(
    format_str: &str,
    out_path: impl AsRef<Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let format: XimFormat = serde_yaml::from_str(format_str)?;

    let mut file = std::fs::File::create(out_path.as_ref())?;

    file.write_all(include_bytes!("../res/snippet.rs"))?;
    format.write(&mut file)?;
    file.flush()?;

    let rustfmt = std::process::Command::new("rustfmt")
        .arg(std::fs::canonicalize(out_path.as_ref())?)
        .spawn()
        .expect("call rustfmt")
        .wait()
        .unwrap();

    assert!(rustfmt.success());

    Ok(())
}
