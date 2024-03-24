use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, Map};

pub type Spanned<Token, Loc, Error> = Result<(Loc, Token, Loc), Error>;

#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub name: String,
    pub port: Option<PortInfo>,
}

#[derive(Clone, Debug)]
pub struct PortInfo {
    pub name: String,
    pub index: Option<usize>,
}

#[derive(Clone, Debug)]
pub enum FBPToken {
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
pub enum Kind {
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

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphLeafJson {
    pub port: String,
    pub process: String,
    pub index: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphEdgeJson {
    pub src: Option<GraphLeafJson>,
    pub tgt: Option<GraphLeafJson>,
    pub data: Option<Value>,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphNodeJson {
    pub component: String,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphExportedPort {
    pub process: String,
    pub port: String,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphGroup {
    pub name: String,
    pub nodes: Vec<String>,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GraphJson {
    pub case_sensitive: bool,
    pub properties: Map<String, Value>,
    pub inports: HashMap<String, GraphExportedPort>,
    pub outports: HashMap<String, GraphExportedPort>,
    pub groups: Vec<GraphGroup>,
    pub processes: HashMap<String, GraphNodeJson>,
    pub connections: Vec<GraphEdgeJson>,
}