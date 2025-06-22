//! Parser that consumes the lexer and builds a `Script` AST.

use crate::model::{
    CHUNK_COLS, CHUNK_H, CHUNK_W, ParsedScripts, Script, ScriptLayer, TOTAL_CHUNKS,
};

use super::ast::*;
use super::lexer::{Lexer, Token};
use super::locations_parser::LocationTags;
use std::collections::HashMap;

pub fn parse_scripts(
    scripts: &ScriptLayer,
    loc_tags: &LocationTags,
) -> Result<ParsedScripts, String> {
    let mut chunks: Vec<Vec<Script>> = vec![Vec::new(); TOTAL_CHUNKS];

    let mut controller = Controller::new();

    for script in &scripts.objects {
        let mut p = Parser::new(&script.script, controller, loc_tags.clone());
        let parse_res = p.parse();
        let cmds = match parse_res {
            Ok(cmds) => cmds,
            Err(e) => return Err(format!("id {} failed: {}", script.id, e)),
        };
        let x_i = script.x as i32 / 16;
        let y_i = script.y as i32 / 16;
        let s = Script {
            script: script.script.clone(),
            body: cmds,
            x: x_i,
            y: y_i,
        };

        let idx = chunk_index(x_i, y_i);
        if !s.body.is_empty() {
            println!(
                "id {} has {} commands for index {} at {},{}",
                script.id,
                s.body.len(),
                idx,
                x_i,
                y_i
            );
            chunks[idx].push(s.clone());
        }

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
    let cx = x / CHUNK_W; // 0‥31 (which 8-wide column)
    let cy = y / CHUNK_H; // 0‥63 (which 4-tall row)
    (cy * CHUNK_COLS + cx) as usize // row-major (0‥2047)
}
struct Controller {
    tags: HashMap<String, u16>,
    flags: HashMap<String, u16>,
    text: HashMap<String, u16>,
    tag_count: u16,
    flag_count: u16,
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

    fn insert_tag(&mut self, tag: &String) -> u16 {
        if !self.tags.contains_key(tag) {
            self.tags.insert(tag.clone(), self.tag_count);
            self.tag_count += 1;
        }
        let v = self.tags.get(tag).unwrap();
        *v
    }
    fn insert_flag(&mut self, flag: &String) -> u16 {
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
    locations: LocationTags,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, controller: Controller, locations: LocationTags) -> Self {
        let lex = Lexer::new(src).peekable();
        Self {
            lex,
            controller,
            locations,
        }
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
        let token = match token_res {
            Ok(t) => t,
            Err(e) => return Err(format!("parse: {e}")),
        };
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

        Ok(Cmd::TpIf { from, to })
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
                self.controller.insert_tag(&at);
                let res = self.locations.get(&at);
                let cords = match res {
                    Some(cords) => cords,
                    None => return Err(format!("location {} not found!", at)),
                };
                Ok(Location::Cords(cords.0, cords.1))
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
            let mut parser = Parser::new(input, Controller::new(), HashMap::new());
            let result = parser.parse_cmd();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_parse_tmsg() {
        let mut locations = LocationTags::new();
        locations.insert("testLoc".into(), (1, 1));
        locations.insert("loc2".into(), (2, 2));
        let test_cases = vec![(
            "tmsg @testLoc {hello world};",
            Ok(Cmd::TMsg {
                at: Location::Cords(1, 1),
                text: Text {
                    text: "hello world".into(),
                    index: 0,
                },
            }),
        )];

        for (input, expected) in test_cases {
            let mut parser = Parser::new(input, Controller::new(), locations.clone());
            let result = parser.parse_cmd();
            assert_eq!(result, expected);
        }
    }
    #[test]
    fn test_parse_tp() {
        let mut locations = LocationTags::new();
        locations.insert("loc1".into(), (1, 1));
        locations.insert("loc2".into(), (2, 2));
        let test_cases = vec![
            (
                "tp @loc1 @loc2;",
                Ok(Cmd::TpIf {
                    from: Location::Cords(1, 1),
                    to: Location::Cords(2, 2),
                }),
            ),
            (
                "tp @loc1 1 2;",
                Ok(Cmd::TpIf {
                    from: Location::Cords(1, 1),
                    to: Location::Cords(1, 2),
                }),
            ),
            (
                "tp 1 2 @loc1;",
                Ok(Cmd::TpIf {
                    from: Location::Cords(1, 2),
                    to: Location::Cords(1, 1),
                }),
            ),
            (
                "tp 256 256 0 0;",
                Ok(Cmd::TpIf {
                    from: Location::Cords(256, 256),
                    to: Location::Cords(0, 0),
                }),
            ),
        ];

        for (input, expected) in test_cases {
            let mut parser = Parser::new(input, Controller::new(), locations.clone());
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
            (
                "if flag_test1 then tp 1 1 0 0 endif;",
                Ok(Cmd::If {
                    condition: Condition::FlagSet(Text {
                        text: "flag_test1".into(),
                        index: 0,
                    }),
                    branches: Branch::Then(Box::new(Cmd::TpIf {
                        from: Location::Cords(1, 1),
                        to: Location::Cords(0, 0),
                    })),
                }),
            ),
        ];

        for (input, expected) in test_cases {
            println!("Testing: {input}");

            let mut parser = Parser::new(input, Controller::new(), HashMap::new());
            let result = parser.parse_cmd();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn test_collects_tags_and_flags() {
        let script = "if flag_A then setflag flag_B else unsetflag flag_C endif;";
        let script_entry = ScriptEntry {
            id: 0,
            script: script.to_string(),
            x: 0 as f32,
            y: 0 as f32,
        };

        let script_layer = ScriptLayer {
            objects: vec![script_entry],
        };
        let parsed_scripts = parse_scripts(&script_layer, &HashMap::new()).unwrap();
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
                id: 0,
                script: "msg {a};".into(),
                x: 0.0,
                y: 0.0, //  chunk 0
            },
            ScriptEntry {
                id: 0,
                script: "msg {b};".into(),
                x: 8.0 * 16 as f32,
                y: 0.0, //  chunk 1
            },
            ScriptEntry {
                id: 0,
                script: "msg {c};".into(),
                x: 0.0,
                y: 4.0 * 16 as f32, //  first row below → chunk 32
            },
        ];

        let layer = ScriptLayer { objects: scripts };
        let parsed = parse_scripts(&layer, &HashMap::new()).expect("scripts parsed");

        // chunk 0 must contain (0,0)
        assert_eq!(parsed.chunks[0].len(), 1, "chunk 0 scripts");
        assert_eq!(parsed.chunks[0][0].x, 0);
        assert_eq!(parsed.chunks[0][0].y, 0);

        // chunk 1 must contain (8,0)
        assert_eq!(parsed.chunks[1].len(), 1, "chunk 1 scripts");
        assert_eq!(parsed.chunks[1][0].x, 8);
        assert_eq!(parsed.chunks[1][0].y, 0);

        // chunk 32 (one full row down) must contain (0,4)
        let idx_row1_col0 = CHUNK_COLS as usize; // 32
        assert_eq!(parsed.chunks[idx_row1_col0].len(), 1, "chunk 2 scripts");
        assert_eq!(parsed.chunks[idx_row1_col0][0].x, 0);
        assert_eq!(parsed.chunks[idx_row1_col0][0].y, 4);
    }
}
