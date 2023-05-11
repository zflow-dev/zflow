// use egui_node_graph::{Graph as EditorGraph, GraphEditorState, NodeId, DataTypeTrait};
// use serde_json::{Value, Map};
// use zflow::graph::{graph::Graph, types::GraphNode};
// use std::{borrow::Cow, collections::HashMap};

// #[derive(Default)]
// // #[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
// pub struct GraphUIState {
//     pub graph:Graph,
//     pub selected_node: Option<NodeId>
// }

// type Metadata = Map<String, Value>;

// impl DataTypeTrait<GraphUIState> for Value {
//     fn data_type_color(&self, _user_state: &mut GraphUIState) -> egui::Color32 {
//         match self {
//             Value::Null => egui::Color32::TRANSPARENT,
//             Value::Bool(v) => {
//                 if v {
//                     return egui::Color32::LIGHT_GREEN;
//                 }
//                 return egui::Color32::LIGHT_RED;
//             },
//             Value::Number(_) => egui::Color32::GOLD,
//             Value::String(_) => egui::Color32::YELLOW,
//             Value::Array(_) => egui::Color32::from_rgb(238, 207, 109),
//             Value::Object(_) => egui::Color32::BLUE,
//         }
//     }
//     fn name(&self) -> Cow<'_, str> {
//         match self {
//             Value::Null => Cow::Borrowed("Null"),
//             Value::Bool(v) => {
//                 if v {
//                     return Cow::Borrowed("True");
//                 }
//                 return Cow::Borrowed("False");
//             },
//             Value::Number(_) => Cow::Borrowed("Number"),
//             Value::String(_) => Cow::Borrowed("String"),
//             Value::Array(_) => Cow::Borrowed("Array"),
//             Value::Object(_) => Cow::Borrowed("Object"),
//         }
//     }
// }

// impl NodeTemplateTrait for GraphNode {
//     type NodeData = Metadata;
//     type DataType = Value;
//     type ValueType = Value;
//     type UserState = GraphUIState;

//     fn node_finder_label(&self, _user_state: &mut Self::UserState) -> Cow<'_, str> {
//         Cow::Borrowed(&self.id)
//     }

//     fn node_graph_label(&self, user_state: &mut Self::UserState) -> String {
//         self.node_finder_label(user_state).into()
//     }

//     fn user_data(&self, _user_state: &mut Self::UserState) -> Self::NodeData {
//         self.metadata
//     }

//     fn build_node(
//         &self,
//         graph: &mut EditorGraph<Self::NodeData, Self::DataType, Self::ValueType>,
//         user_state: &mut Self::UserState,
//         node_id: NodeId,
//     ) {
//         if let Some(inputs) = user_state.graph.inports.into_iter().find(|(_key, value)| value.process == self.id) {
//             graph.add_input_param(
//                 node_id,
//                 value.port,
//                 MyDataType::Scalar,
//                 Value::Null,
//                 InputParamKind::ConnectionOrConstant,
//                 true,
//             );
//         }

//     }

// }

// pub type EditorState = GraphEditorState<Metadata, Value, Value, GraphNode, GraphUIState>;

use std::{
    io,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};
use zflow::GraphNode;
use zflow::Graph;
use zflow::Journal;

pub struct EditorWidget<'a> {
    pub graph: &'a mut Graph,
    pub working_dir: String,
}

impl<'a> EditorWidget<'a> {
    pub fn search_nodes(&self, node_id: &str) -> Vec<&GraphNode> {
        let data = self
            .graph
            .nodes
            .iter()
            .filter(|node| (*node).id.contains(node_id));
        return Vec::from_iter(data);
    }
    pub fn undo(&mut self) {
        self.graph.undo();
    }
    pub fn redo(&mut self) {
        self.graph.undo();
    }
    pub fn save(&self) -> Result<(), io::Error> {
        let mut path = PathBuf::new()
            .join(Path::new(&self.working_dir))
            .join(Path::new(&self.graph.name));
        path.set_extension("json");
        self.graph
            .save(path.as_path().to_str().expect("expect file path"))
    }

    pub fn new_project(&mut self) {
        // Todo: open file dialog to select work directory and project name
        self.working_dir = "".to_string();
        *self.graph = Graph::new("", true);
        self.graph.start_journal(None);
    }

    pub fn import_graph(&mut self, path: &str) {
        if let Ok(graph) = Graph::load_file(path, None) {
            *self.graph = graph;
            self.graph.start_journal(None);
        }
    }

    pub fn run(&mut self, metadata: Option<Map<String, Value>>) {
        (*self.graph).start_journal(None);
    }
}
