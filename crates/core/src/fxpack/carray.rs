use anyhow::{bail, Context};
use std::path::Path;

/// Integer widths supported by the C-array FX source format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CType {
    U8,
    U24,
    U32,
}

/// A source declaration in its original order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub namespace: Option<String>,
    pub ctype: CType,
    /// Decoded initializer bytes for `uint8_t` declarations.
    pub bytes: Option<Vec<u8>>,
    /// One-based source line containing the declaration type.
    pub line: u32,
}

/// Enumerate supported declarations in a C-style FX source file.
///
/// `uint8_t` initializers are decoded, while `uint24_t` and `uint32_t`
/// declarations retain their source metadata for typed-symbol packing.
pub fn enumerate_declarations(path: &Path) -> anyhow::Result<Vec<Declaration>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse_declarations(&content, &path.display().to_string())
}

/// Extract bytes from all `uint8_t` initializers in a C-style source file.
///
/// Numeric values must fit in one byte. String literals support the common C
/// escapes used by FX data, including `\0` and `\xHH`. `uint24_t`
/// declarations and symbolic expressions are intentionally not flattened.
pub fn extract_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
    Ok(enumerate_declarations(path)?
        .into_iter()
        .flat_map(|declaration| declaration.bytes)
        .flatten()
        .collect())
}

/// Extract bytes from one named `uint8_t` declaration in a C-style source file.
pub fn extract_symbol_bytes(path: &Path, symbol: &str) -> anyhow::Result<Vec<u8>> {
    let declarations = enumerate_declarations(path)
        .with_context(|| format!("extracting symbol {symbol:?} from {}", path.display()))?;
    declarations
        .into_iter()
        .filter(|declaration| declaration.name == symbol)
        .take(1)
        .flat_map(|declaration| declaration.bytes)
        .next()
        .ok_or_else(|| anyhow::anyhow!("symbol {symbol:?} not found"))
}

/// Parse all `uint8_t` brace or string initializers, preserving declaration order.
pub fn parse_carray_bytes(content: &str) -> anyhow::Result<Vec<u8>> {
    Ok(parse_declarations(content, "<input>")?
        .into_iter()
        .flat_map(|declaration| declaration.bytes)
        .flatten()
        .collect())
}

/// Parse the initializer for one named `uint8_t` declaration.
pub fn parse_symbol_bytes(content: &str, symbol: &str) -> anyhow::Result<Vec<u8>> {
    parse_declarations(content, "<input>")?
        .into_iter()
        .filter(|declaration| declaration.name == symbol)
        .take(1)
        .flat_map(|declaration| declaration.bytes)
        .next()
        .ok_or_else(|| anyhow::anyhow!("symbol {symbol:?} not found"))
}

fn parse_declarations(content: &str, source: &str) -> anyhow::Result<Vec<Declaration>> {
    let mut parser = Parser {
        input: content.as_bytes(),
        position: 0,
        source,
        declarations: Vec::new(),
        namespaces: Vec::new(),
        unstructured_braces: 0,
    };
    parser.parse()?;
    Ok(parser.declarations)
}

struct Namespace {
    name: String,
    line: u32,
}

struct Parser<'input, 'source> {
    input: &'input [u8],
    position: usize,
    source: &'source str,
    declarations: Vec<Declaration>,
    namespaces: Vec<Namespace>,
    unstructured_braces: u32,
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
                b'{' => {
                    self.unstructured_braces += 1;
                    self.position += 1;
                }
                b'}' if self.unstructured_braces > 0 => {
                    self.unstructured_braces -= 1;
                    self.position += 1;
                }
                b'}' => self.end_namespace()?,
                byte if is_identifier_start(byte) => {
                    let line = self.line();
                    let identifier = self.read_identifier();
                    match identifier {
                        b"namespace" => self.parse_namespace(line)?,
                        b"uint8_t" => self.parse_declaration(CType::U8, line)?,
                        b"uint24_t" => self.parse_declaration(CType::U24, line)?,
                        b"uint32_t" => self.parse_declaration(CType::U32, line)?,
                        identifier if looks_like_uint_type(identifier) => {
                            let identifier = String::from_utf8_lossy(identifier).into_owned();
                            bail!(
                                "{}:{}: unsupported declaration type {identifier:?}",
                                self.source,
                                line,
                            );
                        }
                        b"namespace_end" => bail!(
                            "{}:{}: namespace_end must follow a closing brace",
                            self.source,
                            line
                        ),
                        _ => {}
                    }
                }
                _ => self.position += 1,
            }
        }
        if let Some(namespace) = self.namespaces.last() {
            bail!(
                "{}:{}: namespace {:?} is not closed",
                self.source,
                namespace.line,
                namespace.name
            );
        }
        Ok(())
    }

    fn parse_namespace(&mut self, line: u32) -> anyhow::Result<()> {
        self.skip_whitespace_and_comments()?;
        let name = match self.current_opt() {
            Some(byte) if is_identifier_start(byte) => {
                String::from_utf8(self.read_identifier().to_vec()).expect("ASCII identifier")
            }
            Some(_) => bail!(
                "{}:{}: namespace must name an identifier",
                self.source,
                line
            ),
            None => bail!("{}:{}: unterminated namespace", self.source, line),
        };
        self.skip_whitespace_and_comments()?;
        if self.current_opt() != Some(b'{') {
            bail!(
                "{}:{}: namespace {:?} must open with '{{'",
                self.source,
                line,
                name
            );
        }
        self.position += 1;
        self.namespaces.push(Namespace { name, line });
        Ok(())
    }

    fn end_namespace(&mut self) -> anyhow::Result<()> {
        let line = self.line();
        self.position += 1;
        self.skip_whitespace_and_comments()?;
        if self.input[self.position..].starts_with(b"namespace_end") {
            let end = self.position + b"namespace_end".len();
            if self
                .input
                .get(end)
                .is_none_or(|byte| !is_identifier_continue(*byte))
            {
                self.position = end;
            }
        }
        let Some(_) = self.namespaces.pop() else {
            bail!(
                "{}:{}: closing brace has no open namespace",
                self.source,
                line
            );
        };
        Ok(())
    }

    fn parse_declaration(&mut self, ctype: CType, line: u32) -> anyhow::Result<()> {
        self.skip_whitespace_and_comments()?;
        let name = match self.current_opt() {
            Some(byte) if is_identifier_start(byte) => {
                String::from_utf8(self.read_identifier().to_vec()).expect("ASCII identifier")
            }
            Some(_) => bail!(
                "{}:{}: declaration must name an identifier",
                self.source,
                line
            ),
            None => bail!("{}:{}: unterminated declaration", self.source, line),
        };

        loop {
            self.skip_whitespace_and_comments()?;
            match self.current_opt() {
                Some(b'=') => {
                    self.position += 1;
                    self.skip_whitespace_and_comments()?;
                    let bytes = match ctype {
                        CType::U8 => Some(match self.current_opt() {
                            Some(b'{') => self.parse_braced_initializer()?,
                            Some(b'"') => self.parse_string_initializer()?,
                            Some(b'0'..=b'9') => {
                                let mut bytes = Vec::new();
                                self.parse_number(&mut bytes)?;
                                bytes
                            }
                            Some(_) => bail!(
                                "{}:{}: uint8_t initializer must start with '{{' or a string literal",
                                self.source,
                                line
                            ),
                            None => bail!("{}:{}: unterminated declaration", self.source, line),
                        }),
                        CType::U24 | CType::U32 => {
                            self.skip_typed_initializer(line)?;
                            None
                        }
                    };
                    self.declarations.push(Declaration {
                        name,
                        namespace: self.namespace(),
                        ctype,
                        bytes,
                        line,
                    });
                    return Ok(());
                }
                Some(b';') => {
                    self.position += 1;
                    return Ok(());
                }
                Some(b'"') => self.skip_string()?,
                Some(_) => self.position += 1,
                None => bail!("{}:{}: unterminated declaration", self.source, line),
            }
        }
    }

    fn skip_typed_initializer(&mut self, line: u32) -> anyhow::Result<()> {
        let mut braces = 0_u32;
        loop {
            self.skip_whitespace_and_comments()?;
            match self.current_opt() {
                Some(b'{') => {
                    braces += 1;
                    self.position += 1;
                }
                Some(b'}') if braces > 0 => {
                    braces -= 1;
                    self.position += 1;
                    if braces == 0 {
                        return Ok(());
                    }
                }
                Some(b';') if braces == 0 => {
                    self.position += 1;
                    return Ok(());
                }
                Some(b'"') => self.skip_string()?,
                Some(_) => self.position += 1,
                None => bail!("{}:{}: unterminated declaration", self.source, line),
            }
        }
    }

    fn parse_braced_initializer(&mut self) -> anyhow::Result<Vec<u8>> {
        let mut bytes = Vec::new();
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
                        return Ok(bytes);
                    }
                }
                b'"' => self.parse_string(&mut bytes)?,
                b'0'..=b'9' => self.parse_number(&mut bytes)?,
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

    fn parse_string_initializer(&mut self) -> anyhow::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.parse_string(&mut bytes)?;
        loop {
            self.skip_whitespace_and_comments()?;
            match self.current_opt() {
                Some(b'"') => self.parse_string(&mut bytes)?,
                Some(b';') | None => return Ok(bytes),
                Some(byte) => bail!("invalid byte {:?} after string initializer", byte as char),
            }
        }
    }

    fn parse_string(&mut self, bytes: &mut Vec<u8>) -> anyhow::Result<()> {
        debug_assert_eq!(self.current(), b'"');
        self.position += 1;
        loop {
            let Some(byte) = self.current_opt() else {
                bail!("unterminated string literal");
            };
            self.position += 1;
            match byte {
                b'"' => return Ok(()),
                b'\\' => self.parse_escape(bytes)?,
                _ => bytes.push(byte),
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

    fn parse_escape(&mut self, bytes: &mut Vec<u8>) -> anyhow::Result<()> {
        let Some(escape) = self.current_opt() else {
            bail!("unterminated string escape");
        };
        self.position += 1;
        match escape {
            b'0' => bytes.push(0),
            b'n' => bytes.push(b'\n'),
            b'r' => bytes.push(b'\r'),
            b't' => bytes.push(b'\t'),
            b'\\' => bytes.push(b'\\'),
            b'"' => bytes.push(b'"'),
            b'x' => {
                let high = self.next_hex_digit()?;
                let low = self.next_hex_digit()?;
                bytes.push((high << 4) | low);
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

    fn parse_number(&mut self, bytes: &mut Vec<u8>) -> anyhow::Result<()> {
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
        bytes.push(value);
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

    fn namespace(&self) -> Option<String> {
        (!self.namespaces.is_empty()).then(|| {
            self.namespaces
                .iter()
                .map(|namespace| namespace.name.as_str())
                .collect::<Vec<_>>()
                .join("::")
        })
    }

    fn line(&self) -> u32 {
        self.input[..self.position]
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count() as u32
            + 1
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

fn looks_like_uint_type(identifier: &[u8]) -> bool {
    identifier.starts_with(b"uint") && identifier.ends_with(b"_t")
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
