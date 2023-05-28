use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::HashMap, path::Path};

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphNode {
    pub id: String,
    // pub uid: String,
    pub component: String,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphNodeJson {
    pub component: String,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphLeaf {
    pub port: String,
    pub node_id: String,
    pub index: Option<usize>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphLeafJson {
    pub port: String,
    pub process:String,
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
pub struct GraphStub {
    pub data: Value,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphEdge {
    pub from: GraphLeaf,
    pub to: GraphLeaf,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphEdgeJson {
    pub src: Option<GraphLeafJson>,
    pub tgt: Option<GraphLeafJson>,
    pub data: Option<Value>,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphIIP {
    pub to: Option<GraphLeaf>,
    pub from: Option<GraphStub>,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphGroup {
    pub name: String,
    pub nodes: Vec<String>,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphExportedPort {
    pub process: String,
    pub port: String,
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct GraphTransaction {
    pub id: String,
    pub depth: i32,
    pub metadata: Value,
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


#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TransactionId(pub &'static str);

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub enum GraphEvents {
    AddNode(GraphNode),
    RemoveNode(String),
    RemoveNodeFromGroup{
        group: String,
        node_index: usize
    },
    RenameNode{
        node: GraphNode,
        old_id: String,
        new_id: String
    },
    ChangeNode{node: GraphNode, old_metadata:Option<Map<String, Value>>, new_metadata:Map<String, Value>, index: usize},
    AddEdge(GraphEdge),
    RemoveEdge(GraphEdge),
    // RenameEdge(Value),
    ChangeEdge{edge: GraphEdge, old_metadata:Option<Map<String, Value>>, new_metadata:Map<String, Value>, index: usize},
    AddInitial(GraphIIP),
    RemoveInitial{
        id: String,
        port: String
    },
    ChangeProperties{r#new: Map<String, Value>, old:Option<Map<String, Value>>},
    AddGroup(GraphGroup),
    RemoveGroup(String),
    RenameGroup{old: String, r#new: String},
    ChangeGroup{name:String, metadata: Map<String, Value>, old_metadata: Option<Map<String, Value>>},
    AddInport{inport:GraphExportedPort, name: String},
    RemoveInport(String),
    RenameInport{old_name:String, new_name:String},
    ChangeInport{name:String, port: GraphExportedPort, old_metadata:Option<Map<String, Value>>, new_metadata:Map<String, Value>},
    AddOutport{outport:GraphExportedPort, name: String},
    RemoveOutport(String),
    RenameOutport{old_name:String, new_name:String},
    ChangeOutport{name:String, port: GraphExportedPort, old_metadata:Option<Map<String, Value>>, new_metadata:Map<String, Value>},
    StartTransaction(TransactionId, Value),
    IncTransaction,
    DecTransaction,
    EndTransaction(TransactionId, Value),
    Transaction(Value),
    #[default]
    Noop
}

impl GraphEvents {
    // pub fn to_string(&self) -> &str {
    //     match self {
    //         GraphEvents::AddNode(_) => "add_node",
    //         GraphEvents::RemoveNode(_) => "remove_node",
    //         GraphEvents::RenameNode{..} => "rename_node",
    //         GraphEvents::ChangeNode{..} => "change_node",
    //         GraphEvents::AddEdge(_) => "add_edge",
    //         GraphEvents::RemoveEdge(_) => "remove_edge",
    //         GraphEvents::ChangeEdge{..} => "change_edge",
    //         GraphEvents::AddInitial(_) => "add_initial",
    //         GraphEvents::RemoveInitial{..} => "remove_initial",
    //         GraphEvents::ChangeProperties(_) => "change_properties",
    //         GraphEvents::AddGroup(_) => "add_group",
    //         GraphEvents::RemoveGroup(_) => "remove_group",
    //         GraphEvents::RenameGroup(_) => "rename_group",
    //         GraphEvents::ChangeGroup(_) => "change_group",
    //         GraphEvents::AddInport{..} => "add_inport",
    //         GraphEvents::RemoveInport(_) => "remove_inport",
    //         GraphEvents::RenameInport(_) => "rename_inport",
    //         GraphEvents::ChangeInport(_) => "change_inport",
    //         GraphEvents::AddOutport{..} => "add_outport",
    //         GraphEvents::RemoveOutport(_) => "remove_outport",
    //         GraphEvents::RenameOutport(_) => "rename_outport",
    //         GraphEvents::ChangeOutport(_) => "change_outport",
    //         GraphEvents::StartTransaction(_, _) => "start_transaction",
    //         GraphEvents::EndTransaction(_, _) => "end_transaction",
    //         GraphEvents::Transaction(_) => "transaction",
    //         GraphEvents::RemoveNodeFromGroup{..} => "remove_node_from_group",
    //         _ =>{ "" }
    //     }
    // }

    // pub fn new(name: &str, value: Value) -> GraphEvents {
    //     match name {
    //         "add_node" => GraphEvents::AddNode(),
    //         "remove_node" => GraphEvents::RemoveNode(value),
    //         "rename_node" => GraphEvents::RenameNode(value),
    //         "change_node" => GraphEvents::ChangeNode(value),
    //         "add_edge" => GraphEvents::AddEdge(value),
    //         "remove_edge" => GraphEvents::RemoveEdge(value),
    //         "change_edge" => GraphEvents::ChangeEdge(value),
    //         "add_initial" => GraphEvents::AddInitial(value),
    //         "remove_initial" => GraphEvents::RemoveInitial(value),
    //         "change_properties" => GraphEvents::ChangeProperties(value),
    //         "add_group" => GraphEvents::AddGroup(value),
    //         "remove_group" => GraphEvents::RemoveGroup(value),
    //         "rename_group" => GraphEvents::RenameGroup(value),
    //         "change_group" => GraphEvents::ChangeGroup(value),
    //         "add_inport" => GraphEvents::AddInport(value),
    //         "remove_inport" => GraphEvents::RemoveInport(value),
    //         "rename_inport" => GraphEvents::RenameInport(value),
    //         "change_inport" => GraphEvents::ChangeInport(value),
    //         "add_outport" => GraphEvents::AddOutport(value),
    //         "remove_outport" => GraphEvents::RemoveOutport(value),
    //         "rename_outport" => GraphEvents::RenameOutport(value),
    //         "change_outport" => GraphEvents::ChangeOutport(value),
    //         // "start_transaction" => GraphEvents::StartTransaction(value),
    //         // "end_transaction" => GraphEvents::EndTransaction(value),
    //         // "transaction" => GraphEvents::Transaction(value),
    //         _ => panic!("Unknown Graph event {}", name),
    //     }
    // }
}
