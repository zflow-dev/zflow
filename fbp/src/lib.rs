use regex::Regex;
use serde_json::{Map, Value};

pub struct FBP;

pub type Spanned<Token, Loc, Error> = Result<(Loc, Token, Loc), Error>;

#[derive(Clone, Debug)]
struct NodeInfo {
    pub name: String,
    pub port: Option<PortInfo>,
}

#[derive(Clone, Debug)]
struct PortInfo {
    pub name: String,
    pub index: Option<usize>,
}

#[derive(Clone, Debug)]
enum FBPToken {
    Inport(NodeInfo, PortInfo),
    Outport(NodeInfo, PortInfo),
    Port(PortInfo),
    LongSpace,
    ShortSpace,
    IIPChar(String),
    Ap,
    AnyChar(String),
    Node(NodeInfo, Option<Box<FBPToken>>),
    At,
    Annotation(String, String),
    Doc(String),
    Index(usize),
    BkOpen,
    BkClose,
    BrOpen,
    BrClose,
    Col,
    Comma,
    Dot,
    Hash,
    Eof,
    NewLine,
    Arrow,
    Component(String, String),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Kind {
    InPortOutPort,
    Outport,
    Inport,
    LongSpace,
    ShortSpace,
    StartIIP,
    CloseIIP,
    IIPChar,
    Ap,
    AnyChar,
    Node,
    NodeName,
    At,
    AnnotationKey,
    AnnotationValue,
    Doc,
    Index,
    BkOpen,
    BkClose,
    BrOpen,
    BrClose,
    Col,
    Comma,
    Dot,
    Hash,
    Eof,
    NewLine,
    Arrow,
    Component,
    Name,
    CompMeta,
    CompName,
    Port,
    RightPort,
    LeftPort,
    PortIndex,
}

#[derive(Clone, Debug)]
struct NPort<'input> {
    pub name: &'input str,
    pub index: Option<usize>,
}

#[derive(Clone, Debug)]
struct NMiddlet<'input> {
    pub inport: NPort<'input>,
    pub component: NComponent<'input>,
    pub outport: NPort<'input>,
}

#[derive(Clone, Debug)]
struct NNode<'input> {
    pub name: &'input str,
    pub component: NComponent<'input>,
}

#[derive(Clone, Debug)]
struct NComponent<'input> {
    pub name: &'input str,
    pub meta: Option<Map<String, Value>>,
}

#[derive(Clone, Debug)]
enum Nodes<'input> {
    Port(NPort<'input>),
    MiddLet(NMiddlet<'input>),
    Inport(NPort<'input>, NNode<'input>),
    Outport(NNode<'input>, NPort<'input>),
    Connection(NPort<'input>, NPort<'input>, Option<Vec<Nodes<'input>>>),
    Component(NComponent<'input>),
    Node(NNode<'input>),
    OUTPORT(&'input str, NPort<'input>, &'input str),
    INPORT(&'input str, NPort<'input>, &'input str),
    Comment(&'input str),
    Annotation(&'input str, &'input str),
    IIP(&'input str),
    Eof,
}

use regex_lexer::{Error, LexerBuilder, Token};
use std::{str::CharIndices, string};

struct FBPLexer;

impl FBPLexer {
    pub(crate) fn run(source: &'static str) -> Result<Vec<FBPToken>, Error> {
        let lexer = LexerBuilder::new()
            .token(r"->", Kind::Arrow)
            .token(r"\[", Kind::BkOpen)
            .token(r"\]", Kind::BkClose)
            .token(r"\(", Kind::BrOpen)
            .token(r"\)", Kind::BrClose)
            .token(r":", Kind::Col)
            .token(r",", Kind::Comma)
            .token(r"^'.*'", Kind::IIPChar)
            .token(r"^#.+", Kind::Doc)
            .token(r"^#\s+@.+", Kind::At)
            .token(r"\.", Kind::Dot)
            .token(r"\(([a-zA-Z/\-0-9_]*)?(:.+)?(\))?", Kind::Component)
            .token(r"[a-zA-Z_][a-zA-Z.0-9_]*", Kind::Name)
            // .token(r"->\s+[a-zA-Z_][a-zA-Z.0-9_]*", Kind::RightPort)
            .token(r"[0-9]+", Kind::PortIndex)
            .ignore(r"\n+")
            .token(r"(INPORT|OUTPORT)=([a-zA-Z_][a-zA-Z0-9_\-]*)(\.)(([a-zA-Z_][a-zA-Z.0-9_]*)(:)([a-zA-Z_][a-zA-Z.0-9_]*))", Kind::InPortOutPort)
            .ignore(r"\s+")
            .build().expect("expected to parse source");

        
        let mut atoms = lexer.tokens(source).collect::<Vec<_>>();

        let mut tokens = Vec::new();

        loop {
            match atoms.clone().as_slice() {
                [Token{kind: Kind::Doc, span, text}, ..] => {
                    atoms.remove(0);
                    tokens.push(FBPToken::Doc(text.to_string()));
                },
                [Token{kind: Kind::At, span, text}, ..] => {
                    let text = text.to_string();
                    let text = text.replace("#", "");
                    let annotations = text.trim().split(" ").collect::<Vec<_>>();
                    atoms.remove(0);
                    tokens.push(FBPToken::Annotation(annotations[0].replace("@", "").to_string(), annotations[1].to_string()))
                }
                [Token {
                    kind: Kind::Name,
                    span,
                    text: name,
                }, ..] => {
                    atoms.remove(0);
                    match atoms.clone().as_slice() {
                        [Token {
                            kind: Kind::Component,
                            span: span2,
                            text: comp,
                        }, ..] => {
                            atoms.remove(0);
                            let component = comp.replace("(", "").replace(")", "");
                            let component = component.split(":").collect::<Vec<_>>();
                            let component_name = component[0].to_string();
                            let mut meta = "".to_string();
                            if component.len() > 1 {
                                meta = component[1].to_string();
                            }
                            if comp.starts_with('(') &&  !comp.ends_with(')')  {
                                if atoms[1].kind == Kind::BrClose {
                                    atoms.remove(0);
                                } else {
                                    return Err(Error::Syntax(format!("Opened bracket should be closed for component at {} <==", comp)));
                                }
                                
                            }
                            tokens.push(FBPToken::Node(
                                NodeInfo {
                                    name: name.to_string(),
                                    port: None,
                                },
                                Some(Box::new(FBPToken::Component(
                                    component_name.to_string(),
                                    meta,
                                ))),
                            ));
                        }
                        [Token {
                            kind: Kind::Name,
                            span,
                            text: port,
                        }, ..] => {
                            atoms.remove(0);
                            let mut port_info = PortInfo {
                                name: port.to_string(),
                                index: None,
                            };
                            loop {
                                match atoms.clone().as_slice() {
                                    [Token {
                                        kind: Kind::PortIndex,
                                        span,
                                        text: index,
                                    }, ..] => {
                                        atoms.remove(0);
                                        let port_index = index
                                            .parse::<usize>()
                                            .expect("expected port index to be an integer");
                                        port_info.index = Some(port_index);
                                    }
                                    [..] => {
                                        break;
                                    }
                                }
                            }
                       
                            tokens.push(FBPToken::Node(
                                NodeInfo {
                                    name: name.to_string(),
                                    port: Some(port_info),
                                },
                                None
                                )
                            );
                        }
                        [..] => {
                            tokens.push(FBPToken::Node(
                                NodeInfo {
                                    name: name.to_string(),
                                    port: None,
                                },
                                None)
                            );
                        }
                    }
                }
                [Token {
                    kind: Kind::Arrow,
                    span:_,
                    text:_,
                },
                Token {
                    kind: Kind::Name,
                    span,
                    text: port,
                }, ..] => {
                    atoms.remove(0);
                    atoms.remove(0);
                    let mut port_info = PortInfo {
                        name: port.to_string().replace("-> ", ""),
                        index: None,
                    };
                    loop {
                        match atoms.clone().as_slice() {
                            [Token {
                                kind: Kind::PortIndex,
                                span,
                                text: index,
                            }, ..] => {
                                atoms.remove(0);
                                let port_index = index
                                    .parse::<usize>()
                                    .expect("expected port index to be an integer");
                                port_info.index = Some(port_index);
                            }
                            [..] => {
                                break;
                            }
                        }
                    }
                    tokens.push(FBPToken::Arrow);
                    tokens.push(FBPToken::Port(port_info));
                }
                [..] => {
                    if !atoms.is_empty() {
                        println!("{:?}", atoms[0]);
                        atoms.remove(0);
                    } else {
                        break;
                    }
                }
            }
        }

        Ok(tokens)
    }

    // fn node_component(atoms: &[Token<Kind>]) -> Option<FBPToken> {
    //     match atoms {
    //         [Token {
    //             kind: Kind::BrOpen,
    //             span,
    //             text: _,
    //         }, Token {
    //             kind: Kind::CompName,
    //             span: _,
    //             text,
    //         }, Token {
    //             kind: Kind::BrClose,
    //             span: _,
    //             text: _,
    //         }, ..] => {
    //             return Some(FBPToken::Component(text.to_string(), "".to_owned()));
    //         }
    //         [Token {
    //             kind: Kind::BrOpen,
    //             span,
    //             text: _,
    //         }, Token {
    //             kind: Kind::CompName,
    //             span: _,
    //             text,
    //         }, Token {
    //             kind: Kind::CompMeta,
    //             span: _,
    //             text: meta,
    //         }, Token {
    //             kind: Kind::BrClose,
    //             span: _,
    //             text: _,
    //         }, ..] => {
    //             return Some(FBPToken::Component(text.to_string(), meta.to_string()));
    //         }
    //         // [
    //         //     Token {
    //         //         kind: Kind::CompName,
    //         //         span: _,
    //         //         text,
    //         //     }
    //         // ] => {
    //         //     return Some(FBPToken::Component(text.to_string(), text.to_string()));
    //         // }
    //         [..] => {
    //             if atoms[2].kind != Kind::BrClose {
    //                 panic!("Brack left open");
    //             }
    //             return None;
    //         }
    //     }
    // }
}

impl FBP {
    pub fn parse(source: &str, name: &str, case_sensitive: bool) {}
}

#[cfg(test)]
mod tests {
    use super::FBPLexer;

    #[test]
    fn parse_fbp() {
        let tokens = FBPLexer::run(
            r"
        # A comment line
        # @runtime foo
        INPORT=Read.OPTIONS:CONFIG
        OUTPORT=Process.OUT:RESULT
        Read(ReadFile) OUT -> IN Process(Output:key=value)
        Forward OUT -> IN Log(core/console)
        ",
        ).expect("Syntax error");

    

        println!("{:?}", tokens);
    }
}

