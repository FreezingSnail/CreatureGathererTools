//! Very small hand-written lexer for our script language.
//!
//! At this stage we *only* break the raw source string into `Token`s.
//! No keywords are recognised yet – `msg`, `tp`, etc. all come out as
//! `Ident("msg")`, `Ident("tp")`, …   The parser will interpret them
//! later.
//
//  Grammar excerpts (informal):
//
//      script  ::= stmt* EOF
//      stmt    ::= IDENT … NEWLINE
//
//  Lexical items:
//
//      Ident    ::= [A-Za-z_][A-Za-z0-9_]*
//      Number   ::= [0-9]+        (fits in u16)
//      Text     ::= '{' .*? '}'   (no nesting; '}' inside text forbidden)
//      Symbols  ::= '@' | '!'     (single-byte tokens)
//      Whitespace and comments (# until end-of-line) are discarded.

use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Ident(String),
    Number(u16),
    Text(String), // everything between { … }
    At(String),   // '@'
    Bang(String), // '!'
    Semicolon,    // ';'
    Eof,
}
#[derive(Clone)]
pub struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
    finished: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            chars: src.chars().peekable(),
            finished: false,
        }
    }

    fn next_char(&mut self) -> Option<char> {
        self.chars.next()
    }

    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn consume_while<F: Fn(char) -> bool>(&mut self, pred: F, buf: &mut String) {
        while let Some(c) = self.peek_char() {
            if pred(c) {
                buf.push(c);
                self.next_char();
            } else {
                break;
            }
        }
    }

    fn read_identifier(&mut self, first: char) -> String {
        let mut id = String::new();
        id.push(first);
        self.consume_while(|c| c.is_ascii_alphanumeric() || c == '_', &mut id);
        id
    }

    fn read_number(&mut self, first: char) -> Result<u16, String> {
        let mut num = String::new();
        num.push(first);
        self.consume_while(|c| c.is_ascii_digit(), &mut num);
        let value: u32 = num.parse().unwrap(); // only digits
        if value > u16::MAX as u32 {
            return Err(format!("vaule too large for uint16: {value}"));
        }
        Ok(value as u16)
    }

    fn read_text(&mut self) -> Result<String, String> {
        let mut txt = String::new();
        while let Some(c) = self.next_char() {
            if c == '}' {
                return Ok(txt);
            }
            txt.push(c);
        }
        Err("no closing } found")?
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Token, String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        // Skip whitespace that isn't newline
        while let Some(&c) = self.chars.peek() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.next_char();
            } else {
                break;
            }
        }

        let ch = match self.next_char() {
            Some(c) => c,
            None => return Some(Err("Missing end of script ;".into())),
        };

        let tok_res = match ch {
            '@' => {
                let next_char = self.next_char().unwrap_or('\0');
                let text = self.read_identifier(next_char);
                Ok(Token::At(text))
            }
            '!' => {
                let next_char = self.next_char().unwrap_or('\0');
                let text = self.read_identifier(next_char);
                Ok(Token::Bang(text))
            }
            '{' => self.read_text().map(Token::Text),

            ';' => {
                self.finished = true;
                Ok(Token::Eof)
            }
            c if c.is_ascii_digit() => self.read_number(c).map(Token::Number),
            c if c.is_ascii_alphabetic() || c == '_' => Ok(Token::Ident(self.read_identifier(c))),
            e => Err(format!("Unexpected character {e}")),
        };

        Some(tok_res)
    }
}

mod tests {
    #[cfg(test)]
    use super::{Lexer, Token};

    #[test]
    fn test_tokenisation() {
        let test_cases = vec![
            (
                "msg 42 40 {Hello world};",
                vec![
                    Token::Ident("msg".into()),
                    Token::Number(42),
                    Token::Number(40),
                    Token::Text("Hello world".into()),
                    Token::Eof,
                ],
            ),
            (
                "msg @loc1 {Hello world};",
                vec![
                    Token::Ident("msg".into()),
                    Token::At("loc1".into()),
                    Token::Text("Hello world".into()),
                    Token::Eof,
                ],
            ),
        ];

        for (src, expected) in test_cases {
            let tokens: Result<Vec<_>, _> = Lexer::new(src).collect();
            let tokens = tokens.unwrap();
            assert_eq!(tokens, expected);
        }
    }

    #[test]
    fn test_nest_if_tokens() {
        let test_cases = vec![(
            "if flag_X then if flag_Y then setflag flag_Z endif endif;",
            vec![
                Token::Ident("if".into()),
                Token::Ident("flag_X".into()),
                Token::Ident("then".into()),
                Token::Ident("if".into()),
                Token::Ident("flag_Y".into()),
                Token::Ident("then".into()),
                Token::Ident("setflag".into()),
                Token::Ident("flag_Z".into()),
                Token::Ident("endif".into()),
                Token::Ident("endif".into()),
                Token::Eof,
            ],
        )];

        for (src, expected) in test_cases {
            let tokens: Result<Vec<_>, _> = Lexer::new(src).collect();
            let tokens = tokens.unwrap();
            assert_eq!(tokens, expected);
        }
    }
}
