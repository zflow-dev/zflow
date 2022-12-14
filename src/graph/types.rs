use std::{collections::HashMap, path::Path};
use serde::{Deserialize, Serialize};
use serde_json::{Value, Map};

#[derive(Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id:String,
    pub uid:String,
    pub component:String,
    pub metadata:Option<Map<String, Value>>
}


#[derive(Clone, Serialize, Deserialize)]
pub struct GraphNodeJson {
    pub component:String,
    pub metadata:Option<Map<String, Value>>
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GraphLeaf {
    pub port:String,
    pub node_id:String,
    pub index:Option<usize>
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GraphLeafJson {
    pub port:String,
    pub process:String,
    pub index:Option<usize>
}

#[derive(Clone)]
pub enum StubData {
    Number(f32),
    String(String),
    Path(&'static Path),
    Bytes(Vec<u8>)
}



#[derive(Clone, Serialize, Deserialize)]
pub struct GraphStub {
    pub data:Value
}


#[derive(Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from:GraphLeaf,
    pub to: GraphLeaf,
    pub metadata:Option<Map<String, Value>>
}


#[derive(Clone, Serialize, Deserialize)]
pub struct GraphEdgeJson {
    pub src:Option<GraphLeafJson>,
    pub tgt: Option<GraphLeafJson>,
    pub data:Option<Value>,
    pub metadata:Option<Map<String, Value>>
}


#[derive(Clone,Serialize, Deserialize)]
pub struct GraphIIP {
    pub to: Option<GraphLeaf>,
    pub from: Option<GraphStub>,
    pub metadata:Option<Map<String, Value>>
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GraphGroup {
    pub name:String,
    pub nodes: Vec<String>,
    pub metadata:Option<Map<String, Value>>
}


#[derive(Clone, Serialize, Deserialize)]
pub struct GraphExportedPort {
    pub process:String,
    pub port:String,
    pub metadata:Option<Map<String, Value>>
}


#[derive(Clone)]
pub struct GraphTransaction {
    pub id:Option<String>,
    pub depth: i32
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphJson {
    pub case_sensitive: bool,
    pub properties: Map<String, Value>,
    pub inports: HashMap<String, GraphExportedPort>,
    pub outports: HashMap<String, GraphExportedPort>,
    pub groups: Vec<GraphGroup>,
    pub processes: HashMap<String, GraphNodeJson>,
    pub connections: Vec<GraphEdgeJson>
}