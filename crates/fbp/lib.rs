pub mod types;

use regex::Regex;
use serde_json::{json, Map, Value};

pub struct FBP<'a> {
    source: &'a str,
    graph: GraphJson,
}

use regex_lexer::{Error, LexerBuilder, Token};
use types::*;

struct FBPLexer;

impl FBPLexer {
    pub(crate) fn run<'a>(source: &'a str) -> Result<Vec<FBPToken>, Error> {
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
                [Token {
                    kind: Kind::IIPChar,
                    span: _,
                    text: iip,
                }, ..] => {
                    atoms.remove(0);
                    tokens.push(FBPToken::IIPChar(
                        iip.to_string().replace("'", "").to_string(),
                    ));
                }
                [Token {
                    kind: Kind::InPortOutPort,
                    span: _,
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
                            span: _,
                            text: _,
                        }, ..] => {
                            atoms.remove(0);

                            match atoms.clone().as_slice() {
                                [Token {
                                    kind: Kind::LeftRightPort,
                                    span:_,
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
                    span:_,
                    text,
                }, ..] => {
                    atoms.remove(0);
                    tokens.push(FBPToken::Doc(text.to_string()));
                }
                [Token {
                    kind: Kind::At,
                    span:_,
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
                    span:_,
                    text: name,
                }, ..] => {
                    atoms.remove(0);

                    match atoms.clone().as_slice() {
                        [Token {
                            kind: Kind::Component,
                            span: _,
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
                                            span:_,
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
                            span:_,
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
                                    span:_,
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
                    span:_,
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
                            span:_,
                            text: index,
                        }, ..] => {
                            atoms.remove(0);
                            let port_index = index
                                .parse::<usize>()
                                .expect("expected port index to be an integer");
                            port_info.index = Some(port_index);
                        }
                        [..] => {}
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

impl<'a> FBP<'a> {
    pub fn new(source: &'a str, name: &'a str, case_sensitive: bool) -> Self {
        Self {
            source,
            graph: GraphJson {
                case_sensitive,
                properties: Map::from_iter([("name".to_owned(), json!(name))]),
                ..Default::default()
            },
        }
    }

    fn advance(tokens: &mut Vec<FBPToken>) {
        if tokens.is_empty() {
            return;
        }
        tokens.remove(0);
    }

    fn resolve_process_name(node: &'a str, component: &'a str) -> String {
        return format!("{}/{}", node, component);
    }

    pub fn parse(&mut self) -> Result<GraphJson, Error> {
        let tokens = &mut FBPLexer::run(self.source)?;

        while !tokens.is_empty() {
            match &tokens.clone()[..] {
                [FBPToken::Doc(_), ..] => {
                    FBP::advance(tokens);
                }
                [FBPToken::Annotation(data1, data2), ..] => {
                    self.graph.properties.insert(data1.clone(), json!(data2));
                    FBP::advance(tokens);
                }
                [FBPToken::Inport(node_info, port_info), ..] => {
                    self.graph.inports.insert(
                        port_info.name.clone(),
                        GraphExportedPort {
                            process: node_info.name.clone(),
                            port: if let Some(port) = node_info.port.clone() {
                                port.name
                            } else {
                                port_info.name.clone()
                            },
                            ..Default::default()
                        },
                    );
                    FBP::advance(tokens);
                }
                [FBPToken::Outport(node_info, port_info), ..] => {
                    self.graph.outports.insert(
                        port_info.name.clone(),
                        GraphExportedPort {
                            process: node_info.name.clone(),
                            port: if let Some(port) = node_info.port.clone() {
                                port.name
                            } else {
                                port_info.name.clone()
                            },
                            ..Default::default()
                        },
                    );
                    FBP::advance(tokens);
                }
                [FBPToken::Node(node1, op1), FBPToken::Arrow, FBPToken::Port(port2), FBPToken::Node(node2, op2), ..] =>
                {
                    let re_map_data = Regex::new(
                        r"(([a-zA-Z0-9_\-]*=[a-zA-Z0-9_\-]*),?)+([a-zA-Z0-9_\-]*=[a-zA-Z0-9_\-]*)?",
                    )?;

                    let component_meta = |meta: &str| -> Option<Map<String, Value>> {
                        let mut map = Map::new();
                        let metas: Vec<&str> = meta.split(",").collect();
                        for meta in metas {
                            let split: Vec<&str> = meta.split("=").collect();
                            if split.len() > 1 {
                                let key = split[0];
                                let val = split[1];
                                if !key.is_empty() && !val.is_empty() {
                                    map.insert(key.to_string(), json!(val.to_string()));
                                }
                            }
                        }
                        Some(map)
                    };

                    let mut node1_name = node1.name.clone();
                    if !self.graph.case_sensitive {
                        node1_name = node1_name.to_lowercase();
                    }
                    let mut node2_name = node2.name.clone();
                    if !self.graph.case_sensitive {
                        node2_name = node2_name.to_lowercase();
                    }

                    let port1 = node1.port.clone().unwrap();

                    let mut port1_name = port1.name;
                    if !self.graph.case_sensitive {
                        port1_name = port1_name.to_lowercase();
                    }
                    let mut port2_name = port2.name.clone();
                    if !self.graph.case_sensitive {
                        port2_name = port2_name.to_lowercase();
                    }
                    

                    let (process1_name, compname1, meta1) = if let Some(tok) = op1 {
                        match tok.as_ref() {
                            FBPToken::Component(compname, meta) => {

                                if !compname.is_empty() {
                                    let mut compname = compname.clone();
                                    let mut name = FBP::resolve_process_name(&node1_name, &compname);
                                    if !self.graph.case_sensitive {
                                        name = name.to_lowercase();
                                        compname = compname.to_lowercase();
                                    }
                                    (name, compname.clone(), Some(meta))
                                } else {
                                    (node1_name.clone(), node1_name.clone(), None)
                                }
                            }
                            _ => (node1_name.clone(), node1_name.clone(), None),
                        }
                    } else {
                        (node1_name.clone(), node1_name.clone(), None)
                    };

                    if self.graph.processes.contains_key(&process1_name) {
                        if let Some(meta) = meta1 {
                            if let Some(comp) = self.graph.processes.get_mut(&process1_name) {
                                if re_map_data.is_match(meta) {
                                    comp.metadata = component_meta(meta);
                                }
                            }
                        }
                    } else {
                        self.graph.processes.insert(
                            process1_name.clone(),
                            GraphNodeJson {
                                component: compname1,
                                metadata: if let Some(meta) = meta1 {
                                    if re_map_data.is_match(meta) {
                                        component_meta(meta)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                },
                            },
                        );
                    }

                    let (process2_name, compname2, meta2) = if let Some(tok) = op2 {
                        match tok.as_ref() {
                            FBPToken::Component(compname, meta) => {
                                if !compname.is_empty() {
                                    let mut compname = compname.clone();
                                    let mut name = FBP::resolve_process_name(&node2_name, &compname);
                                    if !self.graph.case_sensitive {
                                        name = name.to_lowercase();
                                        compname = compname.to_lowercase();
                                    }
                                    (name, compname.clone(), Some(meta))
                                } else {
                                    (node2_name.clone(), node2_name.clone(), None)
                                }
                            }
                            _ => (node2_name.clone(), node2_name.clone(), None),
                        }
                    } else {
                        (node2_name.clone(), node2_name.clone(), None)
                    };

                    if self.graph.processes.contains_key(&process2_name) {
                        if let Some(meta) = meta2 {
                            if let Some(comp) = self.graph.processes.get_mut(&process2_name) {
                                if re_map_data.is_match(meta) {
                                    comp.metadata = component_meta(meta)
                                }
                            }
                        }
                    } else {
                        self.graph.processes.insert(
                            process2_name.clone(),
                            GraphNodeJson {
                                component: compname2,
                                metadata: if let Some(meta) = meta2 {
                                    if re_map_data.is_match(meta) {
                                        component_meta(meta)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                },
                            },
                        );
                    }

                    
                    self.graph.connections.push(GraphEdgeJson {
                        src: Some(GraphLeafJson {
                            port: port1_name,
                            process: process1_name.clone(),
                            index: port1.index,
                        }),
                        tgt: Some(GraphLeafJson {
                            port: port2_name.clone(),
                            process: process2_name.clone(),
                            index: port2.index,
                        }),
                        metadata: if let Some(meta) = meta2 {
                            if !re_map_data.is_match(meta) && !meta.is_empty(){
                                Some(Map::from_iter([("name".to_owned(), json!(meta))]))
                            } else {
                                None
                            }
                        } else {
                            None
                        },
                        ..Default::default()
                    });
                    FBP::advance(tokens);
                    FBP::advance(tokens);
                    FBP::advance(tokens);
                },
                [FBPToken::IIPChar(data), FBPToken::Arrow, FBPToken::Port(port2), FBPToken::Node(node2, op2), ..] => {
                    let re_map_data = Regex::new(
                        r"(([a-zA-Z0-9_\-]*=[a-zA-Z0-9_\-]*),?)+([a-zA-Z0-9_\-]*=[a-zA-Z0-9_\-]*)?",
                    )?;

                    let component_meta = |meta: &str| -> Option<Map<String, Value>> {
                        let mut map = Map::new();
                        let metas: Vec<&str> = meta.split(",").collect();
                        for meta in metas {
                            let split: Vec<&str> = meta.split("=").collect();
                            if split.len() > 1 {
                                let key = split[0];
                                let val = split[1];
                                if !key.is_empty() && !val.is_empty() {
                                    map.insert(key.to_string(), json!(val.to_string()));
                                }
                            }
                        }
                        Some(map)
                    };

                    let mut node2_name = node2.name.clone();
                    if !self.graph.case_sensitive {
                        node2_name = node2_name.to_lowercase();
                    }

                    let mut port2_name = port2.name.clone();
                    if !self.graph.case_sensitive {
                        port2_name = port2_name.to_lowercase();
                    }

                    let (process2_name, compname2, meta2) = if let Some(tok) = op2 {
                        match tok.as_ref() {
                            FBPToken::Component(compname, meta) => {
                                if !compname.is_empty() {
                                    let mut compname = compname.clone();
                                    let mut name = FBP::resolve_process_name(&node2_name, &compname);
                                    if !self.graph.case_sensitive {
                                        name = name.to_lowercase();
                                        compname = compname.to_lowercase();
                                    }
                                    (name, compname.clone(), Some(meta))
                                } else {
                                    (node2_name.clone(), node2_name.clone(), None)
                                }
                            }
                            _ => (node2_name.clone(), node2_name.clone(), None),
                        }
                    } else {
                        (node2_name.clone(), node2_name.clone(), None)
                    };

                    if self.graph.processes.contains_key(&process2_name) {
                        if let Some(meta) = meta2 {
                            if let Some(comp) = self.graph.processes.get_mut(&process2_name) {
                                if re_map_data.is_match(meta) {
                                    comp.metadata = component_meta(meta)
                                }
                            }
                        }
                    } else {
                        self.graph.processes.insert(
                            process2_name.clone(),
                            GraphNodeJson {
                                component: compname2,
                                metadata: if let Some(meta) = meta2 {
                                    if re_map_data.is_match(meta) {
                                        component_meta(meta)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                },
                            },
                        );
                    }

                    self.graph.connections.push(GraphEdgeJson {
                        src: None,
                        tgt: Some(GraphLeafJson {
                            port: port2_name.clone(),
                            process: process2_name.clone(),
                            index: port2.index,
                        }),
                        metadata: if let Some(meta) = meta2 {
                            if !re_map_data.is_match(meta) && !meta.is_empty(){
                                Some(Map::from_iter([("name".to_owned(), json!(meta))]))
                            } else {
                                None
                            }
                        } else {
                            None
                        },
                        data: Some(json!(data)),
                        ..Default::default()
                    });

                    FBP::advance(tokens);
                    FBP::advance(tokens);
                    FBP::advance(tokens)
                },
                [FBPToken::Node(_, __), ..] => {
                    FBP::advance(tokens);
                },
                [FBPToken::IIPChar(_), ..] => {
                    FBP::advance(tokens);
                }
                [FBPToken::Port(_), ..] => FBP::advance(tokens),
                [FBPToken::LongSpace, ..] => FBP::advance(tokens),
                [FBPToken::ShortSpace, ..] => FBP::advance(tokens),
                [FBPToken::Ap, ..] => FBP::advance(tokens),
                [FBPToken::AnyChar(_), ..] => FBP::advance(tokens),
                [FBPToken::Arrow, ..] => FBP::advance(tokens),
                [FBPToken::At, ..] => FBP::advance(tokens),
                [FBPToken::Index(_), ..] => FBP::advance(tokens),
                [FBPToken::BkOpen, ..] => FBP::advance(tokens),
                [FBPToken::BkClose, ..] => FBP::advance(tokens),
                [FBPToken::BrOpen, ..] => FBP::advance(tokens),
                [FBPToken::BrClose, ..] => FBP::advance(tokens),
                [FBPToken::Col, ..] => FBP::advance(tokens),
                [FBPToken::Comma, ..] => FBP::advance(tokens),
                [FBPToken::Dot, ..] => FBP::advance(tokens),
                [FBPToken::Hash, ..] => FBP::advance(tokens),
                [FBPToken::Eof, ..] => break,
                [FBPToken::NewLine, ..] => FBP::advance(tokens),
                [FBPToken::Component(_, _), ..] => FBP::advance(tokens),
                &[] => {}
            }
        }

        Ok(self.graph.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::FBP;

    #[test]
    fn parse_fbp() {
        let graph = FBP::new(
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
            "MyGraph",
            false,
        )
        .parse()
        .expect("Syntax error");

        println!("{:?}", graph);
    }
}
