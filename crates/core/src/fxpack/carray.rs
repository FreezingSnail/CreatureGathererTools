use anyhow::{Context, bail};
use std::path::Path;

/// Extract bytes from all `uint8_t` initializers in a C-style source file.
///
/// Numeric values must fit in one byte. String literals support the common C
/// escapes used by FX data, including `\0` and `\xHH`. `uint24_t`
/// declarations and symbolic expressions are intentionally not supported.
pub fn extract_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_carray_bytes(&content)
        .with_context(|| format!("parsing C arrays from {}", path.display()))
}

/// Extract bytes from one named `uint8_t` declaration in a C-style source file.
pub fn extract_symbol_bytes(path: &Path, symbol: &str) -> anyhow::Result<Vec<u8>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_symbol_bytes(&content, symbol)
        .with_context(|| format!("extracting symbol {symbol:?} from {}", path.display()))
}

/// Parse all `uint8_t` brace or string initializers, preserving declaration order.
pub fn parse_carray_bytes(content: &str) -> anyhow::Result<Vec<u8>> {
    let mut parser = Parser {
        input: content.as_bytes(),
        position: 0,
        bytes: Vec::new(),
        symbol: None,
        found_symbol: false,
    };
    parser.parse()?;
    Ok(parser.bytes)
}

/// Parse the initializer for one named `uint8_t` declaration.
pub fn parse_symbol_bytes(content: &str, symbol: &str) -> anyhow::Result<Vec<u8>> {
    let mut parser = Parser {
        input: content.as_bytes(),
        position: 0,
        bytes: Vec::new(),
        symbol: Some(symbol.as_bytes()),
        found_symbol: false,
    };
    parser.parse()?;
    if !parser.found_symbol {
        bail!("symbol {symbol:?} not found");
    }
    Ok(parser.bytes)
}

struct Parser<'input, 'symbol> {
    input: &'input [u8],
    position: usize,
    bytes: Vec<u8>,
    symbol: Option<&'symbol [u8]>,
    found_symbol: bool,
}

impl Parser<'_, '_> {
    fn parse(&mut self) -> anyhow::Result<()> {
        while self.position < self.input.len() {
            self.skip_whitespace_and_comments()?;
            if self.position == self.input.len() {
                break;
            }
            match self.current() {
                b'"' => self.skip_string()?,
                byte if is_identifier_start(byte) => {
                    let identifier = self.read_identifier();
                    if identifier == b"uint8_t" {
                        self.parse_declaration()?;
                    }
                }
                _ => self.position += 1,
            }
        }
        Ok(())
    }

    fn parse_declaration(&mut self) -> anyhow::Result<()> {
        self.skip_whitespace_and_comments()?;
        let symbol = self.symbol;
        let declaration_name = match self.current_opt() {
            Some(byte) if is_identifier_start(byte) => self.read_identifier(),
            Some(_) => bail!("uint8_t declaration must name an identifier"),
            None => bail!("unterminated uint8_t declaration"),
        };
        if symbol.is_some_and(|symbol| symbol != declaration_name) {
            return Ok(());
        }

        loop {
            self.skip_whitespace_and_comments()?;
            if self.position == self.input.len() || self.current() == b';' {
                return Ok(());
            }
            if self.current() == b'=' {
                self.position += 1;
                self.skip_whitespace_and_comments()?;
                self.found_symbol = true;
                return match self.current_opt() {
                    Some(b'{') => self.parse_braced_initializer(),
                    Some(b'"') => self.parse_string_initializer(),
                    Some(_) => {
                        bail!("uint8_t initializer must start with '{{' or a string literal")
                    }
                    None => bail!("unterminated uint8_t declaration"),
                };
            }
            if self.current() == b'"' {
                self.skip_string()?;
            } else {
                self.position += 1;
            }
        }
    }

    fn parse_braced_initializer(&mut self) -> anyhow::Result<()> {
        let mut depth = 0_u32;
        loop {
            self.skip_whitespace_and_comments()?;
            let Some(byte) = self.current_opt() else {
                bail!("unterminated uint8_t initializer");
            };
            match byte {
                b'{' => {
                    depth += 1;
                    self.position += 1;
                }
                b'}' => {
                    depth -= 1;
                    self.position += 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                b'"' => self.parse_string()?,
                b'0'..=b'9' => self.parse_number()?,
                b',' => self.position += 1,
                byte if is_identifier_start(byte) => {
                    let identifier = self.read_identifier();
                    bail!(
                        "unsupported symbolic value {:?} in uint8_t initializer",
                        String::from_utf8_lossy(identifier)
                    );
                }
                _ => bail!("invalid byte {:?} in uint8_t initializer", byte as char),
            }
        }
    }

    fn parse_string_initializer(&mut self) -> anyhow::Result<()> {
        self.parse_string()?;
        loop {
            self.skip_whitespace_and_comments()?;
            match self.current_opt() {
                Some(b'"') => self.parse_string()?,
                Some(b';') | None => return Ok(()),
                Some(byte) => bail!("invalid byte {:?} after string initializer", byte as char),
            }
        }
    }

    fn parse_string(&mut self) -> anyhow::Result<()> {
        debug_assert_eq!(self.current(), b'"');
        self.position += 1;
        loop {
            let Some(byte) = self.current_opt() else {
                bail!("unterminated string literal");
            };
            self.position += 1;
            match byte {
                b'"' => return Ok(()),
                b'\\' => self.parse_escape()?,
                _ => self.bytes.push(byte),
            }
        }
    }

    fn skip_string(&mut self) -> anyhow::Result<()> {
        debug_assert_eq!(self.current(), b'"');
        self.position += 1;
        loop {
            let Some(byte) = self.current_opt() else {
                bail!("unterminated string literal");
            };
            self.position += 1;
            match byte {
                b'"' => return Ok(()),
                b'\\' if self.current_opt().is_some() => self.position += 1,
                b'\\' => bail!("unterminated string escape"),
                _ => {}
            }
        }
    }

    fn parse_escape(&mut self) -> anyhow::Result<()> {
        let Some(escape) = self.current_opt() else {
            bail!("unterminated string escape");
        };
        self.position += 1;
        match escape {
            b'0' => self.bytes.push(0),
            b'n' => self.bytes.push(b'\n'),
            b'r' => self.bytes.push(b'\r'),
            b't' => self.bytes.push(b'\t'),
            b'\\' => self.bytes.push(b'\\'),
            b'"' => self.bytes.push(b'"'),
            b'x' => {
                let high = self.next_hex_digit()?;
                let low = self.next_hex_digit()?;
                self.bytes.push((high << 4) | low);
            }
            _ => bail!("unknown string escape \\{}", escape as char),
        }
        Ok(())
    }

    fn next_hex_digit(&mut self) -> anyhow::Result<u8> {
        let Some(byte) = self.current_opt() else {
            bail!("invalid \\x escape");
        };
        let Some(value) = hex_value(byte) else {
            bail!("invalid \\x escape");
        };
        self.position += 1;
        Ok(value)
    }

    fn parse_number(&mut self) -> anyhow::Result<()> {
        let start = self.position;
        let hexadecimal = self.input[self.position..].starts_with(b"0x")
            || self.input[self.position..].starts_with(b"0X");
        if hexadecimal {
            self.position += 2;
            let digits_start = self.position;
            while self
                .current_opt()
                .is_some_and(|byte| byte.is_ascii_hexdigit())
            {
                self.position += 1;
            }
            if self.position == digits_start {
                bail!("invalid hexadecimal value");
            }
        } else {
            while self.current_opt().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        let literal =
            std::str::from_utf8(&self.input[start..self.position]).expect("ASCII literal");
        let value = if hexadecimal {
            u16::from_str_radix(&literal[2..], 16)
        } else {
            literal.parse()
        }
        .with_context(|| format!("parsing byte value {literal:?}"))?;
        let value = u8::try_from(value)
            .map_err(|_| anyhow::anyhow!("value {literal} is out of byte range"))?;
        self.bytes.push(value);
        Ok(())
    }

    fn skip_whitespace_and_comments(&mut self) -> anyhow::Result<()> {
        loop {
            while self
                .current_opt()
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                self.position += 1;
            }
            if !self.input[self.position..].starts_with(b"//") {
                if !self.input[self.position..].starts_with(b"/*") {
                    return Ok(());
                }
                self.position += 2;
                while !self.input[self.position..].starts_with(b"*/") {
                    if self.position == self.input.len() {
                        bail!("unterminated block comment");
                    }
                    self.position += 1;
                }
                self.position += 2;
                continue;
            }
            self.position += 2;
            while self.current_opt().is_some_and(|byte| byte != b'\n') {
                self.position += 1;
            }
        }
    }

    fn current(&self) -> u8 {
        self.input[self.position]
    }

    fn current_opt(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    fn read_identifier(&mut self) -> &[u8] {
        let start = self.position;
        self.position += 1;
        while self.current_opt().is_some_and(is_identifier_continue) {
            self.position += 1;
        }
        &self.input[start..self.position]
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
