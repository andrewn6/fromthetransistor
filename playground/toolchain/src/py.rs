// small helpers that reproduce python semantics the original toolchain
// relies on, so the rust port gives byte-identical output.

use std::collections::HashMap;
use std::fmt;

// python exception kind + message, e.g. "ValueError: asm: bad register 'r99'"
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub kind: &'static str,
    pub msg: String,
}

impl Error {
    pub fn new(kind: &'static str, msg: impl Into<String>) -> Self {
        Error { kind, msg: msg.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.msg)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn value_error(msg: impl Into<String>) -> Error {
    Error::new("ValueError", msg)
}
pub fn syntax_error(msg: impl Into<String>) -> Error {
    Error::new("SyntaxError", msg)
}
pub fn type_error(msg: impl Into<String>) -> Error {
    Error::new("TypeError", msg)
}
pub fn key_error(key: &str) -> Error {
    Error::new("KeyError", repr(key))
}
pub fn not_impl(msg: impl Into<String>) -> Error {
    Error::new("NotImplementedError", msg)
}
pub fn index_error() -> Error {
    Error::new("IndexError", "string index out of range")
}

// python repr() of a str
pub fn repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::new();
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 32 || c as u32 == 127 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

// python int(s, 0): optional sign, 0x/0o/0b prefixes, underscores between
// digits, and no leading zeros on a non-zero decimal.
pub fn int0(s: &str) -> Result<i128> {
    parse_int0(s).ok_or_else(|| value_error(format!("invalid literal for int() with base 0: {}", repr(s))))
}

fn parse_int0(s: &str) -> Option<i128> {
    let t = s.trim();
    let (neg, body) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let lower = body.to_ascii_lowercase();
    let (radix, digits, prefixed) = if lower.starts_with("0x") {
        (16, &body[2..], true)
    } else if lower.starts_with("0o") {
        (8, &body[2..], true)
    } else if lower.starts_with("0b") {
        (2, &body[2..], true)
    } else {
        (10, body, false)
    };
    // underscores: allowed right after a prefix, otherwise only between digits
    let digits = if prefixed { digits.strip_prefix('_').unwrap_or(digits) } else { digits };
    if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return None;
    }
    let clean: String = digits.chars().filter(|&c| c != '_').collect();
    if !clean.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    if radix == 10 && clean.len() > 1 && clean.starts_with('0') && clean.chars().any(|c| c != '0') {
        return None;
    }
    let v = i128::from_str_radix(&clean, radix).ok()?;
    Some(if neg { -v } else { v })
}

// struct.pack("<I", v): errors when v does not fit an unsigned 32-bit word
pub fn u32_checked(v: i128) -> Result<u32> {
    if (0..=0xFFFF_FFFF).contains(&v) {
        Ok(v as u32)
    } else {
        Err(Error::new("error", "argument out of range"))
    }
}

// python str.splitlines() for the line breaks that show up in source text
pub fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            b'\r' => {
                out.push(&s[start..i]);
                if i + 1 < b.len() && b[i + 1] == b'\n' {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < b.len() {
        out.push(&s[start..]);
    }
    out
}

// python str.split(None, 1): first word, rest with leading whitespace removed
pub fn split_ws1(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], s[i..].trim_start()),
        None => (s, ""),
    }
}

// insertion-ordered map with python dict semantics (overwrite keeps position)
#[derive(Clone, Debug)]
pub struct OrderedMap<V> {
    keys: Vec<String>,
    vals: Vec<V>,
    index: HashMap<String, usize>,
}

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> OrderedMap<V> {
    pub fn new() -> Self {
        OrderedMap { keys: Vec::new(), vals: Vec::new(), index: HashMap::new() }
    }
    pub fn insert(&mut self, k: &str, v: V) {
        if let Some(&i) = self.index.get(k) {
            self.vals[i] = v;
        } else {
            self.index.insert(k.to_string(), self.keys.len());
            self.keys.push(k.to_string());
            self.vals.push(v);
        }
    }
    pub fn get(&self, k: &str) -> Option<&V> {
        self.index.get(k).map(|&i| &self.vals[i])
    }
    pub fn get_mut(&mut self, k: &str) -> Option<&mut V> {
        match self.index.get(k) {
            Some(&i) => Some(&mut self.vals[i]),
            None => None,
        }
    }
    pub fn contains(&self, k: &str) -> bool {
        self.index.contains_key(k)
    }
    pub fn len(&self) -> usize {
        self.keys.len()
    }
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
    pub fn clear(&mut self) {
        self.keys.clear();
        self.vals.clear();
        self.index.clear();
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.keys.iter().map(|k| k.as_str()).zip(self.vals.iter())
    }
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.vals.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int0_matches_python() {
        assert_eq!(int0("0x10").unwrap(), 16);
        assert_eq!(int0(" -4 ").unwrap(), -4);
        assert_eq!(int0("0").unwrap(), 0);
        assert_eq!(int0("000").unwrap(), 0);
        assert_eq!(int0("0x_1f").unwrap(), 31);
        assert_eq!(int0("1_000").unwrap(), 1000);
        assert!(int0("010").is_err());
        assert!(int0("").is_err());
        assert!(int0("main").is_err());
        assert!(int0("1__0").is_err());
        assert!(int0("0x").is_err());
    }

    #[test]
    fn repr_matches_python() {
        assert_eq!(repr("abc"), "'abc'");
        assert_eq!(repr("it's"), "\"it's\"");
        assert_eq!(repr("a\nb"), "'a\\nb'");
    }
}
