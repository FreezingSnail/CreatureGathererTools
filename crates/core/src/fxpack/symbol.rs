//! Native encoder for typed FX declarations with symbolic offsets.
//!
//! `fxdata-build.py` encoded every initializer item at its declaration width:
//! `uint8_t`, `uint24_t`, or `uint32_t`. This small lexer intentionally accepts
//! that data subset and resolves identifiers against fields already packed.

use anyhow::{Context, bail};
use std::collections::BTreeMap;
use std::path::Path;

pub fn extract_symbol_bytes(
    path: &Path,
    symbol: &str,
    namespace: Option<&str>,
    offsets: &BTreeMap<String, u32>,
) -> anyhow::Result<Vec<u8>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_symbol_bytes(&content, symbol, namespace, offsets)
        .with_context(|| format!("extracting typed symbol {symbol:?} from {}", path.display()))
}

pub fn parse_symbol_bytes(
    content: &str,
    symbol: &str,
    namespace: Option<&str>,
    offsets: &BTreeMap<String, u32>,
) -> anyhow::Result<Vec<u8>> {
    let tokens = lex(content)?;
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].as_ident() == Some(symbol)
            && matches!(tokens.get(index + 1), Some(Token::Punct('=')))
        {
            // Historical FX inputs permit an untyped table assignment. The
            // legacy builder treats its symbolic values as uint24_t offsets.
            return encode_initializer(&tokens[index + 2..], 3, namespace, offsets);
        }
        let Some(width) = declaration_width(tokens[index].as_ident()) else {
            index += 1;
            continue;
        };
        index += 1;
        while matches!(
            tokens.get(index),
            Some(Token::Punct('[') | Token::Punct(']'))
        ) {
            index += 1;
        }
        let Some(name) = tokens.get(index).and_then(Token::as_ident) else {
            continue;
        };
        index += 1;
        if name != symbol {
            continue;
        }
        while !matches!(tokens.get(index), Some(Token::Punct('=')) | None) {
            index += 1;
        }
        if !matches!(tokens.get(index), Some(Token::Punct('='))) {
            bail!("symbol {symbol:?} has no initializer");
        }
        return encode_initializer(&tokens[index + 1..], width, namespace, offsets);
    }
    bail!("symbol {symbol:?} not found")
}

fn declaration_width(token: Option<&str>) -> Option<usize> {
    match token? {
        "uint8_t" => Some(1),
        "uint24_t" => Some(3),
        "uint32_t" => Some(4),
        _ => None,
    }
}

fn encode_initializer(
    tokens: &[Token],
    width: usize,
    namespace: Option<&str>,
    offsets: &BTreeMap<String, u32>,
) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut depth = 0_u32;
    let mut braced = false;
    for token in tokens {
        match token {
            Token::Punct('{') => {
                braced = true;
                depth += 1;
            }
            Token::Punct('}') => {
                depth = depth
                    .checked_sub(1)
                    .context("unbalanced typed initializer")?;
                if depth == 0 {
                    return Ok(bytes);
                }
            }
            Token::Punct(';') if !braced => return Ok(bytes),
            Token::Newline if !braced => return Ok(bytes),
            Token::Punct(',') | Token::Newline | Token::Punct('[') | Token::Punct(']') => {}
            Token::Number(value) => push_value(&mut bytes, *value, width)?,
            Token::Ident(name) => {
                let value = resolve(name, namespace, offsets)?;
                push_value(&mut bytes, value, width)?;
            }
            Token::Punct(other) => bail!("unexpected {other:?} in typed initializer"),
        }
    }
    if braced {
        bail!("unterminated typed initializer");
    }
    Ok(bytes)
}

fn push_value(bytes: &mut Vec<u8>, value: u32, width: usize) -> anyhow::Result<()> {
    let max = match width {
        1 => u8::MAX as u32,
        3 => 0x00ff_ffff,
        4 => u32::MAX,
        _ => unreachable!("typed declaration widths are fixed"),
    };
    if value > max {
        bail!("value {value} does not fit in uint{}", width * 8);
    }
    let encoded = value.to_be_bytes();
    bytes.extend_from_slice(&encoded[4 - width..]);
    Ok(())
}

fn resolve(
    name: &str,
    namespace: Option<&str>,
    offsets: &BTreeMap<String, u32>,
) -> anyhow::Result<u32> {
    if name.contains("::") {
        return offsets
            .get(name)
            .copied()
            .with_context(|| format!("unresolved symbolic offset {name:?}"));
    }
    if let Some(namespace) = namespace {
        let qualified = format!("{namespace}::{name}");
        if let Some(offset) = offsets.get(&qualified) {
            return Ok(*offset);
        }
    }
    if let Some(offset) = offsets.get(name) {
        return Ok(*offset);
    }
    let matches: Vec<(&String, &u32)> = offsets
        .iter()
        .filter(|(candidate, _)| candidate.rsplit("::").next() == Some(name))
        .collect();
    match matches.as_slice() {
        [(_, offset)] => Ok(**offset),
        [] => bail!("unresolved symbolic offset {name:?}"),
        _ => bail!("ambiguous symbolic offset {name:?}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    Number(u32),
    Punct(char),
    Newline,
}

impl Token {
    fn as_ident(&self) -> Option<&str> {
        match self {
            Self::Ident(value) => Some(value),
            _ => None,
        }
    }
}

fn lex(input: &str) -> anyhow::Result<Vec<Token>> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\r' => index += 1,
            b'\n' => {
                tokens.push(Token::Newline);
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while bytes.get(index..index + 2) != Some(b"*/") {
                    if index == bytes.len() {
                        bail!("unterminated block comment");
                    }
                    index += 1;
                }
                index += 2;
            }
            b'"' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    if bytes[index] == b'\\' {
                        index += 1;
                    }
                    index += 1;
                }
                if index == bytes.len() {
                    bail!("unterminated string literal");
                }
                index += 1;
            }
            byte if is_ident_start(byte) => {
                let start = index;
                index += 1;
                while index < bytes.len() && is_ident_continue(bytes[index]) {
                    index += 1;
                }
                let mut name =
                    String::from_utf8(bytes[start..index].to_vec()).expect("ASCII identifier");
                while bytes.get(index..index + 2) == Some(b"::") {
                    index += 2;
                    let next = index;
                    if !bytes.get(index).is_some_and(|byte| is_ident_start(*byte)) {
                        bail!("invalid namespace-qualified identifier");
                    }
                    index += 1;
                    while index < bytes.len() && is_ident_continue(bytes[index]) {
                        index += 1;
                    }
                    name.push_str("::");
                    name.push_str(
                        std::str::from_utf8(&bytes[next..index]).expect("ASCII identifier"),
                    );
                }
                tokens.push(Token::Ident(name));
            }
            b'0'..=b'9' => {
                let start = index;
                let binary = bytes
                    .get(index..index + 2)
                    .is_some_and(|prefix| prefix == b"0b" || prefix == b"0B");
                let hexadecimal = bytes
                    .get(index..index + 2)
                    .is_some_and(|prefix| prefix == b"0x" || prefix == b"0X");
                index += if binary || hexadecimal { 2 } else { 0 };
                let digits = index;
                while index < bytes.len()
                    && if binary {
                        matches!(bytes[index], b'0' | b'1')
                    } else if hexadecimal {
                        bytes[index].is_ascii_hexdigit()
                    } else {
                        bytes[index].is_ascii_digit()
                    }
                {
                    index += 1;
                }
                if index == digits {
                    bail!("invalid numeric literal");
                }
                let literal =
                    std::str::from_utf8(&bytes[start..index]).expect("ASCII numeric literal");
                let value = if binary {
                    u32::from_str_radix(&literal[2..], 2)
                } else if hexadecimal {
                    u32::from_str_radix(&literal[2..], 16)
                } else {
                    literal.parse()
                }
                .with_context(|| format!("parsing numeric literal {literal:?}"))?;
                tokens.push(Token::Number(value));
            }
            byte @ (b'{' | b'}' | b'[' | b']' | b'=' | b',' | b';') => {
                tokens.push(Token::Punct(byte as char));
                index += 1;
            }
            other => bail!("unsupported character {:?}", other as char),
        }
    }
    Ok(tokens)
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}
