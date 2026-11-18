//! Hand-written lexer for Monkey C.
//!
//! The lexer is intentionally permissive: [`lex`] never fails and always
//! ends with an `Eof` token, so formatting and linting tools can still
//! operate on broken files. [`lex_full`] additionally returns lexical
//! diagnostics (unterminated literals/comments, unexpected characters)
//! with precise spans.
//!
//! Scanning is UTF-8 safe: the input is decoded one `char` at a time and
//! all token boundaries land on char boundaries.

use monkey_core::{Diagnostic, Span};

use crate::token::{Token, TokenKind, CONTROL_KEYWORDS, KEYWORDS};

/// Multi-character operators, longest first.
const MULTI_PUNCT: &[&str] = &[
    ">>>=", ">>>", "<<=", ">>=", "==", "!=", "<=", ">=", "&&", "||", "++", "--", "+=", "-=", "*=",
    "/=", "%=", "<<", ">>", "&=", "|=", "^=", "=>",
];

/// Single-character punctuation.
const PUNCT: &[char] = &[
    '(', ')', '{', '}', '[', ']', '<', '>', ',', ';', '.', ':', '+', '-', '*', '/', '%', '=', '!',
    '&', '|', '^', '~', '?', '@', '$',
];

/// Tokens plus lexical diagnostics.
#[derive(Debug)]
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Lex `src` into a token stream ending with an `Eof` token.
///
/// This is the permissive entry point used by the formatter and linter;
/// diagnostics are dropped. Use [`lex_full`] when errors matter.
pub fn lex(src: &str) -> Vec<Token> {
    lex_full(src).tokens
}

/// Lex `src`, returning tokens and lexical diagnostics.
pub fn lex_full(src: &str) -> LexResult {
    let mut lx = Lexer {
        src,
        pos: 0,
        line: 1,
        col: 1,
        tokens: Vec::new(),
        diags: Vec::new(),
        newlines: 0,
        space: false,
    };
    lx.run();
    LexResult {
        tokens: lx.tokens,
        diagnostics: lx.diags,
    }
}

struct Lexer<'a> {
    src: &'a str,
    /// Byte offset of the next char.
    pos: usize,
    /// 1-based line of `pos`.
    line: u32,
    /// 1-based char column of `pos`.
    col: u32,
    tokens: Vec<Token>,
    diags: Vec<Diagnostic>,
    /// `\n` count in whitespace before the next token.
    newlines: usize,
    /// Horizontal whitespace seen before the next token on its line.
    space: bool,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\r' => {
                    self.space = true;
                    self.bump_char();
                }
                '\n' => {
                    self.newlines += 1;
                    self.space = false;
                    self.bump_char();
                }
                '/' if self.peek_at(1) == Some('/') => self.line_comment(),
                '/' if self.peek_at(1) == Some('*') => self.block_comment(),
                '"' => self.string_literal(TokenKind::Str, "string"),
                '\'' => self.string_literal(TokenKind::CharLit, "character"),
                '0'..='9' => self.number(),
                '.' if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => self.number(),
                'a'..='z' | 'A'..='Z' | '_' => self.ident(),
                _ if PUNCT.contains(&c) => self.punct(),
                _ => {
                    // Unexpected character (e.g. stray Unicode symbol).
                    let start = self.pos;
                    let (l, c0) = (self.line, self.col);
                    self.bump_char();
                    self.diags.push(
                        Diagnostic::warning("LEX004", format!("unexpected character '{}'", c))
                            .with_span(Span::new(start as u32, self.pos as u32))
                            .note(format!("at {}:{}", l, c0)),
                    );
                }
            }
        }
        self.push(self.pos, self.pos, TokenKind::Eof, String::new());
    }

    // ----- low-level cursor -----

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    /// Char `n` positions ahead of `pos` (n=0 is `peek`).
    fn peek_at(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    /// Advance one char, updating byte pos, line, and column.
    fn bump_char(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
    }

    /// Advance `n` chars.
    fn bump_n(&mut self, n: usize) {
        for _ in 0..n {
            self.bump_char();
        }
    }

    /// Consume the rest of the current line for a `//` comment.
    fn line_comment(&mut self) {
        let start = self.pos;
        let (l, c) = (self.line, self.col);
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            self.bump_char();
        }
        self.emit(start, TokenKind::LineComment, l, c);
    }

    /// Consume a `/* ... */` comment; unterminated comments are diagnosed.
    fn block_comment(&mut self) {
        let start = self.pos;
        let (l, c) = (self.line, self.col);
        self.bump_n(2); // /*
        let mut closed = false;
        while let Some(ch) = self.peek() {
            if ch == '*' && self.peek_at(1) == Some('/') {
                self.bump_n(2);
                closed = true;
                break;
            }
            self.bump_char();
        }
        if !closed {
            self.diags.push(
                Diagnostic::error("LEX003", "unterminated block comment")
                    .with_span(Span::new(start as u32, self.pos as u32)),
            );
        }
        self.emit(start, TokenKind::BlockComment, l, c);
    }

    /// Consume a `"..."` string or `'...'` char literal, honoring `\` escapes.
    fn string_literal(&mut self, kind: TokenKind, what: &str) {
        let start = self.pos;
        let (l, c) = (self.line, self.col);
        let quote = self.peek().unwrap_or('"');
        self.bump_char(); // opening quote
        let mut closed = false;
        while let Some(ch) = self.peek() {
            if ch == '\\' {
                // `bump_char` is a no-op at EOF, so this is safe for a
                // trailing backslash.
                self.bump_n(2);
                continue;
            }
            if ch == quote {
                self.bump_char();
                closed = true;
                break;
            }
            if ch == '\n' {
                // Monkey C literals do not span lines; stop before the newline.
                break;
            }
            self.bump_char();
        }
        if !closed {
            self.diags.push(
                Diagnostic::error(
                    if kind == TokenKind::Str {
                        "LEX001"
                    } else {
                        "LEX002"
                    },
                    format!("unterminated {} literal", what),
                )
                .with_span(Span::new(start as u32, self.pos as u32)),
            );
        }
        self.emit(start, kind, l, c);
    }

    /// Consume an identifier/keyword.
    fn ident(&mut self) {
        let start = self.pos;
        let (l, c) = (self.line, self.col);
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.bump_char();
            } else {
                break;
            }
        }
        let text = &self.src[start..self.pos];
        let kind = if KEYWORDS.contains(&text) {
            TokenKind::Keyword
        } else {
            TokenKind::Ident
        };
        self.emit(start, kind, l, c);
    }

    /// Consume a numeric literal: hex, decimal, float, or exponent form.
    fn number(&mut self) {
        let start = self.pos;
        let (l, c) = (self.line, self.col);
        let first = self.peek().unwrap_or('0');
        if first == '0' && matches!(self.peek_at(1), Some('x') | Some('X')) {
            self.bump_n(2);
            while self.peek().is_some_and(|ch| ch.is_ascii_hexdigit()) {
                self.bump_char();
            }
        } else {
            while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                self.bump_char();
            }
            // Fraction: `.digits` (only when a digit follows the dot).
            if self.peek() == Some('.') && self.peek_at(1).is_some_and(|ch| ch.is_ascii_digit()) {
                self.bump_char();
                while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                    self.bump_char();
                }
            }
            // Exponent: `e[+-]digits`.
            if matches!(self.peek(), Some('e') | Some('E')) {
                let save = (self.pos, self.line, self.col);
                self.bump_char();
                if matches!(self.peek(), Some('+') | Some('-')) {
                    self.bump_char();
                }
                if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                    while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                        self.bump_char();
                    }
                } else {
                    // `1e` / `1e+` without digits — not an exponent; rewind.
                    self.pos = save.0;
                    self.line = save.1;
                    self.col = save.2;
                }
            }
        }
        // Numeric suffixes: float `1.0f`/`1.0d`, long `123L` / `0xFFL`.
        // (For hex, `f`/`F` were already consumed as digits; only `L`/`l`
        // can remain. For decimals all four are possible.)
        if matches!(
            self.peek(),
            Some('f') | Some('F') | Some('l') | Some('L') | Some('d') | Some('D')
        ) && !self
            .peek_at(1)
            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
        {
            self.bump_char();
        }
        self.emit(start, TokenKind::Number, l, c);
    }

    /// Consume an operator or punctuation token (longest match first).
    fn punct(&mut self) {
        let start = self.pos;
        let (l, c) = (self.line, self.col);
        let rest = &self.src[self.pos..];
        let mut len = 1usize;
        for op in MULTI_PUNCT {
            if rest.starts_with(op) {
                len = op.len();
                break;
            }
        }
        // Advance `len` bytes — all punct is ASCII.
        for _ in 0..len {
            self.bump_char();
        }
        self.emit(start, TokenKind::Punct, l, c);
    }

    // ----- emission -----

    /// Record the token spanning `[start, self.pos)` that began at
    /// (`line`, `col`).
    fn emit(&mut self, start: usize, kind: TokenKind, line: u32, col: u32) {
        let end = self.pos;
        let text = self.src[start..end].to_string();
        let newlines = self.newlines;
        let space = self.space;
        self.tokens.push(Token {
            kind,
            text,
            start,
            end,
            line,
            column: col,
            newlines_before: newlines,
            space_before: space,
        });
        self.newlines = 0;
        self.space = false;
    }

    /// Push a token with explicit positions (used for `Eof`).
    fn push(&mut self, start: usize, end: usize, kind: TokenKind, text: String) {
        let newlines = self.newlines;
        let space = self.space;
        self.tokens.push(Token {
            kind,
            text,
            start,
            end,
            line: self.line,
            column: self.col,
            newlines_before: newlines,
            space_before: space,
        });
        self.newlines = 0;
        self.space = false;
    }
}

/// Whether `text` is a keyword that introduces a parenthesised condition.
pub fn is_control_keyword(text: &str) -> bool {
    CONTROL_KEYWORDS.contains(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src).iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_basic_code() {
        let toks = lex("class Foo {\n  function bar() as Void {\n    var x = 1;\n  }\n}\n");
        assert_eq!(toks[0].kind, TokenKind::Keyword);
        assert_eq!(toks[0].text, "class");
        assert_eq!(toks[1].text, "Foo");
        assert_eq!(toks.last().unwrap().kind, TokenKind::Eof);
    }

    #[test]
    fn tracks_newlines_and_space() {
        let toks = lex("var a=1;\n\nvar b = 2;");
        let vars: Vec<_> = toks.iter().filter(|t| t.text == "var").collect();
        let b = toks.iter().find(|t| t.text == "b").unwrap();
        assert_eq!(vars[1].newlines_before, 2);
        assert!(!vars[1].space_before);
        assert!(b.space_before);
    }

    #[test]
    fn lexes_comments_and_strings() {
        let k = kinds("// hi\nvar s = \"a\\\"b\"; /* x\ny */ var c = 'a';");
        assert!(k.contains(&TokenKind::LineComment));
        assert!(k.contains(&TokenKind::BlockComment));
        assert!(k.contains(&TokenKind::Str));
        assert!(k.contains(&TokenKind::CharLit));
    }

    #[test]
    fn lexes_numbers_and_operators() {
        let toks = lex("var x = 0x1F + 60.0 * 2e3 >= 4;");
        let texts: Vec<&str> = toks.iter().map(|t| t.text.as_str()).collect();
        for want in ["0x1F", "60.0", "2e3", ">="] {
            assert!(texts.contains(&want), "missing {}", want);
        }
    }

    #[test]
    fn tracks_line_and_column() {
        let src = "var a = 1;\nvar bb = 2;";
        let toks = lex(src);
        let bb = toks.iter().find(|t| t.text == "bb").unwrap();
        assert_eq!((bb.line, bb.column), (2, 5));
        let semi = toks.iter().find(|t| t.text == ";").unwrap();
        assert_eq!((semi.line, semi.column), (1, 10));
    }

    #[test]
    fn handles_crlf() {
        let src = "var a = 1;\r\nvar b = 2;\r\n";
        let toks = lex(src);
        let vars: Vec<_> = toks.iter().filter(|t| t.text == "var").collect();
        assert_eq!((vars[1].line, vars[1].column), (2, 1));
        // \r\n counts as exactly one newline.
        assert_eq!(vars[1].newlines_before, 1);
        assert!(!vars[1].space_before);
    }

    #[test]
    fn utf8_source_is_safe() {
        // 'é' is two bytes; column counts characters, offsets stay on
        // char boundaries.
        let src = "var café = \"naïve — ✓\";\nvar x = 1;";
        let toks = lex(src);
        // Monkey C identifiers are ASCII: `café` lexes as `caf` + LEX004.
        let cafe = toks.iter().find(|t| t.text == "caf").unwrap();
        assert_eq!(cafe.column, 5);
        assert!(src.is_char_boundary(cafe.start));
        assert!(src.is_char_boundary(cafe.end));
        let r = lex_full(src);
        assert!(r.diagnostics.iter().any(|d| d.code == "LEX004"));
        let x = toks.iter().find(|t| t.text == "x").unwrap();
        assert_eq!(x.line, 2);
    }

    #[test]
    fn block_comment_updates_line_tracking() {
        let src = "/* a\nb\nc */ var x = 1;";
        let toks = lex(src);
        let x = toks.iter().find(|t| t.text == "x").unwrap();
        assert_eq!(x.line, 3);
    }

    #[test]
    fn lex_full_reports_unterminated_string() {
        let r = lex_full("var s = \"abc\nvar t = 1;");
        assert!(r.diagnostics.iter().any(|d| d.code == "LEX001"));
        // Still produces a best-effort token stream.
        assert!(r.tokens.iter().any(|t| t.text == "var"));
    }

    #[test]
    fn lex_full_reports_unterminated_comment() {
        // The unterminated comment consumes the rest of the file.
        let r = lex_full("/* never closed\nvar c = 'x");
        assert!(r.diagnostics.iter().any(|d| d.code == "LEX003"));
        let r = lex_full("var c = 'x");
        assert!(r.diagnostics.iter().any(|d| d.code == "LEX002"));
    }

    #[test]
    fn lex_full_reports_unexpected_char() {
        let r = lex_full("var x = 1 § 2;");
        assert!(r.diagnostics.iter().any(|d| d.code == "LEX004"));
    }

    #[test]
    fn lex_full_clean_source_has_no_diags() {
        let r = lex_full("using Toybox.System;\nclass A {\n  function f() as Void {\n    var s = \"hi\";\n  }\n}\n");
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.tokens.last().unwrap().kind, TokenKind::Eof);
    }

    #[test]
    fn number_edge_cases() {
        // `1e` without digits is not an exponent; `.5` is a number.
        let toks = lex("var a = 1e; var b = .5; var c = 1e-3;");
        let texts: Vec<&str> = toks.iter().map(|t| t.text.as_str()).collect();
        assert!(texts.contains(&"1"));
        assert!(texts.contains(&".5"));
        assert!(texts.contains(&"1e-3"));
        assert!(texts.contains(&"e")); // `1e` split as `1` + ident `e`
    }
}
