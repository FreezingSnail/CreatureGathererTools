//! Parser that consumes the lexer and builds a `Script` AST.
//!
//! Supported commands (first slice):
//!   msg     <tile:u16>                 {text}
//!   tmsg    @loc1  @loc2               {text}
//!   tp      x1 y1 x2 y2
//!   tp      @loc1  @loc2
//!   setflag    flag_<n>
//!   unsetflag  flag_<n>
//!   readflag   flag_<n>
//!   end

use crate::model::{ParsedScripts, Script, ScriptLayer};

use super::ast::*;
use super::lexer::{Lexer, Token};
use std::collections::HashSet;

pub fn parse_scripts(scripts: ScriptLayer) -> Result<ParsedScripts, String> {
    let mut parsed_scripts = Vec::<Script>::new();
    let mut controller = Controller::new();
    for script in scripts.objects {
        let mut p = Parser::new(&script.script, controller);
        let cmds = p.parse()?;
        parsed_scripts.push(Script {
            body: cmds,
            x: script.x as i32,
            y: script.y as i32,
        });
        controller = p.controller;
    }
    Ok(ParsedScripts {
        scripts: parsed_scripts,
        tags: controller.tags,
        flags: controller.flags,
    })
}
struct Controller {
    tags: HashSet<String>,
    flags: HashSet<String>,
}
impl Controller {
    fn new() -> Self {
        Self {
            tags: HashSet::new(),
            flags: HashSet::new(),
        }
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
        Ok(Cmd::Msg { text: text.into() })
    }

    fn parse_tmsg(&mut self) -> Result<Cmd, String> {
        let loc = self.parse_location()?;
        let text = self.parse_text()?;
        Ok(Cmd::TMsg {
            at: loc,
            text: text.into(),
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
                self.controller.tags.insert(at.clone());
                Ok(Location::Tag(at))
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
            Token::Ident(flag) => Ok(Condition::FlagSet(flag)),
            Token::Bang(flag) => Ok(Condition::FlagClear(flag)),
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

        self.controller.flags.insert(flag.clone());

        let cmd = match op.as_str() {
            "setflag" => Cmd::SetFlag { flag },
            "unsetflag" => Cmd::UnsetFlag { flag },
            "readflag" => Cmd::ReadFlag { flag },
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
                text: "hello world".into(),
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
                at: Location::Tag("testLoc".into()),
                text: "hello world".into(),
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
                    from: Location::Tag("loc1".into()),
                    to: Location::Tag("loc2".into()),
                }),
            ),
            (
                "tp @loc1 1 2;",
                Ok(Cmd::Tp {
                    from: Location::Tag("loc1".into()),
                    to: Location::Cords(1, 2),
                }),
            ),
            (
                "tp 1 2 @loc1;",
                Ok(Cmd::Tp {
                    from: Location::Cords(1, 2),
                    to: Location::Tag("loc1".into()),
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
                    condition: Condition::FlagSet("flag_X".parse().unwrap()),
                    branches: Branch::ThenElse(
                        Box::new(Cmd::SetFlag {
                            flag: "flag_Y".parse().unwrap(),
                        }),
                        Box::new(Cmd::UnsetFlag {
                            flag: "flag_Y".parse().unwrap(),
                        }),
                    ),
                }),
            ),
            (
                "if flag_X then setflag flag_Y endif;",
                Ok(Cmd::If {
                    condition: Condition::FlagSet("flag_X".parse().unwrap()),
                    branches: Branch::Then(Box::new(Cmd::SetFlag {
                        flag: "flag_Y".parse().unwrap(),
                    })),
                }),
            ),
            (
                "if flag_X then if flag_Y then setflag flag_Z endif endif;",
                Ok(Cmd::If {
                    condition: Condition::FlagSet("flag_X".parse().unwrap()),
                    branches: Branch::Then(Box::new(Cmd::If {
                        condition: Condition::FlagSet("flag_Y".parse().unwrap()),
                        branches: Branch::Then(Box::new(Cmd::SetFlag {
                            flag: "flag_Z".parse().unwrap(),
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
        //                         "if flag_X then setflag flag_Y else unsetflag flag_Y endif;"
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
        assert_eq!(parsed_scripts.flags.len(), 2);
    }
}
