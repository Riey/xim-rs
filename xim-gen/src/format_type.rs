use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::io::{self, Write};

#[derive(Debug, Eq, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: FormatType,
}

#[derive(Debug, Eq, PartialEq)]
pub enum FormatType {
    Append(Box<Self>, usize),
    Pad(Box<Self>),
    List(Box<Self>, usize),
    String(usize),
    Normal(String),
}

impl FormatType {
    pub fn read(&self, out: &mut impl Write) -> io::Result<()> {
        match self {
            FormatType::Append(inner, size) => {
                write!(out, "{{ let inner = ")?;
                inner.read(out)?;
                write!(out, "; u{}::read(reader)?; inner }}", size)?;
            }
            FormatType::Pad(inner) => {
                write!(out, "{{ let inner = ")?;
                inner.read(out)?;
                write!(out, "; reader.pad4()?; inner }}")?;
            }
            FormatType::List(inner, len) => {
                writeln!(out, "{{ let mut out = Vec::new(); let len = u{}::read(reader)? as usize; let end = reader.cursor() - len; while reader.cursor() > end {{", len)?;
                write!(out, "out.push(")?;
                inner.read(out)?;
                write!(out, ");")?;
                write!(out, "}}")?;
                write!(out, "out }}")?;
            }
            FormatType::String(len) => {
                writeln!(out, "{{ let len = u{}::read(reader)?;", len)?;
                writeln!(
                    out,
                    "let bytes = reader.consume(len as usize)?; XimString(bytes)"
                )?;
                writeln!(out, "}}")?
            }
            FormatType::Normal(name) => write!(out, "{}::read(reader)?", name)?,
        }

        Ok(())
    }

    pub fn write(&self, this: &str, out: &mut impl Write) -> io::Result<()> {
        match self {
            FormatType::Append(inner, size) => {
                inner.write(this, out)?;
                writeln!(out, "0u{}.write(writer);", size)?;
            }
            FormatType::List(inner, len) => {
                write!(out, "((")?;
                self.size(this, out)?;
                writeln!(
                    out,
                    " - {len_size}) as u{len}).write(writer);",
                    len_size = len / 8,
                    len = len
                )?;
                writeln!(out, "for elem in {}.iter() {{", this)?;
                inner.write("elem", out)?;
                writeln!(out, "}}")?;
            }
            FormatType::Pad(inner) => {
                inner.write(this, out)?;
                writeln!(out, "writer.write_pad4();")?;
            }
            FormatType::String(len) => {
                writeln!(out, "({}.0.len() as u{}).write(writer);", this, len)?;
                writeln!(out, "writer.write({}.0);", this)?;
            }
            FormatType::Normal(_name) => write!(out, "{}.write(writer);", this)?,
        }

        Ok(())
    }

    pub fn size(&self, this: &str, out: &mut impl Write) -> io::Result<()> {
        match self {
            FormatType::Append(inner, size) => {
                inner.size(this, out)?;
                write!(out, "+ {}", size / 8)
            }
            FormatType::String(len) => {
                write!(out, "{}.0.len() + {}", this, len / 8)
            }
            FormatType::List(inner, len) => {
                write!(out, "{}.iter().map(|e| ", this)?;
                inner.size("e", out)?;
                write!(out, ").sum::<usize>() + {}", len / 8)
            }
            FormatType::Pad(inner) => {
                write!(out, "pad4(")?;
                inner.size(this, out)?;
                write!(out, ")")
            }
            FormatType::Normal(_inner) => write!(out, "{}.size()", this),
        }
    }
}

impl fmt::Display for FormatType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatType::Append(inner, _len) => inner.fmt(f),
            FormatType::Pad(inner) => inner.fmt(f),
            FormatType::List(inner, _len) => write!(f, "Vec<{}>", inner),
            FormatType::String(_len) => f.write_str("XimString<'b>"),
            FormatType::Normal(name) => f.write_str(name),
        }
    }
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        let pos = text
            .find(" ")
            .ok_or_else(|| D::Error::custom("Can't parse field"))?;
        let (name, left) = text.split_at(pos);

        Ok(Self {
            name: name.into(),
            ty: left.parse().map_err(D::Error::custom)?,
        })
    }
}

impl std::str::FromStr for FormatType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim_start();

        if let Some(left) = s.strip_prefix("@pad") {
            Ok(Self::Pad(Box::new(left.parse()?)))
        } else if let Some(left) = s.strip_prefix("@list") {
            Ok(Self::List(Box::new(left.parse()?), 16))
        } else if let Some(left) = s.strip_prefix("@append8") {
            Ok(Self::Append(Box::new(left.parse()?), 8))
        } else if s.starts_with("string8") {
            Ok(Self::String(8))
        } else if s.starts_with("string") {
            Ok(Self::String(16))
        } else {
            if s.starts_with("@") {
                Err("Invalid format command")
            } else {
                Ok(Self::Normal(s.into()))
            }
        }
    }
}
