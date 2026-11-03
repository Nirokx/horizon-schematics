//! Named Binary Tag: Java's big-endian files (gzip, zlib or raw) and
//! Bedrock's little-endian ones, read and written.

use std::fmt;
use std::io::{Read, Write};

/// A tag. Compounds keep their keys in file order.
#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<Tag>),
    Compound(Compound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

/// A compound's entries, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Compound(pub Vec<(String, Tag)>);

impl Compound {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn get(&self, key: &str) -> Option<&Tag> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Tag> {
        self.0.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    /// Sets `key`, replacing an entry of the same key in place.
    pub fn insert(&mut self, key: impl Into<String>, value: Tag) {
        let key = key.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => *v = value,
            None => self.0.push((key, value)),
        }
    }
    pub fn remove(&mut self, key: &str) -> Option<Tag> {
        let at = self.0.iter().position(|(k, _)| k == key)?;
        Some(self.0.remove(at).1)
    }
    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Tag)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn i32(&self, key: &str) -> Option<i32> {
        self.get(key)?.as_i32()
    }
    pub fn i64(&self, key: &str) -> Option<i64> {
        self.get(key)?.as_i64()
    }
    pub fn f64(&self, key: &str) -> Option<f64> {
        self.get(key)?.as_f64()
    }
    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }
    pub fn compound(&self, key: &str) -> Option<&Compound> {
        self.get(key)?.as_compound()
    }
    pub fn list(&self, key: &str) -> Option<&[Tag]> {
        self.get(key)?.as_list()
    }
}

impl Tag {
    pub fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(_) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }
    /// Any whole number as an `i32` (a byte, short or int; a long that fits).
    pub fn as_i32(&self) -> Option<i32> {
        match *self {
            Tag::Byte(v) => Some(v.into()),
            Tag::Short(v) => Some(v.into()),
            Tag::Int(v) => Some(v),
            Tag::Long(v) => i32::try_from(v).ok(),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Tag::Long(v) => Some(v),
            _ => self.as_i32().map(i64::from),
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Tag::Float(v) => Some(v.into()),
            Tag::Double(v) => Some(v),
            _ => self.as_i64().map(|v| v as f64),
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_compound(&self) -> Option<&Compound> {
        match self {
            Tag::Compound(c) => Some(c),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&[Tag]> {
        match self {
            Tag::List(l) => Some(l),
            _ => None,
        }
    }
    /// An int array, or a list of whole numbers, as `i32`s.
    pub fn as_ints(&self) -> Option<Vec<i32>> {
        match self {
            Tag::IntArray(v) => Some(v.clone()),
            Tag::ByteArray(v) => Some(v.iter().map(|&b| b.into()).collect()),
            Tag::List(l) => l.iter().map(Tag::as_i32).collect(),
            _ => None,
        }
    }
    /// A list of numbers as `f64`s.
    pub fn as_doubles(&self) -> Option<Vec<f64>> {
        match self {
            Tag::List(l) => l.iter().map(Tag::as_f64).collect(),
            Tag::IntArray(v) => Some(v.iter().map(|&i| i.into()).collect()),
            _ => None,
        }
    }
    pub fn get(&self, key: &str) -> Option<&Tag> {
        self.as_compound()?.get(key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endian {
    /// Java Edition's files.
    Big,
    /// Bedrock Edition's files (`.mcstructure`).
    Little,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn err<T>(what: impl Into<String>) -> Result<T> {
    Err(Error(what.into()))
}

/// The bytes of a file, inflated when it is gzip or zlib compressed.
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.starts_with(&[0x1F, 0x8B]) {
        let mut out = Vec::new();
        flate2::read::MultiGzDecoder::new(bytes)
            .read_to_end(&mut out)
            .map_err(|e| Error(format!("not a valid gzip file: {e}")))?;
        Ok(out)
    } else if bytes.len() > 2 && bytes[0] == 0x78 && (u16::from(bytes[0]) << 8 | u16::from(bytes[1])) % 31 == 0 {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(bytes)
            .read_to_end(&mut out)
            .map_err(|e| Error(format!("not a valid zlib stream: {e}")))?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

/// The root tag of a file (compressed or not), with its name.
pub fn read_file(bytes: &[u8], endian: Endian) -> Result<(String, Tag)> {
    let raw = decompress(bytes)?;
    read_root(&raw, endian)
}

/// The root tag of uncompressed NBT.
pub fn read_root(bytes: &[u8], endian: Endian) -> Result<(String, Tag)> {
    let mut r = Reader { bytes, at: 0, endian, depth: 0 };
    let id = r.u8()?;
    if id != 10 {
        return err(format!("the file does not start with a compound tag (tag {id})"));
    }
    let name = r.string()?;
    let tag = r.payload(id)?;
    Ok((name, tag))
}

/// A tag as the network writes it: its type, then its payload, no name.
pub fn read_unnamed(bytes: &[u8]) -> Result<Tag> {
    let mut r = Reader { bytes, at: 0, endian: Endian::Big, depth: 0 };
    let id = r.u8()?;
    r.payload(id)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    endian: Endian,
    depth: u32,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        if self.bytes.len() - self.at < n {
            return err("the file ends in the middle of a tag");
        }
        let s = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut a: [u8; N] = self.take(N)?.try_into().unwrap();
        if self.endian == Endian::Little {
            a.reverse();
        }
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.array()?))
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.array()?))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(self.array()?))
    }
    fn len(&mut self, item: usize) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 {
            return err("a negative array length");
        }
        let n = n as usize;
        if n.saturating_mul(item) > self.bytes.len() - self.at {
            return err("an array longer than the file");
        }
        Ok(n)
    }
    fn string(&mut self) -> Result<String> {
        let n = self.u16()? as usize;
        let endian = self.endian;
        let raw = self.take(n)?;
        Ok(match endian {
            Endian::Big => java_utf8(raw),
            Endian::Little => String::from_utf8_lossy(raw).into_owned(),
        })
    }
    fn payload(&mut self, id: u8) -> Result<Tag> {
        Ok(match id {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.len(1)?;
                Tag::ByteArray(self.take(n)?.iter().map(|&b| b as i8).collect())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let item = self.u8()?;
                let n = self.i32()?.max(0) as usize;
                if item == 0 || n == 0 {
                    return Ok(Tag::List(Vec::new()));
                }
                if n > self.bytes.len() - self.at {
                    return err("a list longer than the file");
                }
                self.nest()?;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push(self.payload(item)?);
                }
                self.depth -= 1;
                Tag::List(items)
            }
            10 => {
                self.nest()?;
                let mut c = Compound::new();
                loop {
                    let item = self.u8()?;
                    if item == 0 {
                        break;
                    }
                    let key = self.string()?;
                    let value = self.payload(item)?;
                    c.0.push((key, value));
                }
                self.depth -= 1;
                Tag::Compound(c)
            }
            11 => {
                let n = self.len(4)?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(self.i32()?);
                }
                Tag::IntArray(v)
            }
            12 => {
                let n = self.len(8)?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(self.i64()?);
                }
                Tag::LongArray(v)
            }
            _ => return err(format!("unknown tag type {id}")),
        })
    }
    fn nest(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > 512 {
            return err("tags nested too deep");
        }
        Ok(())
    }
}

/// Java's modified UTF-8 (`DataInput.readUTF`): nulls as two bytes and
/// characters outside the BMP as surrogate pairs.
fn java_utf8(raw: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.to_owned();
    }
    let mut units: Vec<u16> = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        if b < 0x80 {
            units.push(b.into());
            i += 1;
        } else if b & 0xE0 == 0xC0 && i + 1 < raw.len() {
            units.push(u16::from(b & 0x1F) << 6 | u16::from(raw[i + 1] & 0x3F));
            i += 2;
        } else if b & 0xF0 == 0xE0 && i + 2 < raw.len() {
            units.push(u16::from(b & 0x0F) << 12 | u16::from(raw[i + 1] & 0x3F) << 6 | u16::from(raw[i + 2] & 0x3F));
            i += 3;
        } else {
            units.push(0xFFFD);
            i += 1;
        }
    }
    String::from_utf16_lossy(&units)
}

fn java_utf8_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for unit in s.encode_utf16() {
        match unit {
            0x0001..=0x007F => out.push(unit as u8),
            0x0000 | 0x0080..=0x07FF => {
                out.push(0xC0 | (unit >> 6) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
            _ => {
                out.push(0xE0 | (unit >> 12) as u8);
                out.push(0x80 | (unit >> 6 & 0x3F) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
        }
    }
    out
}

/// Big-endian, uncompressed NBT of `tag` as a root named `name`.
pub fn write_root(name: &str, tag: &Tag) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(tag.id());
    write_string(&mut out, name);
    write_payload(&mut out, tag);
    out
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    let mut bytes = java_utf8_bytes(s);
    bytes.truncate(u16::MAX as usize);
    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(&bytes);
}

fn write_payload(out: &mut Vec<u8>, tag: &Tag) {
    match tag {
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Float(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        Tag::Double(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        Tag::ByteArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            out.extend(v.iter().map(|&b| b as u8));
        }
        Tag::String(s) => write_string(out, s),
        Tag::List(items) => {
            out.push(items.first().map_or(0, Tag::id));
            out.extend_from_slice(&(items.len() as i32).to_be_bytes());
            for item in items {
                write_payload(out, item);
            }
        }
        Tag::Compound(c) => {
            for (k, v) in &c.0 {
                out.push(v.id());
                write_string(out, k);
                write_payload(out, v);
            }
            out.push(0);
        }
        Tag::IntArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for i in v {
                out.extend_from_slice(&i.to_be_bytes());
            }
        }
        Tag::LongArray(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for i in v {
                out.extend_from_slice(&i.to_be_bytes());
            }
        }
    }
}

/// A compound from `(key, tag)` pairs.
pub fn compound<const N: usize>(entries: [(&str, Tag); N]) -> Tag {
    Tag::Compound(Compound(entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Tag {
        compound([
            ("byte", Tag::Byte(-3)),
            ("short", Tag::Short(1234)),
            ("int", Tag::Int(-70000)),
            ("long", Tag::Long(1 << 40)),
            ("float", Tag::Float(1.5)),
            ("double", Tag::Double(-2.25)),
            ("bytes", Tag::ByteArray(vec![1, -1, 3])),
            ("text", Tag::String("héllo \u{0} 🎉".into())),
            ("list", Tag::List(vec![Tag::Int(1), Tag::Int(2)])),
            ("empty", Tag::List(vec![])),
            ("ints", Tag::IntArray(vec![5, -6])),
            ("longs", Tag::LongArray(vec![i64::MIN, 7])),
            ("nested", compound([("x", Tag::Int(1))])),
        ])
    }

    #[test]
    fn round_trips_big_endian_and_gzip() {
        let tag = sample();
        let raw = write_root("root", &tag);
        assert_eq!(read_root(&raw, Endian::Big).unwrap(), ("root".into(), tag.clone()));
        assert_eq!(read_file(&gzip(&raw), Endian::Big).unwrap().1, tag);
    }

    #[test]
    fn modified_utf8_nulls_and_supplementary_characters() {
        let bytes = java_utf8_bytes("a\u{0}🎉");
        assert_eq!(&bytes[..3], &[b'a', 0xC0, 0x80]);
        assert_eq!(bytes.len(), 1 + 2 + 6);
        assert_eq!(java_utf8(&bytes), "a\u{0}🎉");
    }

    #[test]
    fn reads_little_endian() {
        // A compound "" { Int "a" = 1 } in Bedrock's byte order.
        let raw = [10, 0, 0, 3, 1, 0, b'a', 1, 0, 0, 0, 0];
        let (_, tag) = read_root(&raw, Endian::Little).unwrap();
        assert_eq!(tag.get("a"), Some(&Tag::Int(1)));
    }

    #[test]
    fn refuses_truncated_files() {
        let raw = write_root("", &sample());
        assert!(read_root(&raw[..raw.len() - 3], Endian::Big).is_err());
        assert!(read_root(&[10, 0, 0, 7, 0, 1, b'b', 0x7F, 0xFF, 0xFF, 0xFF], Endian::Big).is_err());
    }
}
