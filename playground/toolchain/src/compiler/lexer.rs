// preprocessor + tokenizer, ported from compiler.py

use crate::py::{self, index_error, syntax_error, OrderedMap, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Kw(String),
    Id(String),
    IntL(i128),
    ChrL(char),
    StrL(String),
    P(char),
    O(&'static str),
}

impl Tok {
    pub fn kind(&self) -> &'static str {
        match self {
            Tok::Kw(_) => "kw",
            Tok::Id(_) => "id",
            Tok::IntL(_) => "intl",
            Tok::ChrL(_) => "chrl",
            Tok::StrL(_) => "strl",
            Tok::P(_) => "p",
            Tok::O(_) => "o",
        }
    }

    // the token's value as python would show it with !r
    pub fn val_repr(&self) -> String {
        match self {
            Tok::Kw(s) | Tok::Id(s) | Tok::StrL(s) => py::repr(s),
            Tok::IntL(n) => n.to_string(),
            Tok::ChrL(c) | Tok::P(c) => py::repr(&c.to_string()),
            Tok::O(o) => py::repr(o),
        }
    }

    // the token's value as plain text, for the playground's token chips
    pub fn text(&self) -> String {
        match self {
            Tok::Kw(s) | Tok::Id(s) => s.clone(),
            Tok::StrL(s) => format!("{:?}", s),
            Tok::IntL(n) => n.to_string(),
            Tok::ChrL(c) => format!("{:?}", c),
            Tok::P(c) => c.to_string(),
            Tok::O(o) => o.to_string(),
        }
    }

    pub fn repr(&self) -> String {
        format!("{}:{}", self.kind(), self.val_repr())
    }

    pub fn is_kw(&self, k: &str) -> bool {
        matches!(self, Tok::Kw(s) if s == k)
    }
    pub fn is_p(&self, p: char) -> bool {
        matches!(self, Tok::P(c) if *c == p)
    }
    pub fn is_o(&self, o: &str) -> bool {
        matches!(self, Tok::O(s) if *s == o)
    }
}

// reserved words so identifiers do not capture them
pub const KEYWORDS: [&str; 18] = [
    "int", "char", "void", "if", "while", "return", "else", "for", "break", "continue",
    "struct", "typedef", "sizeof", "const", "unsigned", "signed", "long", "short",
];

// ordered so multi-char symbols match before single chars
pub const OPERATORS: &[&str] = &[
    "==", "!=", "<=", ">=", "&&", "||",
    "<<=", ">>=",
    "+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "++", "--", "->",
    "<<", ">>",
    "+", "-", "*", "/", "%", "<", ">", "=", "!", "&", "|", "^", "~",
];

const PUNCT: &str = "(){}[];,.";

fn esc(c: char) -> char {
    // convert an escape code into the actual character
    match c {
        'n' => '\n',
        't' => '\t',
        '0' => '\0',
        c => c, // \\ ' " and anything else map to themselves
    }
}

// where #include looks for files. the cli reads the disk, the browser
// build hands in its in-memory file list.
pub trait SourceFs {
    fn read(&self, path: &str) -> Option<String>;
}

pub struct DiskFs;

impl SourceFs for DiskFs {
    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => "",
    }
}

fn join(base: &str, f: &str) -> String {
    if f.starts_with('/') || base.is_empty() {
        f.to_string()
    } else if base.ends_with('/') {
        format!("{}{}", base, f)
    } else {
        format!("{}/{}", base, f)
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

// re.sub(r"\bNAME\b", val, text) for a plain word name
fn replace_word(text: &str, name: &str, val: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let pat: Vec<char> = name.chars().collect();
    if pat.is_empty() {
        return text.to_string();
    }
    let boundary = |i: usize| -> bool {
        // \b between chars[i-1] and chars[i]
        let before = i > 0 && is_word(chars[i - 1]);
        let after = i < chars.len() && is_word(chars[i]);
        before != after
    };
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let fits = i + pat.len() <= chars.len() && chars[i..i + pat.len()] == pat[..];
        if fits && boundary(i) && boundary(i + pat.len()) {
            out.push_str(val);
            i += pat.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub fn preprocess(src: &str, path: &str, fs: &dyn SourceFs, defines: &mut OrderedMap<String>) -> Result<String> {
    let mut out: Vec<String> = Vec::new();
    for line in py::splitlines(src) {
        let s = line.trim();
        if let Some(rest) = s.strip_prefix("#include") {
            let f = rest.trim();
            if !(f.len() >= 2 && f.starts_with('"') && f.ends_with('"')) {
                return Err(syntax_error(format!("preprocess: only #include \"file\" is supported: {}", py::repr(s))));
            }
            let f = &f[1..f.len() - 1];
            let inc = join(dirname(path), f);
            let text = fs
                .read(&inc)
                .ok_or_else(|| py::Error::new("FileNotFoundError", format!("No such file or directory: {}", py::repr(&inc))))?;
            out.push(preprocess(&text, &inc, fs, defines)?);
        } else if s.starts_with("#define") {
            let parts: Vec<&str> = {
                // s.split(None, 2)
                let (a, rest) = py::split_ws1(s);
                let (b, c) = py::split_ws1(rest);
                [a, b, c].into_iter().filter(|x| !x.is_empty()).collect()
            };
            if parts.len() < 2 {
                return Err(syntax_error(format!("preprocess: bad #define: {}", py::repr(s))));
            }
            let name = parts[1];
            if name.contains('(') {
                return Err(syntax_error(format!("preprocess: function-like macros not supported: {}", py::repr(s))));
            }
            defines.insert(name, parts.get(2).map(|v| v.to_string()).unwrap_or_default());
        } else if s.starts_with('#') {
            return Err(syntax_error(format!("preprocess: unsupported directive: {}", py::repr(s))));
        } else {
            out.push(line.to_string());
        }
    }
    let mut text = out.join("\n");
    for (name, val) in defines.iter() {
        text = replace_word(&text, name, val);
    }
    Ok(text)
}

// a token plus where it came from in the preprocessed text (char offsets)
#[derive(Clone, Debug)]
pub struct Spanned {
    pub tok: Tok,
    pub start: usize,
    pub end: usize,
}

pub fn tokenize(src: &str) -> Result<Vec<Spanned>> {
    let s: Vec<char> = src.chars().collect();
    let n = s.len();
    let at = |i: usize| -> Result<char> { s.get(i).copied().ok_or_else(index_error) };
    let starts = |i: usize, p: &str| -> bool {
        let p: Vec<char> = p.chars().collect();
        i + p.len() <= n && s[i..i + p.len()] == p[..]
    };
    let mut toks = Vec::new();
    let mut i = 0;
    while i < n {
        let c = s[i];
        let start = i;
        // line comment
        if starts(i, "//") {
            while i < n && s[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // block comment
        if starts(i, "/*") {
            i += 2;
            while i < n && !starts(i, "*/") {
                i += 1;
            }
            if i >= n {
                return Err(syntax_error("unterminated /* comment"));
            }
            i += 2;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // integer literal (decimal or 0x hex)
        if c.is_ascii_digit() {
            let mut j = i;
            let tok;
            if c == '0' && i + 1 < n && (s[i + 1] == 'x' || s[i + 1] == 'X') && i + 2 < n && s[i + 2].is_ascii_hexdigit() {
                j = i + 2;
                while j < n && s[j].is_ascii_hexdigit() {
                    j += 1;
                }
                let digits: String = s[i + 2..j].iter().collect();
                tok = Tok::IntL(i128::from_str_radix(&digits, 16).map_err(|_| py::value_error("int too large"))?);
            } else {
                while j < n && s[j].is_ascii_digit() {
                    j += 1;
                }
                let digits: String = s[i..j].iter().collect();
                tok = Tok::IntL(digits.parse::<i128>().map_err(|_| py::value_error("int too large"))?);
            }
            i = j;
            toks.push(Spanned { tok, start, end: i });
            continue;
        }
        // char literal
        if c == '\'' {
            i += 1;
            let ch;
            if at(i)? == '\\' {
                ch = esc(at(i + 1)?);
                i += 2;
            } else {
                ch = at(i)?;
                i += 1;
            }
            if at(i)? != '\'' {
                return Err(syntax_error("lex: malformed character literal"));
            }
            i += 1;
            toks.push(Spanned { tok: Tok::ChrL(ch), start, end: i });
            continue;
        }
        // string literal
        if c == '"' {
            i += 1;
            let mut buf = String::new();
            while i < n && s[i] != '"' {
                if s[i] == '\\' {
                    buf.push(esc(at(i + 1)?));
                    i += 2;
                } else {
                    buf.push(s[i]);
                    i += 1;
                }
            }
            if i >= n {
                return Err(syntax_error("unterminated string"));
            }
            i += 1;
            toks.push(Spanned { tok: Tok::StrL(buf), start, end: i });
            continue;
        }
        // identifier or keyword
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < n && (s[j].is_alphanumeric() || s[j] == '_') {
                j += 1;
            }
            let word: String = s[i..j].iter().collect();
            let tok = if KEYWORDS.contains(&word.as_str()) { Tok::Kw(word) } else { Tok::Id(word) };
            i = j;
            toks.push(Spanned { tok, start, end: i });
            continue;
        }
        if PUNCT.contains(c) {
            i += 1;
            toks.push(Spanned { tok: Tok::P(c), start, end: i });
            continue;
        }
        // operator (longest match first)
        let op = OPERATORS.iter().find(|o| starts(i, o));
        match op {
            Some(o) => {
                i += o.chars().count();
                toks.push(Spanned { tok: Tok::O(o), start, end: i });
            }
            None => return Err(syntax_error(format!("lex: unexpected char {}", py::repr(&c.to_string())))),
        }
    }
    Ok(toks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_replaces_whole_words() {
        assert_eq!(replace_word("ANSWER + ANSWERS + (ANSWER)", "ANSWER", "42"), "42 + ANSWERS + (42)");
    }

    #[test]
    fn lex_ops_longest_first() {
        let t: Vec<Tok> = tokenize("a <<= 1; p->x").unwrap().into_iter().map(|s| s.tok).collect();
        assert_eq!(t[1], Tok::O("<<="));
        assert_eq!(t[5], Tok::O("->"));
    }
}
