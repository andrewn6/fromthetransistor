// tiny syntax highlighters for the editor. they never fail on half-typed
// code, unlike the real lexer, which is why they are separate.

use crate::theme;
use eframe::egui::{text::LayoutJob, Color32, TextFormat};

const C_KEYWORDS: &[&str] = &[
    "int", "char", "void", "if", "while", "return", "else", "for", "break", "continue", "struct",
    "typedef", "sizeof", "const", "unsigned", "signed", "long", "short",
];

const COMMENT: Color32 = Color32::from_rgb(0x5c, 0x67, 0x77);
const PREPROC: Color32 = Color32::from_rgb(0xc7, 0x8b, 0xe8);

fn push(job: &mut LayoutJob, text: &str, color: Color32, size: f32) {
    job.append(text, 0.0, TextFormat { font_id: theme::mono(size), color, ..Default::default() });
}

pub fn c(src: &str, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let b: Vec<char> = src.chars().collect();
    let mut i = 0;
    let at_line_start = |i: usize| -> bool {
        let mut j = i;
        while j > 0 && (b[j - 1] == ' ' || b[j - 1] == '\t') {
            j -= 1;
        }
        j == 0 || b[j - 1] == '\n'
    };
    while i < b.len() {
        let start = i;
        let c = b[i];
        let (end, color) = if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            (i, COMMENT)
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i < b.len() && !(b[i] == '*' && b.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i = (i + 2).min(b.len());
            (i, COMMENT)
        } else if c == '#' && at_line_start(i) {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            (i, PREPROC)
        } else if c == '"' || c == '\'' {
            i += 1;
            while i < b.len() && b[i] != c && b[i] != '\n' {
                if b[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(b.len());
            (i, theme::token_color("strl"))
        } else if c.is_ascii_digit() {
            while i < b.len() && b[i].is_ascii_alphanumeric() {
                i += 1;
            }
            (i, theme::token_color("intl"))
        } else if c.is_alphabetic() || c == '_' {
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            let w: String = b[start..i].iter().collect();
            let next_paren = b[i..].iter().find(|c| !c.is_whitespace()) == Some(&'(');
            let color = if C_KEYWORDS.contains(&w.as_str()) {
                theme::token_color("kw")
            } else if next_paren {
                Color32::from_rgb(0x9c, 0xd7, 0xff)
            } else {
                theme::TEXT
            };
            (i, color)
        } else if "+-*/%<>=!&|^~".contains(c) {
            i += 1;
            (i, theme::token_color("o"))
        } else {
            i += 1;
            (i, if "(){}[];,.".contains(c) { theme::DIM } else { theme::TEXT })
        };
        let s: String = b[start..end].iter().collect();
        push(&mut job, &s, color, size);
    }
    job
}

pub fn asm(src: &str, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for line in src.split_inclusive('\n') {
        let (code, comment) = match line.find('@') {
            Some(i) => (&line[..i], &line[i..]),
            None => (line, ""),
        };
        asm_code(&mut job, code, size);
        if !comment.is_empty() {
            push(&mut job, comment, COMMENT, size);
        }
    }
    job
}

fn asm_code(job: &mut LayoutJob, code: &str, size: f32) {
    let mut first_word = true;
    let mut rest = code;
    while !rest.is_empty() {
        let ws = rest.len() - rest.trim_start().len();
        if ws > 0 {
            push(job, &rest[..ws], theme::TEXT, size);
            rest = &rest[ws..];
            continue;
        }
        let end = rest.find(|c: char| c.is_whitespace() || c == ',').unwrap_or(rest.len()).max(1);
        let word = &rest[..end];
        let color = if word.ends_with(':') {
            theme::LINK
        } else if word.starts_with('.') && first_word {
            PREPROC
        } else if first_word {
            theme::ASM
        } else if word.starts_with('#') || word.starts_with('=') || word.starts_with(|c: char| c.is_ascii_digit()) {
            theme::token_color("intl")
        } else if word.starts_with('"') {
            theme::token_color("strl")
        } else if word.starts_with(['r', '[', '{']) || matches!(word, "sp" | "lr" | "pc" | "fp" | "ip") {
            theme::TOKENS
        } else {
            theme::TEXT
        };
        if !word.ends_with(':') && word != "," {
            first_word = false;
        }
        push(job, word, color, size);
        rest = &rest[end..];
    }
}
