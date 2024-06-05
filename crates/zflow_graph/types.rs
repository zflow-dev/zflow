use serde::{Deserialize, Serialize};

use serde_json::{Map, Value};

use std::{collections::HashMap, path::Path};

#[cfg(feature = "build_wasm")]
use tsify::Tsify;
#[cfg(feature = "build_wasm")]
use wasm_bindgen::prelude::*;


#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphNode {
    pub id: String,
    pub component: String,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphNodeJson {
    pub component: String,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphLeaf {
    pub port: String,
    pub node_id: String,
    pub index: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphLeafJson {
    pub port: String,
    pub process: String,
    pub index: Option<usize>,
}

#[derive(Clone)]
pub enum StubData {
    Number(f32),
    String(String),
    Path(&'static Path),
    Bytes(Vec<u8>),
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphStub {
    #[cfg_attr(feature = "build_wasm", tsify(type = "any"))]
    pub data: Value,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphEdge {
    pub from: GraphLeaf,
    pub to: GraphLeaf,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphEdgeJson {
    pub src: Option<GraphLeafJson>,
    pub tgt: Option<GraphLeafJson>,
    #[cfg_attr(feature = "build_wasm", tsify(type = "any| undefined"))]
    pub data: Option<Value>,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphIIP {
    pub to: Option<GraphLeaf>,
    pub from: Option<GraphStub>,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphGroup {
    pub name: String,
    pub nodes: Vec<String>,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphExportedPort {
    pub process: String,
    pub port: String,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct GraphTransaction {
    pub id: String,
    pub depth: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct GraphJson {
    pub case_sensitive: bool,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub properties: Map<String, Value>,
    pub inports: HashMap<String, GraphExportedPort>,
    pub outports: HashMap<String, GraphExportedPort>,
    pub groups: Vec<GraphGroup>,
    pub processes: HashMap<String, GraphNodeJson>,
    pub connections: Vec<GraphEdgeJson>,
}

type EventValue = Value;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(namespace))]
#[serde(tag = "_type")]
pub enum GraphEvents {
    AddNode(EventValue),
    RemoveNode(EventValue),
    RenameNode(EventValue),
    ChangeNode(EventValue),
    AddEdge(EventValue),
    RemoveEdge(EventValue),
    // RenameEdge(EventValue),
    ChangeEdge(EventValue),
    AddInitial(EventValue),
    RemoveInitial(EventValue),
    ChangeProperties(EventValue),
    AddGroup(EventValue),
    RemoveGroup(EventValue),
    RenameGroup(EventValue),
    ChangeGroup(EventValue),
    AddInport(EventValue),
    RemoveInport(EventValue),
    RenameInport(EventValue),
    ChangeInport(EventValue),
    AddOutport(EventValue),
    RemoveOutport(EventValue),
    RenameOutport(EventValue),
    ChangeOutport(EventValue),
    StartTransaction(EventValue),
    EndTransaction(EventValue),
    Transaction(EventValue),
    #[default]
    None,
}


impl GraphEvents {
    pub fn to_string(&self) -> &str {
        match self {
            GraphEvents::AddNode(_) => "add_node",
            GraphEvents::RemoveNode(_) => "remove_node",
            GraphEvents::RenameNode(_) => "rename_node",
            GraphEvents::ChangeNode(_) => "change_node",
            GraphEvents::AddEdge(_) => "add_edge",
            GraphEvents::RemoveEdge(_) => "remove_edge",
            GraphEvents::ChangeEdge(_) => "change_edge",
            GraphEvents::AddInitial(_) => "add_initial",
            GraphEvents::RemoveInitial(_) => "remove_initial",
            GraphEvents::ChangeProperties(_) => "change_properties",
            GraphEvents::AddGroup(_) => "add_group",
            GraphEvents::RemoveGroup(_) => "remove_group",
            GraphEvents::RenameGroup(_) => "rename_group",
            GraphEvents::ChangeGroup(_) => "change_group",
            GraphEvents::AddInport(_) => "add_inport",
            GraphEvents::RemoveInport(_) => "remove_inport",
            GraphEvents::RenameInport(_) => "rename_inport",
            GraphEvents::ChangeInport(_) => "change_inport",
            GraphEvents::AddOutport(_) => "add_outport",
            GraphEvents::RemoveOutport(_) => "remove_outport",
            GraphEvents::RenameOutport(_) => "rename_outport",
            GraphEvents::ChangeOutport(_) => "change_outport",
            GraphEvents::StartTransaction(_) => "start_transaction",
            GraphEvents::EndTransaction(_) => "end_transaction",
            GraphEvents::Transaction(_) => "transaction",
            GraphEvents::None => "none"
        }
    }

    fn make_new(name: &str, value: Value) -> GraphEvents {
        match name {
            "add_node" => GraphEvents::AddNode(value),
            "remove_node" => GraphEvents::RemoveNode(value),
            "rename_node" => GraphEvents::RenameNode(value),
            "change_node" => GraphEvents::ChangeNode(value),
            "add_edge" => GraphEvents::AddEdge(value),
            "remove_edge" => GraphEvents::RemoveEdge(value),
            "change_edge" => GraphEvents::ChangeEdge(value),
            "add_initial" => GraphEvents::AddInitial(value),
            "remove_initial" => GraphEvents::RemoveInitial(value),
            "change_properties" => GraphEvents::ChangeProperties(value),
            "add_group" => GraphEvents::AddGroup(value),
            "remove_group" => GraphEvents::RemoveGroup(value),
            "rename_group" => GraphEvents::RenameGroup(value),
            "change_group" => GraphEvents::ChangeGroup(value),
            "add_inport" => GraphEvents::AddInport(value),
            "remove_inport" => GraphEvents::RemoveInport(value),
            "rename_inport" => GraphEvents::RenameInport(value),
            "change_inport" => GraphEvents::ChangeInport(value),
            "add_outport" => GraphEvents::AddOutport(value),
            "remove_outport" => GraphEvents::RemoveOutport(value),
            "rename_outport" => GraphEvents::RenameOutport(value),
            "change_outport" => GraphEvents::ChangeOutport(value),
            "start_transaction" => GraphEvents::StartTransaction(value),
            "end_transaction" => GraphEvents::EndTransaction(value),
            // "transaction" => GraphEvents::Transaction(value),
            _ => panic!("Unknown Graph event {}", name),
        }
    }

    pub fn new(name: &str, value: Value) -> GraphEvents {
        GraphEvents::make_new(name, value)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "build_wasm", derive(Tsify))]
#[cfg_attr(feature = "build_wasm", tsify(into_wasm_abi))]
#[cfg_attr(feature = "build_wasm", tsify(from_wasm_abi))]
pub struct TransactionEntry {
    pub cmd: Option<GraphEvents>,
    pub rev: Option<i32>,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub old: Option<Map<String, Value>>,
    #[cfg_attr(feature = "build_wasm", tsify(type = "Map<string, any> | undefined"))]
    pub new: Option<Map<String, Value>>,
}


