//! High-level AST for one script *before* it is lowered to byte-code.

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub text: String,
    pub index: u16, // numeric id assigned by the parser
}

/* ------------------------------------------------------------------------- */
/*  AST nodes                                                                */
/* ------------------------------------------------------------------------- */

#[derive(Debug, Clone, PartialEq)]
pub enum Location {
    Cords(u16, u16),
    Tag(Text), // ← uses Text now
}

#[derive(Debug, Clone, PartialEq)]
pub enum Branch {
    ThenElse(Box<Cmd>, Box<Cmd>),
    Then(Box<Cmd>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    FlagSet(Text),   // flag_X
    FlagClear(Text), // !flag_X
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    /// `msg {text}`
    Msg {
        text: Text,
    },

    /// `tmsg @loc {text}`
    TMsg {
        at: Location,
        text: Text,
    },

    /// `tp …`
    Tp {
        from: Location,
        to: Location,
    },

    /// `if …`
    If {
        condition: Condition,
        branches: Branch,
    },

    SetFlag {
        flag: Text,
    },
    UnsetFlag {
        flag: Text,
    },
    ReadFlag {
        flag: Text,
    },

    End,
}

impl Cmd {
    pub const VARIANT_NAMES: &'static [&'static str] = &[
        "Msg",
        "TMsg",
        "Tp",
        "If",
        "SetFlag",
        "UnsetFlag",
        "ReadFlag",
        "End",
    ];
}

/* ------------------------------------------------------------------------- */
/*  Byte-code generation                                                     */
/* ------------------------------------------------------------------------- */

/// Everything that can be put into the final byte-stream implements this trait.
pub trait ToBytecode {
    fn to_bytes(&self) -> Vec<u8>;
}

/* -------- Helper ---------- */

fn write_u16(v: u16, out: &mut Vec<u8>) {
    out.extend_from_slice(&v.to_le_bytes());
}

/* -------- Implementations -- */

impl ToBytecode for Text {
    fn to_bytes(&self) -> Vec<u8> {
        self.index.to_le_bytes().to_vec()
    }
}

impl ToBytecode for Location {
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            Location::Cords(x, y) => {
                buf.push(0); // variant tag
                write_u16(*x, &mut buf);
                write_u16(*y, &mut buf);
            }
            Location::Tag(t) => {
                buf.push(1);
                buf.extend_from_slice(&t.to_bytes());
            }
        }
        buf
    }
}

impl ToBytecode for Condition {
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            Condition::FlagSet(f) => {
                buf.push(0);
                buf.extend_from_slice(&f.to_bytes());
            }
            Condition::FlagClear(f) => {
                buf.push(1);
                buf.extend_from_slice(&f.to_bytes());
            }
        }
        buf
    }
}

impl ToBytecode for Branch {
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            Branch::ThenElse(then_cmd, else_cmd) => {
                buf.push(0);
                buf.extend_from_slice(&then_cmd.to_bytes());
                buf.extend_from_slice(&else_cmd.to_bytes());
            }
            Branch::Then(cmd) => {
                buf.push(1);
                buf.extend_from_slice(&cmd.to_bytes());
            }
        }
        buf
    }
}

impl ToBytecode for Cmd {
    fn to_bytes(&self) -> Vec<u8> {
        use Cmd::*;

        let mut buf = Vec::new();
        let opcode = match self {
            Msg { .. } => 0,
            TMsg { .. } => 1,
            Tp { .. } => 2,
            If { .. } => 3,
            SetFlag { .. } => 4,
            UnsetFlag { .. } => 5,
            ReadFlag { .. } => 6,
            End => 7,
        };
        buf.push(opcode);

        match self {
            Msg { text } => {
                buf.extend_from_slice(&text.to_bytes());
            }
            TMsg { at, text } => {
                buf.extend_from_slice(&at.to_bytes());
                buf.extend_from_slice(&text.to_bytes());
            }
            Tp { from, to } => {
                buf.extend_from_slice(&from.to_bytes());
                buf.extend_from_slice(&to.to_bytes());
            }
            If {
                condition,
                branches,
            } => {
                buf.extend_from_slice(&condition.to_bytes());
                buf.extend_from_slice(&branches.to_bytes());
            }
            SetFlag { flag } | UnsetFlag { flag } | ReadFlag { flag } => {
                buf.extend_from_slice(&flag.to_bytes());
            }
            End => { /* nothing */ }
        }
        buf
    }
}
