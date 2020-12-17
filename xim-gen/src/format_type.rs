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
    List(Box<Self>, usize, usize),
    String { between_unused: usize, len: usize },
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
            FormatType::Pad(inner) => {
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
            FormatType::String {
                len,
                between_unused,
            } => {
                writeln!(out, "{{ let len = u{}::read(reader)?;", len * 8)?;
                if *between_unused > 0 {
                    writeln!(out, "reader.consume({})?;", between_unused)?;
                }
                writeln!(
                    out,
                    "let mut bytes = reader.consume(len as usize)?;
                    match bytes.split_last() {{
                        Some((b, left)) if *b == 0 => bytes = left,
                        _ => {{}}
                    }}
                    XimString(bytes.to_vec())"
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
            FormatType::Pad(inner) => {
                inner.write(this, out)?;
                writeln!(out, "writer.write_pad4();")?;
            }
            FormatType::String {
                len,
                between_unused,
            } => {
                writeln!(out, "({}.0.len() as u{}).write(writer);", this, len * 8)?;
                if *between_unused > 0 {
                    writeln!(out, "writer.write(&[0u8; {}]);", between_unused)?;
                }
                writeln!(out, "writer.write(&{}.0);", this)?;
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
            FormatType::String {
                len,
                between_unused,
            } => {
                write!(out, "{}.0.len() + {} + {}", this, len, between_unused)
            }
            FormatType::List(inner, prefix, len) => {
                write!(out, "{}.iter().map(|e| ", this)?;
                inner.size("e", out)?;
                write!(out, ").sum::<usize>() + {} + {}", prefix, len)
            }
            FormatType::Pad(inner) => {
                write!(out, "with_pad4(")?;
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
            FormatType::List(inner, _prefix, _len) => write!(f, "Vec<{}>", inner),
            FormatType::String { .. } => f.write_str("XimString"),
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
