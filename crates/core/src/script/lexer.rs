//! Lexer for the creature script DSL.
//!
//! Port of creatureTools `internal/script/lexer.go` with one extension:
//! every token (and every lexer error) carries a 1-based line/column so
//! `ScriptError`s can report where the problem is.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String),
    Number(u16),
    Text(String),
    At(String),
    Bang(String),
    Semicolon,
    Eof,
}

/// A token together with its start position (1-based line and column).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, col: usize) -> Self {
        Self { kind, line, col }
    }

    pub fn eof(line: usize, col: usize) -> Self {
        Self {
            kind: TokenKind::Eof,
            line,
            col,
        }
    }
}

/// Stateful scanner over `src`. Call [`Lexer::next`] repeatedly; it returns
/// [`TokenKind::Eof`] once the input is exhausted.
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(src: &str) -> Self {
        Self {
            input: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    /// Advance one character, keeping line/column bookkeeping.
    fn advance(&mut self) {
        if self.pos < self.input.len() {
            if self.input[self.pos] == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    /// Current 1-based (line, column) — the position of the next unconsumed
    /// character (or end of input).
    pub fn position(&self) -> (usize, usize) {
        (self.line, self.col)
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Consume everything up to (but not including) the next newline.
    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn read_ident(&mut self) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                out.push(c);
                self.advance();
            } else {
                break;
            }
        }
        out
    }

    fn read_number(&mut self) -> Result<u16, String> {
        let mut value: u32 = 0;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                value = value * 10 + c.to_digit(10).unwrap();
                self.advance();
            } else {
                break;
            }
        }
        if value > u16::MAX as u32 {
            return Err(format!("number too large for uint16: {value}"));
        }
        Ok(value as u16)
    }

    fn read_text(&mut self) -> Result<String, String> {
        let mut out = String::new();
        loop {
            match self.peek() {
                Some('}') => {
                    self.advance();
                    return Ok(out);
                }
                Some(c) => {
                    out.push(c);
                    self.advance();
                }
                None => return Err("unterminated text string (missing '}')".to_string()),
            }
        }
    }

    /// Produce the next token. Lexical errors (unexpected character,
    /// overflowing number, unterminated text) return `Err` with the
    /// position of the offending input.
    pub fn next(&mut self) -> Result<Token, crate::script::ScriptError> {
        self.skip_whitespace();

        let Some(ch) = self.peek() else {
            return Ok(Token::eof(self.line, self.col));
        };

        let (line, col) = (self.line, self.col);

        match ch {
            '@' => {
                self.advance();
                Ok(Token::new(TokenKind::At(self.read_ident()), line, col))
            }
            '!' => {
                self.advance();
                Ok(Token::new(TokenKind::Bang(self.read_ident()), line, col))
            }
            '{' => {
                self.advance();
                match self.read_text() {
                    Ok(text) => Ok(Token::new(TokenKind::Text(text), line, col)),
                    Err(msg) => Err(crate::script::ScriptError { line, col, msg }),
                }
            }
            ';' => {
                self.advance();
                Ok(Token::new(TokenKind::Semicolon, line, col))
            }
            '#' => {
                self.skip_line();
                self.next()
            }
            c if c.is_ascii_digit() => match self.read_number() {
                Ok(n) => Ok(Token::new(TokenKind::Number(n), line, col)),
                Err(msg) => Err(crate::script::ScriptError { line, col, msg }),
            },
            c if c.is_alphabetic() || c == '_' => {
                Ok(Token::new(TokenKind::Ident(self.read_ident()), line, col))
            }
            c => {
                self.advance();
                Err(crate::script::ScriptError {
                    line,
                    col,
                    msg: format!("unexpected character {c}"),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(s: &str) -> TokenKind {
        TokenKind::Ident(s.to_string())
    }

    fn collect(src: &str) -> Vec<TokenKind> {
        let mut lex = Lexer::new(src);
        let mut out = Vec::new();
        loop {
            let tok = lex.next().expect("no lex error");
            let is_eof = tok.kind == TokenKind::Eof;
            out.push(tok.kind);
            if is_eof {
                break;
            }
        }
        out
    }

    /// Parity with Go `TestLexer` (internal/script/script_test.go).
    #[test]
    fn parity_lexer_cases() {
        let cases: Vec<(&str, Vec<TokenKind>)> = vec![
            (
                "msg {hello};",
                vec![
                    ident("msg"),
                    TokenKind::Text("hello".into()),
                    TokenKind::Semicolon,
                    TokenKind::Eof,
                ],
            ),
            (
                "tpif @loc1 @loc2;",
                vec![
                    ident("tpif"),
                    TokenKind::At("loc1".into()),
                    TokenKind::At("loc2".into()),
                    TokenKind::Semicolon,
                    TokenKind::Eof,
                ],
            ),
            (
                "if !flag_X then setflag flag_Y endif;",
                vec![
                    ident("if"),
                    TokenKind::Bang("flag_X".into()),
                    ident("then"),
                    ident("setflag"),
                    ident("flag_Y"),
                    ident("endif"),
                    TokenKind::Semicolon,
                    TokenKind::Eof,
                ],
            ),
            (
                "# comment\nmsg {a};",
                vec![
                    ident("msg"),
                    TokenKind::Text("a".into()),
                    TokenKind::Semicolon,
                    TokenKind::Eof,
                ],
            ),
        ];
        for (src, want) in cases {
            assert_eq!(collect(src), want, "input: {src:?}");
        }
    }

    /// Drain a lexer, returning the first error encountered.
    fn first_error(src: &str) -> crate::script::ScriptError {
        let mut lex = Lexer::new(src);
        loop {
            match lex.next() {
                Err(e) => return e,
                Ok(t) if t.kind == TokenKind::Eof => panic!("expected a lex error"),
                Ok(_) => continue,
            }
        }
    }

    #[test]
    fn lexer_error_reports_line_and_col() {
        // Error character '$' sits on line 2, column 3.
        let err = first_error("msg {a}\n  $");
        assert_eq!(err.line, 2);
        assert_eq!(err.col, 3);
        assert!(err.msg.contains('$'), "msg: {}", err.msg);
    }

    #[test]
    fn lexer_error_on_line_1() {
        let err = first_error("msg {a} &");
        assert_eq!(err.line, 1);
    }

    #[test]
    fn lexer_reports_line_across_tokens() {
        let mut lex = Lexer::new("msg\n{a}");
        let first = lex.next().unwrap();
        assert_eq!(first.kind, ident("msg"));
        assert_eq!(first.line, 1);
        let second = lex.next().unwrap();
        assert_eq!(second.kind, TokenKind::Text("a".into()));
        assert_eq!(second.line, 2);
    }

    #[test]
    fn unterminated_text_errors() {
        let err = first_error("msg {hello");
        assert!(err.msg.contains("unterminated"), "msg: {}", err.msg);
    }

    #[test]
    fn number_overflow_errors() {
        let err = Lexer::new("65536").next().expect_err("must fail");
        assert!(err.msg.contains("uint16"), "msg: {}", err.msg);
    }
}
