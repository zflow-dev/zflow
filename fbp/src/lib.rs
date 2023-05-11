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
    LeftRightPort,
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
            .token(r"[a-zA-Z_][a-zA-Z0-9_\-]*", Kind::Name)
            // .token(r"->\s+[a-zA-Z_][a-zA-Z.0-9_]*", Kind::RightPort)
            .token(r"[0-9]+", Kind::PortIndex)
            .ignore(r"\n+")
            .token(
                r"(INPORT|OUTPORT)=([a-zA-Z_][a-zA-Z0-9_\-]*)",
                Kind::InPortOutPort,
            )
            .token(
                r"([a-zA-Z_][a-zA-Z.0-9_]*)(:)([a-zA-Z_][a-zA-Z.0-9_]*)",
                Kind::LeftRightPort,
            )
            .ignore(r"\s+")
            .build()
            .expect("expected to parse source");

        let mut atoms = lexer.tokens(source).collect::<Vec<_>>();

        let mut tokens = Vec::new();

        loop {
            match atoms.clone().as_slice() {
                [Token{kind: Kind::IIPChar, span, text:iip}, ..] =>{
                    atoms.remove(0);
                    tokens.push(FBPToken::IIPChar(iip.to_string().replace("'", "").to_string()));
                }
                [Token {
                    kind: Kind::InPortOutPort,
                    span,
                    text,
                }, ..] => {
                    let first_operands = text.split("=").collect::<Vec<_>>();
                    let operand = first_operands[0];
                    let node = first_operands[1];

                    let mut node = NodeInfo {
                        name: node.to_string(),
                        port: None,
                    };
                    atoms.remove(0);
                    match atoms.clone().as_slice() {
                        [Token {
                            kind: Kind::Dot,
                            span,
                            text: name,
                        }, ..] => {
                            atoms.remove(0);

                            match atoms.clone().as_slice() {
                                [Token {
                                    kind: Kind::LeftRightPort,
                                    span,
                                    text: name,
                                }, ..] => {
                                    let operands = name.split(":").collect::<Vec<_>>();
                                    let left_port = operands[0];
                                    let right_port = operands[1];
                                    node.port = Some(PortInfo {
                                        name: left_port.to_string(),
                                        index: None,
                                    });
                                    atoms.remove(0);

                                    match operand {
                                        "INPORT" => {
                                            tokens.push(FBPToken::Inport(
                                                node,
                                                PortInfo {
                                                    name: right_port.to_string(),
                                                    index: None,
                                                },
                                            ));
                                        }
                                        "OUTPORT" => {
                                            tokens.push(FBPToken::Outport(
                                                node,
                                                PortInfo {
                                                    name: right_port.to_string(),
                                                    index: None,
                                                },
                                            ));
                                        }
                                        _ => {}
                                    }
                                }
                                [..] => {}
                            }
                        }
                        [..] => {}
                    }
                }
                [Token {
                    kind: Kind::Doc,
                    span,
                    text,
                }, ..] => {
                    atoms.remove(0);
                    tokens.push(FBPToken::Doc(text.to_string()));
                }
                [Token {
                    kind: Kind::At,
                    span,
                    text,
                }, ..] => {
                    let text = text.to_string();
                    let text = text.replace("#", "");
                    let annotations = text.trim().split(" ").collect::<Vec<_>>();
                    atoms.remove(0);
                    tokens.push(FBPToken::Annotation(
                        annotations[0].replace("@", "").to_string(),
                        annotations[1].to_string(),
                    ))
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
                            let mut node = NodeInfo {
                                name: name.to_string(),
                                port: None,
                            };
                            atoms.remove(0);
                            let component = comp.replace("(", "").replace(")", "");
                            let component = component.split(":").collect::<Vec<_>>();
                            let component_name = component[0].to_string();
                            let mut meta = "".to_string();
                            if component.len() > 1 {
                                meta = component[1].to_string();
                            }
                            if comp.starts_with('(') && !comp.ends_with(')') {
                                if atoms[1].kind == Kind::BrClose {
                                    atoms.remove(0);
                                } else {
                                    return Err(Error::Syntax(format!(
                                        "Opened bracket should be closed for component at {} <==",
                                        comp
                                    )));
                                }
                            }

                            if atoms.len() > 0 {
                                if atoms[0].kind == Kind::Name && atoms[1].kind == Kind::Arrow {
                                    let port = atoms[0].text;
                                    let mut port_info = PortInfo {
                                        name: port.to_string(),
                                        index: None,
                                    };
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
                                            node.port = Some(port_info.clone());
                                        }
                                        [..] => {
                                            node.port = Some(port_info.clone());
                                        }
                                    }
                                }
                            }

                            tokens.push(FBPToken::Node(
                                node,
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
                            let mut node = NodeInfo {
                                name: name.to_string(),
                                port: None,
                            };
                            atoms.remove(0);
                            let mut port_info = PortInfo {
                                name: port.to_string(),
                                index: None,
                            };
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
                                    node.port = Some(port_info.clone());
                                }
                                [..] => {
                                    node.port = Some(port_info.clone());
                                }
                            }

                            tokens.push(FBPToken::Node(node, None));
                        }
                        [..] => {}
                    }
                }
                [Token {
                    kind: Kind::Arrow,
                    span: _,
                    text: _,
                }, Token {
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

                        }
                    }

                    tokens.push(FBPToken::Arrow);
                    tokens.push(FBPToken::Port(port_info));
                }
                [..] => {
                    if !atoms.is_empty() {
                        atoms.remove(0);
                    } else {
                        break;
                    }
                }
            }
        }

        Ok(tokens)
    }
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
        'somefile.txt' -> SOURCE Read(ReadFile) OUT -> IN Split(SplitStr)
        Split OUT -> IN Count(Counter) COUNT -> IN Display(Output)
        Read() ERROR -> IN Display()
        INPORT=Read.IN:FILENAME
        'somefile.txt' -> SOURCE Read(ReadFile:main)
        Read() OUT -> IN Split(SplitStr:main)
        Split() OUT -> IN Count(Counter:main)
        Count() COUNT -> IN Display(Output:main)
        Read() ERROR -> IN Display()
        Read() OUT -> IN Split(SplitStr:foo=bar,baz=123)
        ",
        )
        .expect("Syntax error");

        println!("{:?}", tokens);
    }
}
