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
    Pad(Box<Self>, usize),
    List(Box<Self>, usize, usize),
    String { between_unused: usize, len: usize },
    XString,
    Normal(String),
}

impl FormatType {
    pub fn read(&self, out: &mut impl Write) -> io::Result<()> {
        match self {
            FormatType::Append(inner, size) => {
                write!(out, "{{ let inner = ")?;
                inner.read(out)?;
                write!(out, "; reader.consume({})?; inner }}", size)?;
            }
            FormatType::Pad(inner, _size_sub) => {
                write!(out, "{{ let inner = ")?;
                inner.read(out)?;
                write!(out, "; reader.pad4()?; inner }}")?;
            }
            FormatType::List(inner, prefix, len) => {
                writeln!(out, "{{ let mut out = Vec::new(); let len = u{}::read(reader)? as usize; let end = reader.cursor() - len;", len * 8)?;
                if *prefix > 0 {
                    writeln!(out, "u{}::read(reader)?;", prefix * 8)?;
                }
                writeln!(out, "while reader.cursor() > end {{")?;
                write!(out, "out.push(")?;
                inner.read(out)?;
                write!(out, ");")?;
                write!(out, "}}")?;
                write!(out, "out }}")?;
            }
            FormatType::XString => {
                writeln!(
                    out,
                    "{{ let len = u16::read(reader)?; reader.consume(len as usize)?.to_vec() }}"
                )?;
            }
            FormatType::String {
                len,
                between_unused,
            } => {
                writeln!(out, "{{ let len = u{}::read(reader)?;", len * 8)?;
                if *between_unused > 0 {
                    writeln!(out, "reader.consume({})?;", between_unused)?;
                }
                writeln!(out, "reader.consume(len as usize)?.to_vec().into()")?;
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
                writeln!(out, "writer.write(&[0u8; {}]);", size)?;
            }
            FormatType::List(inner, prefix, len) => {
                write!(out, "((")?;
                self.size(this, out)?;
                writeln!(
                    out,
                    " - {} - {}) as u{}).write(writer);",
                    len,
                    prefix,
                    len * 8,
                )?;

                if *prefix > 0 {
                    writeln!(out, "0u{}.write(writer);", prefix * 8)?;
                }

                writeln!(out, "for elem in {}.iter() {{", this)?;
                inner.write("elem", out)?;
                writeln!(out, "}}")?;
            }
            FormatType::Pad(inner, _size_sub) => {
                inner.write(this, out)?;
                writeln!(out, "writer.write_pad4();")?;
            }
            FormatType::XString => {
                writeln!(out, "({}.len() as u16).write(writer);", this)?;
                writeln!(out, "writer.write(&{});", this)?
            }
            FormatType::String {
                len,
                between_unused,
            } => {
                writeln!(out, "({}.len() as u{}).write(writer);", this, len * 8)?;
                if *between_unused > 0 {
                    writeln!(out, "writer.write(&[0u8; {}]);", between_unused)?;
                }
                writeln!(out, "writer.write({}.as_bytes());", this)?;
            }
            FormatType::Normal(_name) => write!(out, "{}.write(writer);", this)?,
        }

        Ok(())
    }

    pub fn size(&self, this: &str, out: &mut impl Write) -> io::Result<()> {
        match self {
            FormatType::Append(inner, size) => {
                inner.size(this, out)?;
                write!(out, "+ {}", size)
            }
            FormatType::XString => write!(out, "{}.len() + 2", this),
            FormatType::String {
                len,
                between_unused,
            } => {
                write!(out, "{}.len() + {} + {}", this, len, between_unused)
            }
            FormatType::List(inner, prefix, len) => {
                write!(out, "{}.iter().map(|e| ", this)?;
                inner.size("e", out)?;
                write!(out, ").sum::<usize>() + {} + {}", prefix, len)
            }
            FormatType::Pad(inner, size_add) => {
                write!(out, "with_pad4(")?;
                inner.size(this, out)?;
                write!(out, ")")?;
                if *size_add > 0 {
                    write!(out, " + {}", size_add)
                } else {
                    Ok(())
                }
            }
            FormatType::Normal(_inner) => write!(out, "{}.size()", this),
        }
    }
}

impl fmt::Display for FormatType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatType::Append(inner, _len) => inner.fmt(f),
            FormatType::Pad(inner, ..) => inner.fmt(f),
            FormatType::List(inner, _prefix, _len) => write!(f, "Vec<{}>", inner),
            FormatType::XString => f.write_str("Vec<u8>"),
            FormatType::String { .. } => f.write_str("BString"),
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
        if let Some(left) = s.strip_prefix("@padadd2") {
            Ok(Self::Pad(Box::new(left.parse()?), 2))
        } else if let Some(left) = s.strip_prefix("@pad") {
            Ok(Self::Pad(Box::new(left.parse()?), 0))
        } else if let Some(mut left) = s.strip_prefix("@list") {
            let mut prefix = 0;
            let mut len = 2;

            if left.chars().next().unwrap().is_numeric() {
                let (num, new_left) = left.split_at(2);
                left = new_left;
                let num = num.parse::<usize>().or_else(|_| Err("Invalid number"))?;
                prefix = num / 10;
                len = num % 10;
            }

            Ok(Self::List(Box::new(left.parse()?), prefix, len))
        } else if let Some(left) = s.strip_prefix("@append") {
            let (n, left) = left.split_at(1);
            Ok(Self::Append(
                Box::new(left.parse()?),
                n.parse().or_else(|_| Err("@append need number!"))?,
            ))
        } else if s.starts_with("xstring") {
            Ok(Self::XString)
        } else if s.starts_with("err_string") {
            Ok(Self::String {
                len: 2,
                between_unused: 2,
            })
        } else if s.starts_with("string1") {
            Ok(Self::String {
                len: 1,
                between_unused: 0,
            })
        } else if s.starts_with("string") {
            Ok(Self::String {
                len: 2,
                between_unused: 0,
            })
        } else {
            if s.starts_with("@") {
                Err("Invalid format command")
            } else {
                Ok(Self::Normal(s.into()))
            }
        }
    }
}
