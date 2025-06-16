//! Parser that consumes the lexer and builds a `Script` AST.
//!
//! Supported commands (first slice):
//!   msg     <tile:u8>                 {text}
//!   tmsg    @loc1  @loc2               {text}
//!   tp      x1 y1 x2 y2
//!   tp      @loc1  @loc2
//!   setflag    flag_<n>
//!   unsetflag  flag_<n>
//!   readflag   flag_<n>
//!   end

use crate::model::{
    CHUNK_COLS, CHUNK_H, CHUNK_W, ParsedScripts, Script, ScriptLayer, TOTAL_CHUNKS,
};

use super::ast::*;
use super::lexer::{Lexer, Token};
use std::collections::HashMap;

pub fn parse_scripts(scripts: ScriptLayer) -> Result<ParsedScripts, String> {
    let mut chunks: Vec<Vec<Script>> = vec![Vec::new(); TOTAL_CHUNKS];

    let mut controller = Controller::new();

    for script in scripts.objects {
        let mut p = Parser::new(&script.script, controller);
        let cmds = p.parse()?;
        let x_i = script.x as i32;
        let y_i = script.y as i32;
        let s = Script {
            body: cmds,
            x: x_i,
            y: y_i,
        };

        let idx = chunk_index(x_i, y_i);
        chunks[idx].push(s.clone());

        controller = p.controller;
    }

    Ok(ParsedScripts {
        chunks,
        tags: controller.tags,
        flags: controller.flags,
        texts: controller.text,
    })
}

/// Convert map-coordinates into the linear chunk index (0‥2047).
#[inline]
fn chunk_index(x: i32, y: i32) -> usize {
    let cx = x / CHUNK_W; // 0‥31
    let cy = y / CHUNK_H; // 0‥63
    (cy * CHUNK_COLS + cx) as usize // row-major (0‥2047)
}
struct Controller {
    tags: HashMap<String, u8>,
    flags: HashMap<String, u8>,
    text: HashMap<String, u16>,
    tag_count: u8,
    flag_count: u8,
    text_count: u16,
}
impl Controller {
    fn new() -> Self {
        Self {
            tags: HashMap::new(),
            flags: HashMap::new(),
            text: HashMap::new(),
            tag_count: 0,
            flag_count: 0,
            text_count: 0,
        }
    }

    fn insert_tag(&mut self, tag: &String) -> u8 {
        if !self.tags.contains_key(tag) {
            self.tags.insert(tag.clone(), self.tag_count);
            self.tag_count += 1;
        }
        let v = self.tags.get(tag).unwrap();
        *v
    }
    fn insert_flag(&mut self, flag: &String) -> u8 {
        if !self.flags.contains_key(flag) {
            self.flags.insert(flag.clone(), self.flag_count);
            self.flag_count += 1;
        }

        let v = self.flags.get(flag).unwrap();
        *v
    }
    fn insert_text(&mut self, text: &String) -> u16 {
        if !self.text.contains_key(text) {
            self.text.insert(text.clone(), self.text_count);
            self.text_count += 1;
        }

        let v = self.text.get(text).unwrap();
        *v
    }
}

struct Parser<'a> {
    lex: std::iter::Peekable<Lexer<'a>>,
    controller: Controller,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, controller: Controller) -> Self {
        let lex = Lexer::new(src).peekable();
        Self { lex, controller }
    }
    fn parse(&mut self) -> Result<Vec<Cmd>, String> {
        let mut res = Vec::<Cmd>::new();
        while self.lex.peek().unwrap().clone() != Ok(Token::Eof) {
            let cmd = self.parse_cmd()?;
            res.push(cmd);
        }
        Ok(res)
    }
    fn parse_cmd(&mut self) -> Result<Cmd, String> {
        // while cmd is not end, iterate thought all tokens

        let token_res = match self.lex.next() {
            Some(token) => token,
            None => return Err(format!("unexpected end of script")),
        };
        let token = token_res.unwrap();
        let cmd = match token {
            Token::Ident(ident) => match ident.as_str() {
                "msg" => self.parse_msg()?,
                "tmsg" => self.parse_tmsg()?,
                "tp" => self.parse_tp()?,
                "if" => self.parse_if()?,
                "setflag" | "unsetflag" | "readflag" => self.parse_flag_cmd(ident)?,

                t => return Err(format!("parse: invalid ident token: {t}")),
            },
            _ => return Err(format!("parse: invalid token: {token:?}")),
        };

        Ok(cmd)
    }
    fn parse_msg(&mut self) -> Result<Cmd, String> {
        let text = self.parse_text()?;
        let i = self.controller.insert_text(&text);
        Ok(Cmd::Msg {
            text: Text {
                text: text,
                index: i,
            },
        })
    }

    fn parse_tmsg(&mut self) -> Result<Cmd, String> {
        let loc = self.parse_location()?;
        let text = self.parse_text()?;
        let i = self.controller.insert_text(&text);
        Ok(Cmd::TMsg {
            at: loc,
            text: Text {
                text: text,
                index: i,
            },
        })
    }

    fn parse_tp(&mut self) -> Result<Cmd, String> {
        let from = match self.parse_location() {
            Err(e) => return Err(e),
            Ok(loc) => loc,
        };

        let to = match self.parse_location() {
            Err(e) => return Err(e),
            Ok(loc) => loc,
        };

        Ok(Cmd::Tp { from, to })
    }

    fn parse_if(&mut self) -> Result<Cmd, String> {
        let condition = self.parse_condition()?;
        let then_branch = self.parse_branch()?;

        let then_branch = match then_branch {
            Some(then_branch) => then_branch,
            None => return Err("if branch must have then branch".to_string()),
        };

        let else_branch = self.parse_branch()?;

        // if else is none  then its an then only branch
        let branches = match else_branch {
            Some(else_branch) => Branch::ThenElse(Box::new(then_branch), Box::new(else_branch)),
            None => Branch::Then(Box::new(then_branch)),
        };

        match branches {
            Branch::ThenElse(_, _) => {
                let endif = match self.lex.next().unwrap() {
                    Ok(Token::Ident(ident)) => match ident.as_str() {
                        "endif" => true,
                        _ => false,
                    },
                    _ => return Err("invalid token after tp".to_string()),
                };
                if !endif {
                    return Err("invalid token after if, expected endif".to_string());
                }
            }
            _ => {}
        }

        Ok(Cmd::If {
            condition,
            branches,
        })
    }

    fn parse_text(&mut self) -> Result<String, String> {
        let text = match self.lex.next().unwrap() {
            Ok(Token::Text(text)) => text,
            _ => return Err("invalid token after tp".to_string()),
        };
        Ok(text)
    }

    fn parse_location(&mut self) -> Result<Location, String> {
        let next = self.lex.next().unwrap();
        let next_token = match next {
            Err(e) => return Err(e),
            Ok(token) => token,
        };

        match next_token {
            Token::At(at) => {
                let i = self.controller.insert_tag(&at);
                Ok(Location::Tag(Text {
                    text: at,
                    index: i as u16,
                }))
            }
            Token::Number(n1) => {
                let n2 = match self
                    .lex
                    .next()
                    .unwrap()
                    .map_err(|e| format!("invalid token after tp {e}"))?
                {
                    Token::Number(n) => n,
                    _ => return Err("invalid token after number".to_string()),
                };

                Ok(Location::Cords(n1, n2))
            }
            _ => Err("invalid token after tp".to_string()),
        }
    }

    fn parse_condition(&mut self) -> Result<Condition, String> {
        let next = self.lex.next().unwrap();
        let next_token = match next {
            Err(e) => return Err(e),
            Ok(token) => token,
        };

        match next_token {
            Token::Ident(flag) => {
                let i = self.controller.insert_flag(&flag);
                Ok(Condition::FlagSet(Text {
                    text: flag,
                    index: i as u16,
                }))
            }
            Token::Bang(flag) => {
                let i = self.controller.insert_flag(&flag);
                Ok(Condition::FlagClear(Text {
                    text: flag,
                    index: i as u16,
                }))
            }
            _ => Err("invalid token after if".to_string()),
        }
    }

    fn parse_branch(&mut self) -> Result<Option<Cmd>, String> {
        let next = self.lex.next().unwrap();
        let next_token = match next {
            Err(e) => return Err(e),
            Ok(token) => token,
        };

        match next_token {
            Token::Ident(t) => match t.as_str() {
                "endif" => Ok(None),
                _ => {
                    let branch = self.parse_cmd()?;
                    Ok(Some(branch))
                }
            },
            _ => Err("invalid token after if".to_string()),
        }
    }

    fn parse_flag_cmd(&mut self, op: String) -> Result<Cmd, String> {
        let next = self.lex.next().ok_or("expected flag after command")??;
        let flag = match next {
            Token::Ident(f) if f.starts_with("flag_") => f,
            other => return Err(format!("invalid flag token: {other:?}")),
        };

        let i = self.controller.insert_flag(&flag);

        let cmd = match op.as_str() {
            "setflag" => Cmd::SetFlag {
                flag: Text {
                    text: flag,
                    index: i as u16,
                },
            },
            "unsetflag" => Cmd::UnsetFlag {
                flag: Text {
                    text: flag,
                    index: i as u16,
                },
            },
            "readflag" => Cmd::ReadFlag {
                flag: Text {
                    text: flag,
                    index: i as u16,
                },
            },
            _ => unreachable!(),
        };
        Ok(cmd)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::ScriptEntry;

    use super::*;

    #[test]
    fn test_parse_msg() {
        let test_cases = vec![(
            "msg {hello world};",
            Ok(Cmd::Msg {
                text: Text {
                    text: "hello world".into(),
                    index: 0,
                },
            }),
        )];

        for (input, expected) in test_cases {
            let mut parser = Parser::new(input, Controller::new());
            let result = parser.parse_cmd();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_parse_tmsg() {
        let test_cases = vec![(
            "tmsg @testLoc {hello world};",
            Ok(Cmd::TMsg {
                at: Location::Tag(Text {
                    text: "testLoc".into(),
                    index: 0,
                }),
                text: Text {
                    text: "hello world".into(),
                    index: 0,
                },
            }),
        )];

        for (input, expected) in test_cases {
            let mut parser = Parser::new(input, Controller::new());
            let result = parser.parse_cmd();
            assert_eq!(result, expected);
        }
    }
    #[test]
    fn test_parse_tp() {
        let test_cases = vec![
            (
                "tp @loc1 @loc2;",
                Ok(Cmd::Tp {
                    from: Location::Tag(Text {
                        text: "loc1".into(),
                        index: 0,
                    }),
                    to: Location::Tag(Text {
                        text: "loc2".into(),
                        index: 1,
                    }),
                }),
            ),
            (
                "tp @loc1 1 2;",
                Ok(Cmd::Tp {
                    from: Location::Tag(Text {
                        text: "loc1".into(),
                        index: 0,
                    }),
                    to: Location::Cords(1, 2),
                }),
            ),
            (
                "tp 1 2 @loc1;",
                Ok(Cmd::Tp {
                    from: Location::Cords(1, 2),
                    to: Location::Tag(Text {
                        text: "loc1".into(),
                        index: 0,
                    }),
                }),
            ),
            (
                "tp 256 256 0 0;",
                Ok(Cmd::Tp {
                    from: Location::Cords(256, 256),
                    to: Location::Cords(0, 0),
                }),
            ),
        ];

        for (input, expected) in test_cases {
            let mut parser = Parser::new(input, Controller::new());
            let result = parser.parse_cmd();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_parse_if() {
        let test_cases = vec![
            (
                "if flag_X then setflag flag_Y else unsetflag flag_Y endif;",
                Ok(Cmd::If {
                    condition: Condition::FlagSet(Text {
                        text: "flag_X".into(),
                        index: 0,
                    }),
                    branches: Branch::ThenElse(
                        Box::new(Cmd::SetFlag {
                            flag: Text {
                                text: "flag_Y".into(),
                                index: 1,
                            },
                        }),
                        Box::new(Cmd::UnsetFlag {
                            flag: Text {
                                text: "flag_Y".into(),
                                index: 1,
                            },
                        }),
                    ),
                }),
            ),
            (
                "if flag_X then setflag flag_Y endif;",
                Ok(Cmd::If {
                    condition: Condition::FlagSet(Text {
                        text: "flag_X".into(),
                        index: 0,
                    }),
                    branches: Branch::Then(Box::new(Cmd::SetFlag {
                        flag: Text {
                            text: "flag_Y".into(),
                            index: 1,
                        },
                    })),
                }),
            ),
            (
                "if flag_X then if flag_Y then setflag flag_Z endif endif;",
                Ok(Cmd::If {
                    condition: Condition::FlagSet(Text {
                        text: "flag_X".into(),
                        index: 0,
                    }),
                    branches: Branch::Then(Box::new(Cmd::If {
                        condition: Condition::FlagSet(Text {
                            text: "flag_Y".into(),
                            index: 1,
                        }),
                        branches: Branch::Then(Box::new(Cmd::SetFlag {
                            flag: Text {
                                text: "flag_Z".into(),
                                index: 2,
                            },
                        })),
                    })),
                }),
            ),
        ];

        for (input, expected) in test_cases {
            println!("Testing: {input}");

            let mut parser = Parser::new(input, Controller::new());
            let result = parser.parse_cmd();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_collects_tags_and_flags() {
        let script = "if flag_A then setflag flag_B else unsetflag flag_C endif;";
        let script_entry = ScriptEntry {
            script: script.to_string(),
            x: 0 as f32,
            y: 0 as f32,
        };

        let script_layer = ScriptLayer {
            objects: vec![script_entry],
        };
        let parsed_scripts = parse_scripts(script_layer).unwrap();
        assert_eq!(parsed_scripts.tags.len(), 0);
        assert_eq!(parsed_scripts.flags.len(), 3);
    }

    /* ------------------------------------------------------------------ */
    /*  Chunk grouping                                                    */
    /* ------------------------------------------------------------------ */

    #[test]
    fn test_chunk_grouping() {
        use crate::model::{CHUNK_COLS, ScriptEntry};

        //
        //  World->chunk layout (8×4 tiles per chunk)
        //
        //  (0,0) → chunk 0
        //  (8,0) → chunk 1
        //  (0,4) → chunk CHUNK_COLS (= 32)
        //
        let scripts = vec![
            ScriptEntry {
                script: "msg {a};".into(),
                x: 0.0,
                y: 0.0, //  chunk 0
            },
            ScriptEntry {
                script: "msg {b};".into(),
                x: 8.0,
                y: 0.0, //  chunk 1
            },
            ScriptEntry {
                script: "msg {c};".into(),
                x: 0.0,
                y: 4.0, //  first row below → chunk 32
            },
        ];

        let layer = ScriptLayer { objects: scripts };
        let parsed = parse_scripts(layer).expect("scripts parsed");

        // chunk 0 must contain (0,0)
        assert_eq!(parsed.chunks[0].len(), 1);
        assert_eq!(parsed.chunks[0][0].x, 0);
        assert_eq!(parsed.chunks[0][0].y, 0);

        // chunk 1 must contain (8,0)
        assert_eq!(parsed.chunks[1].len(), 1);
        assert_eq!(parsed.chunks[1][0].x, 8);
        assert_eq!(parsed.chunks[1][0].y, 0);

        // chunk 32 (one full row down) must contain (0,4)
        let idx_row1_col0 = CHUNK_COLS as usize; // 32
        assert_eq!(parsed.chunks[idx_row1_col0].len(), 1);
        assert_eq!(parsed.chunks[idx_row1_col0][0].x, 0);
        assert_eq!(parsed.chunks[idx_row1_col0][0].y, 4);
    }
}
