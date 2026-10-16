//! Unified script DSL module: lexer + parser + controller.
//!
//! This is the Rust port of creatureTools `internal/script` (lexer.go,
//! ast.go, script.go), unified with the creaturevm-rust VM pipeline.
//!
//! AST + bytecode encoding are **not** reimplemented here: `crate::processor::ast`
//! already defines the canonical `Cmd`/`Text`/`Location`/`Condition`/`Branch`
//! tree and the `ToBytecode` encoder, and `crate::writer` consumes exactly
//! that representation. This module re-exports those types so `script::parse`
//! returns values the existing writer/assembler can serialize unchanged —
//! `writer` stays the byte-layout source of truth.
//!
//! Semantics (argument shapes, then/else/endif handling, controller
//! deduplication) mirror the Go implementation case-for-case; the Go
//! `Command2Bytes` / `script_test.go` expectations are asserted as parity
//! tests in `mod tests`.

pub mod lexer;

use std::collections::HashMap;
use std::fmt;

pub use crate::processor::ast::{
    validate_give_item, Branch, Cmd, Condition, Location, Opcode, Text, ToBytecode,
};

pub use lexer::{Lexer, Token, TokenKind};

/// Compile error carrying the 1-based line (and column) of the offending
/// token or character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, col {}: {}", self.line, self.col, self.msg)
    }
}

impl std::error::Error for ScriptError {}

fn err_at(tok: &Token, msg: impl Into<String>) -> ScriptError {
    ScriptError {
        line: tok.line,
        col: tok.col,
        msg: msg.into(),
    }
}

fn err(line: usize, col: usize, msg: impl Into<String>) -> ScriptError {
    ScriptError {
        line,
        col,
        msg: msg.into(),
    }
}

/// Symbol table used during compilation. Mirrors Go `Controller`
/// (script.go): each name space (tags, flags, texts) assigns fresh,
/// sequentially increasing `u16` ids, deduplicating by string content.
#[derive(Debug, Clone, Default)]
pub struct Controller {
    pub tags: HashMap<String, u16>,
    pub flags: HashMap<String, u16>,
    pub texts: HashMap<String, u16>,
    tag_count: u16,
    flag_count: u16,
    text_count: u16,
}

impl Controller {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a location-tag. Returns its existing id or assigns a new one.
    pub fn insert_tag(&mut self, name: &str) -> u16 {
        if let Some(&id) = self.tags.get(name) {
            return id;
        }
        let id = self.tag_count;
        self.tags.insert(name.to_string(), id);
        self.tag_count += 1;
        id
    }

    /// Register a flag name. Returns its existing id or assigns a new one.
    pub fn insert_flag(&mut self, name: &str) -> u16 {
        if let Some(&id) = self.flags.get(name) {
            return id;
        }
        let id = self.flag_count;
        self.flags.insert(name.to_string(), id);
        self.flag_count += 1;
        id
    }

    /// Register a text string. Returns its existing id or assigns a new one.
    pub fn insert_text(&mut self, text: &str) -> u16 {
        if let Some(&id) = self.texts.get(text) {
            return id;
        }
        let id = self.text_count;
        self.texts.insert(text.to_string(), id);
        self.text_count += 1;
        id
    }
}

/// Parse a complete script into a list of commands.
///
/// `locations` maps location-tag names to tile coordinates; `@name`
/// references are resolved to `Location::Cords` at compile time (the tag is
/// still registered with the controller for header generation).
pub fn parse(
    src: &str,
    locations: &HashMap<String, (u16, u16)>,
    controller: &mut Controller,
) -> Result<Vec<Cmd>, ScriptError> {
    let mut lex = Lexer::new(src);
    let mut cmds = Vec::new();

    loop {
        let tok = lex.next()?;
        match &tok.kind {
            TokenKind::Eof => break,
            TokenKind::Semicolon => continue,
            TokenKind::Ident(ident) => {
                let cmd = parse_cmd(&mut lex, ident, tok.line, tok.col, locations, controller)?;
                cmds.push(cmd);
            }
            other => {
                return Err(err_at(&tok, format!("expected command, got {other:?}")));
            }
        }
    }
    Ok(cmds)
}

fn parse_cmd(
    lex: &mut Lexer,
    ident: &str,
    line: usize,
    col: usize,
    locations: &HashMap<String, (u16, u16)>,
    controller: &mut Controller,
) -> Result<Cmd, ScriptError> {
    match ident {
        "msg" => parse_msg(lex, controller),
        "tmsg" => parse_tmsg(lex, locations, controller),
        "smsg" => parse_smsg(lex, locations, controller),
        "tp" => parse_tp(lex, locations, controller),
        "tpif" => parse_tpif(lex, locations, controller),
        "if" => parse_if(lex, locations, controller),
        "setflag" | "unsetflag" | "readflag" => parse_flag_cmd(lex, ident, controller),
        "giveitem" => parse_giveitem(lex, line, col),
        other => Err(ScriptError {
            line,
            col,
            msg: format!("unknown command: {other:?}"),
        }),
    }
}

fn expect(
    lex: &mut Lexer,
    pred: impl Fn(&TokenKind) -> bool,
    what: &str,
) -> Result<Token, ScriptError> {
    let tok = lex.next()?;
    if !pred(&tok.kind) {
        return Err(err_at(&tok, format!("expected {what}, got {:?}", tok.kind)));
    }
    Ok(tok)
}

fn parse_msg(lex: &mut Lexer, controller: &mut Controller) -> Result<Cmd, ScriptError> {
    let tok = expect(lex, |k| matches!(k, TokenKind::Text(_)), "text")?;
    let TokenKind::Text(text) = tok.kind else {
        unreachable!()
    };
    let index = controller.insert_text(&text);
    Ok(Cmd::Msg {
        text: Text { text, index },
    })
}

fn parse_location(
    lex: &mut Lexer,
    locations: &HashMap<String, (u16, u16)>,
    controller: &mut Controller,
) -> Result<Location, ScriptError> {
    let Token { kind, line, col } = lex.next()?;
    match kind {
        TokenKind::At(name) => {
            controller.insert_tag(&name);
            match locations.get(&name) {
                Some(&(x, y)) => Ok(Location::Cords(x, y)),
                None => Err(err(line, col, format!("unknown location: {name:?}"))),
            }
        }
        TokenKind::Number(x) => {
            let second = expect(lex, |k| matches!(k, TokenKind::Number(_)), "number")?;
            let TokenKind::Number(y) = second.kind else {
                unreachable!()
            };
            Ok(Location::Cords(x, y))
        }
        other => Err(err(line, col, format!("expected location, got {other:?}"))),
    }
}

fn parse_tmsg(
    lex: &mut Lexer,
    locations: &HashMap<String, (u16, u16)>,
    controller: &mut Controller,
) -> Result<Cmd, ScriptError> {
    let at = parse_location(lex, locations, controller)?;
    let tok = expect(lex, |k| matches!(k, TokenKind::Text(_)), "text")?;
    let TokenKind::Text(text) = tok.kind else {
        unreachable!()
    };
    let index = controller.insert_text(&text);
    Ok(Cmd::TMsg {
        at,
        text: Text { text, index },
    })
}

fn parse_smsg(
    lex: &mut Lexer,
    locations: &HashMap<String, (u16, u16)>,
    controller: &mut Controller,
) -> Result<Cmd, ScriptError> {
    let selected = parse_location(lex, locations, controller)?;
    let tok = expect(lex, |k| matches!(k, TokenKind::Text(_)), "text")?;
    let TokenKind::Text(text) = tok.kind else {
        unreachable!()
    };
    let index = controller.insert_text(&text);
    Ok(Cmd::SMsg {
        selected,
        text: Text { text, index },
    })
}

fn parse_tp(
    lex: &mut Lexer,
    locations: &HashMap<String, (u16, u16)>,
    controller: &mut Controller,
) -> Result<Cmd, ScriptError> {
    let from_or_to = parse_location(lex, locations, controller)?;
    // Existing Tiled maps use `tp from to` for the conditional two-location
    // form. Preserve the one-location `tp to` form and emit TpIf whenever a
    // second location immediately follows, matching the explicit `tpif` ABI.
    let next_is_location = matches!(
        lex.clone().next()?.kind,
        TokenKind::At(_) | TokenKind::Number(_)
    );
    if next_is_location {
        let to = parse_location(lex, locations, controller)?;
        Ok(Cmd::TpIf {
            from: from_or_to,
            to,
        })
    } else {
        Ok(Cmd::Tp { to: from_or_to })
    }
}

fn parse_tpif(
    lex: &mut Lexer,
    locations: &HashMap<String, (u16, u16)>,
    controller: &mut Controller,
) -> Result<Cmd, ScriptError> {
    let from = parse_location(lex, locations, controller)?;
    let to = parse_location(lex, locations, controller)?;
    Ok(Cmd::TpIf { from, to })
}

fn parse_if(
    lex: &mut Lexer,
    locations: &HashMap<String, (u16, u16)>,
    controller: &mut Controller,
) -> Result<Cmd, ScriptError> {
    let condition = parse_condition(lex, controller)?;
    let then_cmd = parse_branch(lex, locations, controller)?;
    let then_cmd = then_cmd.ok_or_else(|| {
        let (line, col) = lex.position();
        ScriptError {
            line,
            col,
            msg: "if branch must have then branch".to_string(),
        }
    })?;
    let else_cmd = parse_branch(lex, locations, controller)?;

    let branches = match else_cmd {
        Some(else_cmd) => {
            // ThenElse form: the token after the else-branch command must be
            // `endif`, which we consume here (Go parseIf behaviour).
            let tok = lex.next()?;
            match tok.kind {
                TokenKind::Ident(ref t) if t == "endif" => {}
                _ => return Err(err_at(&tok, "if: expected endif")),
            }
            Branch::ThenElse(Box::new(then_cmd), Box::new(else_cmd))
        }
        None => {
            // Then-only form: `endif` was already consumed by the second
            // parse_branch's lookahead (Go behaviour).
            Branch::Then(Box::new(then_cmd))
        }
    };

    Ok(Cmd::If {
        condition,
        branches,
    })
}

fn parse_condition(lex: &mut Lexer, controller: &mut Controller) -> Result<Condition, ScriptError> {
    let Token { kind, line, col } = lex.next()?;
    match kind {
        TokenKind::Ident(flag) if flag.starts_with("flag_") => {
            let index = controller.insert_flag(&flag);
            Ok(Condition::FlagSet(Text { text: flag, index }))
        }
        TokenKind::Ident(flag) => Err(err(line, col, format!("if: expected flag_, got {flag:?}"))),
        TokenKind::Bang(flag) if flag.starts_with("flag_") => {
            let index = controller.insert_flag(&flag);
            Ok(Condition::FlagClear(Text { text: flag, index }))
        }
        TokenKind::Bang(flag) => Err(err(line, col, format!("if: expected flag_, got !{flag:?}"))),
        _ => Err(err(line, col, "if: expected flag or !flag")),
    }
}

/// Parse one branch command. `endif` (consumed) terminates a branch without
/// producing a command; the optional `then`/`else` keyword is consumed.
fn parse_branch(
    lex: &mut Lexer,
    locations: &HashMap<String, (u16, u16)>,
    controller: &mut Controller,
) -> Result<Option<Cmd>, ScriptError> {
    let Token { kind, line, col } = lex.next()?;
    match kind {
        TokenKind::Ident(kw) if kw == "endif" => Ok(None),
        TokenKind::Ident(kw) if kw == "then" || kw == "else" => {
            // Consume the keyword, then the real command must follow.
            let Token { kind, line, col } = lex.next()?;
            match kind {
                TokenKind::Ident(ident) => {
                    parse_cmd(lex, &ident, line, col, locations, controller).map(Some)
                }
                other => Err(err(
                    line,
                    col,
                    format!("if branch: expected command after {kw}, got {other:?}"),
                )),
            }
        }
        TokenKind::Ident(ident) => {
            parse_cmd(lex, &ident, line, col, locations, controller).map(Some)
        }
        other => Err(err(
            line,
            col,
            format!("if branch: expected command or endif, got {other:?}"),
        )),
    }
}

fn parse_flag_cmd(
    lex: &mut Lexer,
    op: &str,
    controller: &mut Controller,
) -> Result<Cmd, ScriptError> {
    let tok = expect(lex, |k| matches!(k, TokenKind::Ident(_)), "flag")?;
    let Token { kind, line, col } = tok;
    let TokenKind::Ident(flag) = kind else {
        unreachable!()
    };
    if !flag.starts_with("flag_") {
        return Err(err(line, col, format!("{op}: invalid flag {flag:?}")));
    }
    let index = controller.insert_flag(&flag);
    let flag = Text { text: flag, index };
    match op {
        "setflag" => Ok(Cmd::SetFlag { flag }),
        "unsetflag" => Ok(Cmd::UnsetFlag { flag }),
        "readflag" => Ok(Cmd::ReadFlag { flag }),
        _ => unreachable!(),
    }
}

/// Parse `giveitem <kind> <id> <count>`; the three operands are authored as
/// decimal numbers and validated host-side before any byte is emitted.
fn parse_giveitem(lex: &mut Lexer, line: usize, col: usize) -> Result<Cmd, ScriptError> {
    let kind = parse_u16_operand(lex, "kind")?;
    let id = parse_u16_operand(lex, "id")?;
    let count = parse_u16_operand(lex, "count")?;
    validate_give_item(kind, id, count)
        .map_err(|msg| err(line, col, format!("giveitem: {msg}")))?;
    Ok(Cmd::GiveItem {
        kind: kind as u8,
        id: id as u8,
        count: count as u8,
    })
}

fn parse_u16_operand(lex: &mut Lexer, what: &str) -> Result<u16, ScriptError> {
    let tok = expect(lex, |k| matches!(k, TokenKind::Number(_)), what)?;
    let TokenKind::Number(value) = tok.kind else {
        unreachable!()
    };
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc_map() -> HashMap<String, (u16, u16)> {
        let mut m = HashMap::new();
        m.insert("testLoc".to_string(), (1, 1));
        m.insert("loc1".to_string(), (1, 2));
        m.insert("loc2".to_string(), (3, 4));
        m
    }

    fn parse_ok(src: &str) -> (Vec<Cmd>, Controller) {
        let mut ctrl = Controller::new();
        let cmds = parse(src, &loc_map(), &mut ctrl).expect("parse ok");
        (cmds, ctrl)
    }

    // ──────────────────────────────────────────────────────────────────
    //  Parse parity with Go script_test.go
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn parity_parse_msg() {
        let (cmds, ctrl) = parse_ok("msg {hello world}");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::Msg { text } => {
                assert_eq!(text.text, "hello world");
                assert_eq!(text.index, 0);
            }
            other => panic!("expected Msg, got {other:?}"),
        }
        assert_eq!(ctrl.texts.get("hello world"), Some(&0));
    }

    #[test]
    fn parity_parse_tmsg() {
        let (cmds, _) = parse_ok("tmsg @testLoc {hello}");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::TMsg { at, text } => {
                assert_eq!(*at, Location::Cords(1, 1));
                assert_eq!(text.text, "hello");
            }
            other => panic!("expected TMsg, got {other:?}"),
        }
    }

    #[test]
    fn parity_parse_tp() {
        let (cmds, _) = parse_ok("tp 10 20");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::Tp { to } => assert_eq!(*to, Location::Cords(10, 20)),
            other => panic!("expected Tp, got {other:?}"),
        }
    }

    #[test]
    fn parity_parse_tpif() {
        let (cmds, _) = parse_ok("tpif @loc1 @loc2");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::TpIf { from, to } => {
                assert_eq!(*from, Location::Cords(1, 2));
                assert_eq!(*to, Location::Cords(3, 4));
            }
            other => panic!("expected TpIf, got {other:?}"),
        }
    }

    #[test]
    fn parity_parse_setflag() {
        let (cmds, _) = parse_ok("setflag flag_door");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::SetFlag { flag } => assert_eq!(flag.text, "flag_door"),
            other => panic!("expected SetFlag, got {other:?}"),
        }
    }

    #[test]
    fn parity_parse_unsetflag() {
        let (cmds, _) = parse_ok("unsetflag flag_door");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::UnsetFlag { flag } => assert_eq!(flag.text, "flag_door"),
            other => panic!("expected UnsetFlag, got {other:?}"),
        }
    }

    #[test]
    fn parity_parse_readflag() {
        let (cmds, _) = parse_ok("readflag flag_door");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::ReadFlag { flag } => assert_eq!(flag.text, "flag_door"),
            other => panic!("expected ReadFlag, got {other:?}"),
        }
    }

    #[test]
    fn parity_parse_smsg() {
        let (cmds, _) = parse_ok("smsg @testLoc {secret}");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::SMsg { selected, text } => {
                assert_eq!(*selected, Location::Cords(1, 1));
                assert_eq!(text.text, "secret");
            }
            other => panic!("expected SMsg, got {other:?}"),
        }
    }

    #[test]
    fn parity_parse_if_then_else() {
        let (cmds, _) = parse_ok("if flag_X then setflag flag_Y else unsetflag flag_Y endif");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::If {
                condition,
                branches,
            } => {
                assert_eq!(
                    *condition,
                    Condition::FlagSet(Text {
                        text: "flag_X".into(),
                        index: 0
                    })
                );
                match branches {
                    Branch::ThenElse(then_cmd, else_cmd) => {
                        assert!(matches!(**then_cmd, Cmd::SetFlag { .. }));
                        assert!(matches!(**else_cmd, Cmd::UnsetFlag { .. }));
                    }
                    other => panic!("expected ThenElse, got {other:?}"),
                }
            }
            other => panic!("expected If, got {other:?}"),
        }
    }

    #[test]
    fn parity_parse_if_then() {
        let (cmds, _) = parse_ok("if flag_test1 then tp 1 1 endif");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::If {
                condition,
                branches,
            } => {
                assert_eq!(
                    *condition,
                    Condition::FlagSet(Text {
                        text: "flag_test1".into(),
                        index: 0
                    })
                );
                match branches {
                    Branch::Then(cmd) => assert!(matches!(**cmd, Cmd::Tp { .. })),
                    other => panic!("expected Then branch, got {other:?}"),
                }
            }
            other => panic!("expected If, got {other:?}"),
        }
    }

    #[test]
    fn parity_parse_empty() {
        let (cmds, _) = parse_ok("");
        assert!(cmds.is_empty());
    }

    #[test]
    fn parity_parse_unknown() {
        let mut ctrl = Controller::new();
        let err = parse("xyz foo", &loc_map(), &mut ctrl).expect_err("must fail");
        assert!(err.msg.contains("unknown command"), "msg: {}", err.msg);
    }

    #[test]
    fn parity_compile_multiple() {
        let (cmds, _) = parse_ok("msg {Hi}; setflag flag_door");
        assert_eq!(cmds.len(), 2);
        match &cmds[0] {
            Cmd::Msg { text } => assert_eq!(text.text, "Hi"),
            other => panic!("bad first cmd: {other:?}"),
        }
        match &cmds[1] {
            Cmd::SetFlag { flag } => assert_eq!(flag.text, "flag_door"),
            other => panic!("bad second cmd: {other:?}"),
        }
    }

    #[test]
    fn parity_controller_dedup() {
        let mut ctrl = Controller::new();
        let id1 = ctrl.insert_text("hello");
        let id2 = ctrl.insert_text("hello");
        assert_eq!(id1, id2);
        let id3 = ctrl.insert_flag("flag_a");
        let id4 = ctrl.insert_flag("flag_b");
        assert_ne!(id3, id4);
        let t1 = ctrl.insert_tag("town");
        let t2 = ctrl.insert_tag("town");
        assert_eq!(t1, t2);
        assert_eq!(t2, 0);
    }

    // ──────────────────────────────────────────────────────────────────
    //  Bytecode parity with Go Command2Bytes / script_test.go
    // ──────────────────────────────────────────────────────────────────

    fn txt(idx: u16, text: &str) -> Text {
        Text {
            text: text.to_string(),
            index: idx,
        }
    }

    #[test]
    fn parity_text_bytes() {
        assert_eq!(txt(0x1224, "dummy").to_bytes(), vec![0x12, 0x24]);
    }

    #[test]
    fn parity_location_bytes() {
        assert_eq!(Location::Cords(1, 2).to_bytes(), vec![0, 1, 0, 2]);
        assert_eq!(Location::Tag(txt(7, "tag")).to_bytes(), vec![255, 0, 7]);
    }

    #[test]
    fn parity_cmd_msg_bytes() {
        let cmd = Cmd::Msg { text: txt(3, "hi") };
        assert_eq!(cmd.to_bytes(), vec![0, 0, 3]);
    }

    #[test]
    fn parity_cmd_tmsg_tag_bytes() {
        let cmd = Cmd::TMsg {
            at: Location::Tag(txt(1, "place")),
            text: txt(2, "hello"),
        };
        assert_eq!(cmd.to_bytes(), vec![1, 255, 0, 1, 0, 2]);
    }

    #[test]
    fn parity_cmd_tmsg_cords_bytes() {
        // Go Command2Bytes: TMsg opcode 1 + Cords(u16 BE, u16 BE) + text index.
        let cmd = Cmd::TMsg {
            at: Location::Cords(10, 20),
            text: txt(0, "Hello"),
        };
        assert_eq!(cmd.to_bytes(), vec![1, 0, 10, 0, 20, 0, 0]);
    }

    #[test]
    fn parity_cmd_smsg_cords_bytes() {
        // Go Command2Bytes: SMsg opcode 2 + Cords + text index.
        let cmd = Cmd::SMsg {
            selected: Location::Cords(3, 4),
            text: txt(3, "Secret"),
        };
        assert_eq!(cmd.to_bytes(), vec![2, 0, 3, 0, 4, 0, 3]);
    }

    #[test]
    fn parity_cmd_tp_cords_bytes() {
        // Go Command2Bytes: Tp opcode 3 + Cords(u16 BE, u16 BE).
        let cmd = Cmd::Tp {
            to: Location::Cords(5, 6),
        };
        assert_eq!(cmd.to_bytes(), vec![3, 0, 5, 0, 6]);
    }

    #[test]
    fn parity_cmd_unsetflag_bytes() {
        let cmd = Cmd::UnsetFlag {
            flag: txt(5, "flag"),
        };
        assert_eq!(cmd.to_bytes(), vec![7, 0, 5]);
    }

    #[test]
    fn parity_cmd_readflag_bytes() {
        let cmd = Cmd::ReadFlag {
            flag: txt(5, "flag"),
        };
        assert_eq!(cmd.to_bytes(), vec![8, 0, 5]);
    }

    #[test]
    fn parity_cmd_tpif_cords_bytes() {
        let cmd = Cmd::TpIf {
            from: Location::Cords(1, 2),
            to: Location::Cords(3, 4),
        };
        assert_eq!(cmd.to_bytes(), vec![4, 0, 1, 0, 2, 0, 3, 0, 4]);
    }

    #[test]
    fn parity_cmd_setflag_bytes() {
        let cmd = Cmd::SetFlag {
            flag: txt(5, "flag"),
        };
        assert_eq!(cmd.to_bytes(), vec![6, 0, 5]);
    }

    #[test]
    fn parity_cmd_end_bytes() {
        assert_eq!(Cmd::End.to_bytes(), vec![255]);
    }

    #[test]
    fn parity_cmd_if_then_bytes() {
        let cmd = Cmd::If {
            condition: Condition::FlagSet(txt(0, "flag_test1")),
            branches: Branch::Then(Box::new(Cmd::Tp {
                to: Location::Cords(1, 1),
            })),
        };
        let b = cmd.to_bytes();
        // [5][0 flag-idx 0][1 branch-kind][5 then-len][3 op 0,1 0,1]
        assert_eq!(b, vec![5, 0, 0, 0, 1, 5, 3, 0, 1, 0, 1]);
        assert_eq!(b[0], 5);
        assert_eq!(b[1], 0);
        assert_eq!(b[4], 1);
    }

    #[test]
    fn parity_cmd_if_then_else_bytes() {
        let cmd = Cmd::If {
            condition: Condition::FlagClear(txt(1, "flag_X")),
            branches: Branch::ThenElse(
                Box::new(Cmd::SetFlag {
                    flag: txt(2, "flag_Y"),
                }),
                Box::new(Cmd::UnsetFlag {
                    flag: txt(2, "flag_Y"),
                }),
            ),
        };
        // [5][1 0 1][0][3 (then len)][6 0 2][7 0 2]
        assert_eq!(cmd.to_bytes(), vec![5, 1, 0, 1, 0, 3, 6, 0, 2, 7, 0, 2]);
    }

    // ──────────────────────────────────────────────────────────────────
    //  @location resolution
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn location_ref_resolves_to_cords() {
        let (cmds, ctrl) = parse_ok("tmsg @testLoc {hi}; tp @loc2; tpif @loc1 @loc2");
        assert_eq!(cmds.len(), 3);
        match &cmds[0] {
            Cmd::TMsg { at, .. } => assert_eq!(*at, Location::Cords(1, 1)),
            other => panic!("got {other:?}"),
        }
        match &cmds[1] {
            Cmd::Tp { to } => assert_eq!(*to, Location::Cords(3, 4)),
            other => panic!("got {other:?}"),
        }
        match &cmds[2] {
            Cmd::TpIf { from, to } => {
                assert_eq!(*from, Location::Cords(1, 2));
                assert_eq!(*to, Location::Cords(3, 4));
            }
            other => panic!("got {other:?}"),
        }
        // All three tags registered in encounter order.
        assert_eq!(ctrl.tags.get("testLoc"), Some(&0));
        assert_eq!(ctrl.tags.get("loc2"), Some(&1));
        assert_eq!(ctrl.tags.get("loc1"), Some(&2));
    }

    #[test]
    fn unknown_location_errors() {
        let mut ctrl = Controller::new();
        let err = parse("tp @nope", &loc_map(), &mut ctrl).expect_err("must fail");
        assert!(err.msg.contains("unknown location"), "msg: {}", err.msg);
    }

    // ──────────────────────────────────────────────────────────────────
    //  Error line/col reporting
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn parse_error_reports_line() {
        let mut ctrl = Controller::new();
        // Command on line 2: `msg` expects a {text} but gets a number.
        let err = parse("msg {ok};\nmsg 5", &loc_map(), &mut ctrl).expect_err("must fail");
        assert_eq!(err.line, 2);
        assert!(err.msg.contains("expected text"), "msg: {}", err.msg);
    }

    #[test]
    fn invalid_flag_name_reports_position() {
        let mut ctrl = Controller::new();
        let err = parse("setflag nope", &loc_map(), &mut ctrl).expect_err("must fail");
        assert_eq!(err.line, 1);
        assert!(err.msg.contains("invalid flag"), "msg: {}", err.msg);
    }

    #[test]
    fn missing_endif_reports_position() {
        let mut ctrl = Controller::new();
        let err = parse(
            "if flag_X then setflag flag_Y else setflag flag_Z",
            &loc_map(),
            &mut ctrl,
        )
        .expect_err("must fail");
        assert!(err.msg.contains("expected endif"), "msg: {}", err.msg);
    }

    #[test]
    fn lexer_error_reports_line_in_parse() {
        let mut ctrl = Controller::new();
        let err = parse("msg {a}\n# c\n$", &loc_map(), &mut ctrl).expect_err("must fail");
        assert_eq!(err.line, 3);
        assert!(err.msg.contains("unexpected character"), "msg: {}", err.msg);
    }

    // ──────────────────────────────────────────────────────────────────
    //  Extra semantic coverage (Go behaviour)
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn branch_keywords_are_optional() {
        // Go tolerates if-branches without `then`/`else` keywords.
        let (cmds, _) = parse_ok("if flag_X setflag flag_Y else unsetflag flag_Y endif");
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Cmd::If { .. }));
    }

    #[test]
    fn nested_if_parses() {
        let (cmds, _) = parse_ok("if flag_X then if flag_Y then setflag flag_Z endif endif");
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Cmd::If { branches, .. } => match branches {
                Branch::Then(inner) => assert!(matches!(**inner, Cmd::If { .. })),
                other => panic!("expected nested if, got {other:?}"),
            },
            other => panic!("expected If, got {other:?}"),
        }
    }

    #[test]
    fn flag_indexes_are_assigned_in_parse_order() {
        let (_, ctrl) = parse_ok("if flag_A then setflag flag_B else unsetflag flag_C endif");
        assert_eq!(ctrl.flags.get("flag_A"), Some(&0));
        assert_eq!(ctrl.flags.get("flag_B"), Some(&1));
        assert_eq!(ctrl.flags.get("flag_C"), Some(&2));
    }

    #[test]
    fn text_identifiers_are_deduplicated() {
        let (cmds, ctrl) = parse_ok("msg {hi}; msg {hi}; msg {bye}");
        assert_eq!(cmds.len(), 3);
        assert_eq!(ctrl.texts.len(), 2);
        assert_eq!(ctrl.texts.get("hi"), Some(&0));
        assert_eq!(ctrl.texts.get("bye"), Some(&1));
    }

    #[test]
    fn error_is_displayable() {
        let err = ScriptError {
            line: 2,
            col: 5,
            msg: "boom".to_string(),
        };
        assert_eq!(err.to_string(), "line 2, col 5: boom");
    }

    // ──────────────────────────────────────────────────────────────────
    //  GiveItem (opcode 9, contract revision 2026-10-12)
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn parse_giveitem_operands_and_bytes() {
        let (cmds, _) = parse_ok("giveitem 0 5 3");
        assert_eq!(cmds.len(), 1);
        assert_eq!(
            cmds[0],
            Cmd::GiveItem {
                kind: 0,
                id: 5,
                count: 3
            }
        );
        // Pinned four-byte stream, in operand order kind,id,count.
        assert_eq!(cmds[0].to_bytes(), vec![9, 0, 5, 3]);
    }

    #[test]
    fn parse_giveitem_kinds_and_boundaries() {
        for (src, kind, id, count) in [
            ("giveitem 0 0 1", 0u8, 0u8, 1u8),
            ("giveitem 0 23 99", 0, 23, 99),
            ("giveitem 1 0 1", 1, 0, 1),
            ("giveitem 1 7 99", 1, 7, 99),
            ("giveitem 2 0 1", 2, 0, 1),
            ("giveitem 2 7 1", 2, 7, 1),
        ] {
            let (cmds, _) = parse_ok(src);
            assert_eq!(cmds[0], Cmd::GiveItem { kind, id, count }, "src: {src}");
            assert_eq!(cmds[0].to_bytes(), vec![9, kind, id, count], "src: {src}");
        }
    }

    #[test]
    fn parse_giveitem_rejects_bad_authoring() {
        // Unsupported kind.
        let err = parse("giveitem 3 0 1", &loc_map(), &mut Controller::new())
            .expect_err("kind 3 must fail");
        assert!(err.msg.contains("invalid kind"), "msg: {}", err.msg);
        // Lure id above 23.
        let err = parse("giveitem 0 24 1", &loc_map(), &mut Controller::new())
            .expect_err("lure id 24 must fail");
        assert!(err.msg.contains("invalid id"), "msg: {}", err.msg);
        // Consumable id above 7.
        let err = parse("giveitem 1 8 1", &loc_map(), &mut Controller::new())
            .expect_err("consumable id 8 must fail");
        assert!(err.msg.contains("invalid id"), "msg: {}", err.msg);
        // Count out of range.
        let err = parse("giveitem 0 0 0", &loc_map(), &mut Controller::new())
            .expect_err("count 0 must fail");
        assert!(err.msg.contains("invalid count"), "msg: {}", err.msg);
        let err = parse("giveitem 0 0 100", &loc_map(), &mut Controller::new())
            .expect_err("count 100 must fail");
        assert!(err.msg.contains("invalid count"), "msg: {}", err.msg);
    }

    #[test]
    fn parse_giveitem_key_requires_count_one() {
        let err = parse("giveitem 2 0 2", &loc_map(), &mut Controller::new())
            .expect_err("key count 2 must fail");
        assert!(err.msg.contains("key grants require count 1"), "msg: {}", err.msg);
        // Keys are not counted stacks: exactly one is valid.
        let (cmds, _) = parse_ok("giveitem 2 7 1");
        assert_eq!(cmds[0].to_bytes(), vec![9, 2, 7, 1]);
    }

    #[test]
    fn parse_giveitem_reports_command_position() {
        let err = parse("msg {ok};\ngiveitem 0 24 1", &loc_map(), &mut Controller::new())
            .expect_err("must fail");
        assert_eq!(err.line, 2);
        assert_eq!(err.col, 1);
    }

    #[test]
    fn giveitem_inside_branch_keeps_skip_offset() {
        // The then-branch length byte must count GiveItem's four bytes.
        let (cmds, _) = parse_ok("if flag_X then giveitem 0 1 5 endif");
        assert_eq!(
            cmds[0].to_bytes(),
            vec![5, 0, 0, 0, 1, 4, 9, 0, 1, 5]
        );
    }

    #[test]
    fn giveitem_in_then_else_branch_keeps_offsets() {
        let (cmds, _) = parse_ok("if flag_X then giveitem 0 1 5 else giveitem 2 3 1 endif");
        // [If][0 flag=0][0 ThenElse][4 then-len][9 0 1 5][9 2 3 1]
        assert_eq!(
            cmds[0].to_bytes(),
            vec![5, 0, 0, 0, 0, 4, 9, 0, 1, 5, 9, 2, 3, 1]
        );
    }

    #[test]
    fn old_stream_bytes_unchanged_alongside_giveitem() {
        // Prepending a GiveItem must not renumber or resize any existing
        // command's encoding.
        let (cmds, _) = parse_ok("giveitem 0 1 5; msg {hi}; setflag flag_x; tp 5 6");
        assert_eq!(cmds[0].to_bytes(), vec![9, 0, 1, 5]);
        assert_eq!(cmds[1].to_bytes(), vec![0, 0, 0]);
        assert_eq!(cmds[2].to_bytes(), vec![6, 0, 0]);
        assert_eq!(cmds[3].to_bytes(), vec![3, 0, 5, 0, 6]);
    }
}
