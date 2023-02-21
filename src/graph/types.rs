use std::{collections::HashMap, path::Path};
use serde::{Deserialize, Serialize};
use serde_json::{Value, Map};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GraphNode {
    pub id:String,
    pub uid:String,
    pub component:String,
    pub metadata:Option<Map<String, Value>>
}


#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GraphNodeJson {
    pub component:String,
    pub metadata:Option<Map<String, Value>>
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GraphLeaf {
    pub port:String,
    pub node_id:String,
    pub index:Option<usize>
}

#[derive(Clone, Serialize, Deserialize, Debug)]
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



#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GraphStub {
    pub data:Value
}


#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GraphEdge {
    pub from:GraphLeaf,
    pub to: GraphLeaf,
    pub metadata:Option<Map<String, Value>>
}


#[derive(Clone, Serialize, Deserialize, Debug)]
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GraphGroup {
    pub name:String,
    pub nodes: Vec<String>,
    pub metadata:Option<Map<String, Value>>
}


#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GraphExportedPort {
    pub process:String,
    pub port:String,
    pub metadata:Option<Map<String, Value>>
}


#[derive(Clone, Debug)]
pub struct GraphTransaction {
    pub id:Option<String>,
    pub depth: i32
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum GraphEvents {
    AddNode(Value),
    RemoveNode(Value),
    RenameNode(Value),
    ChangeNode(Value),
    AddEdge(Value),
    RemoveEdge(Value),
    // RenameEdge(Value),
    ChangeEdge(Value),
    AddInitial(Value),
    RemoveInitial(Value),
    ChangeProperties(Value),
    AddGroup(Value),
    RemoveGroup(Value),
    RenameGroup(Value),
    ChangeGroup(Value),
    AddInport(Value),
    RemoveInport(Value),
    RenameInport(Value),
    ChangeInport(Value),
    AddOutport(Value),
    RemoveOutport(Value),
    RenameOutport(Value),
    ChangeOutport(Value),
    StartTransaction(Value),
    EndTransaction(Value),
    Transaction(Value)
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
        }
    }

    pub fn new(name:&str, value:Value) -> GraphEvents {
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
            _ => panic!("Unknown Graph event {}", name)
        }
    }
}