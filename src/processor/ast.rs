//! High-level AST for one script *before* it is lowered to byte-code.
// import the macros needed

#[derive(Debug, Clone, PartialEq)]
pub enum Location {
    Cords(u16, u16),
    Tag(String),
}

#[derive(Debug, Clone, PartialEq)]

pub enum Branch {
    ThenElse(Box<Cmd>, Box<Cmd>),
    Then(Box<Cmd>),
}

#[derive(Debug, Clone, PartialEq)]

pub enum Condition {
    FlagSet(String), // `flag_X`
    FlagClear(String), // `!flag_X`
                     // more variants later: TileEq, VarEq, etc.
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    /// `msg {text}` – show text when selecting a tile.
    Msg { text: String },

    /// `tmsg @loc1 {text}` – shorthand that uses two map
    /// locations instead of raw tile numbers.
    TMsg { at: Location, text: String },

    /// `tp x1 y1 x2 y2` OR `tp @loc1 @loc2` – teleport player.
    Tp { from: Location, to: Location },

    /// `if <condition> { … } [else { … }]`.
    If {
        condition: Condition,
        branches: Branch,
    },

    /// `setflag flag_X`
    SetFlag { flag: String },

    /// `unsetflag flag_X`
    UnsetFlag { flag: String },

    /// `readflag flag_X` – push its value on the VM stack.
    ReadFlag { flag: String },

    /// Script terminator (implicit if omitted).
    End,
}
impl Cmd {
    /// Simple array whose order defines the *numeric opcode* that will be
    /// used in the generated C++ enum (index == opcode).
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

/// One complete script.
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub name: String,
    pub body: Vec<Cmd>,
}
