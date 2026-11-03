//! Tags as commands write them (`/data merge block`, `/setblock`, `/summon`).

use crate::nbt::Tag;
use std::fmt::Write;

pub fn to_snbt(tag: &Tag) -> String {
    let mut out = String::new();
    write(&mut out, tag);
    out
}

fn bare(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+'))
}

fn quoted(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn float(out: &mut String, v: f64, suffix: char) {
    if v.is_finite() {
        let _ = write!(out, "{v:?}{suffix}");
    } else {
        let _ = write!(out, "0{suffix}");
    }
}

fn write(out: &mut String, tag: &Tag) {
    match tag {
        Tag::Byte(v) => {
            let _ = write!(out, "{v}b");
        }
        Tag::Short(v) => {
            let _ = write!(out, "{v}s");
        }
        Tag::Int(v) => {
            let _ = write!(out, "{v}");
        }
        Tag::Long(v) => {
            let _ = write!(out, "{v}L");
        }
        Tag::Float(v) => float(out, f64::from(*v), 'f'),
        Tag::Double(v) => float(out, *v, 'd'),
        Tag::String(s) => quoted(out, s),
        Tag::ByteArray(v) => array(out, 'B', v.iter().map(|b| format!("{b}b"))),
        Tag::IntArray(v) => array(out, 'I', v.iter().map(i32::to_string)),
        Tag::LongArray(v) => array(out, 'L', v.iter().map(|l| format!("{l}L"))),
        Tag::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(out, item);
            }
            out.push(']');
        }
        Tag::Compound(c) => {
            out.push('{');
            for (i, (k, v)) in c.0.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if bare(k) {
                    out.push_str(k);
                } else {
                    quoted(out, k);
                }
                out.push(':');
                write(out, v);
            }
            out.push('}');
        }
    }
}

fn array(out: &mut String, kind: char, items: impl Iterator<Item = String>) {
    let _ = write!(out, "[{kind};");
    for (i, item) in items.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&item);
    }
    out.push(']');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::compound;

    #[test]
    fn writes_typed_values_and_quotes_keys() {
        let tag = compound([
            ("Items", Tag::List(vec![compound([("id", Tag::String("minecraft:stone".into())), ("count", Tag::Int(3))])])),
            ("odd key", Tag::Byte(1)),
            ("f", Tag::Float(0.5)),
            ("l", Tag::LongArray(vec![1, 2])),
            ("say", Tag::String("a \"b\" \\".into())),
        ]);
        assert_eq!(
            to_snbt(&tag),
            r#"{Items:[{id:"minecraft:stone",count:3}],"odd key":1b,f:0.5f,l:[L;1L,2L],say:"a \"b\" \\"}"#
        );
    }
}
