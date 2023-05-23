use std::collections::HashMap;
use std::sync::{Arc, Mutex};
/// MIT License
///
/// Copyright (c) 2022 setzer22
/// Copyright (c) 2023 Damilare Akinlaja
use std::{cell::Cell, rc::Rc};

use egui::Vec2;
use egui::{Pos2, Rect};
use serde::{Deserialize, Serialize};
use slotmap::{SecondaryMap};
use zflow_graph::Graph;

use crate::graph::GraphImpl;
use crate::node::NodeData;
use crate::types::{AnyParameterId, InputId, OutputId};


pub const DISTANCE_TO_CONNECT: f32 = 10.0;

lazy_static! {
    pub static ref CURRENT_GRAPH: Arc<Mutex<Graph>> =
        Arc::new(Mutex::new(Graph::new("zflow graph", false)));
}

slotmap::new_key_type! { pub struct NodeId; }

pub type PortLocations = HashMap<AnyParameterId, Pos2>;
pub type NodeRects = HashMap<NodeId, Rect>;

pub fn use_state<T: Copy>(value: T) -> (impl Fn() -> T, impl FnMut(T)) {
    let val = Rc::new(Cell::new(value));

    let state = {
        let val = Rc::clone(&val);
        move || -> T { val.get() }
    };

    let set_state = move |v: T| {
        val.set(v);
    };
    (state, set_state)
}

#[derive(Default, Copy, Clone, Serialize, Deserialize)]
pub struct PanZoom {
    pub pan: egui::Vec2,
    pub zoom: f32,
}

impl PanZoom {
    pub fn adjust_zoom(
        &mut self,
        zoom_delta: f32,
        point: egui::Vec2,
        zoom_min: f32,
        zoom_max: f32,
    ) {
        let zoom_clamped = (self.zoom + zoom_delta).clamp(zoom_min, zoom_max);
        let zoom_delta = zoom_clamped - self.zoom;

        self.zoom += zoom_delta;
        self.pan += point * zoom_delta;
    }
}


#[derive(Clone, Serialize, Deserialize)]
pub struct GraphEditorState {
    pub graph: GraphImpl,
    /// Nodes are drawn in this order. Draw order is important because nodes
    /// that are drawn last are on top.
    pub node_order: Vec<NodeId>,
    /// An ongoing connection interaction: The mouse has dragged away from a
    /// port and the user is holding the click
    pub connection_in_progress: Option<(NodeId, AnyParameterId)>,
    /// The currently selected node. Some interface actions depend on the
    /// currently selected node.
    pub selected_nodes: Vec<NodeId>,
    /// The mouse drag start position for an ongoing box selection.
    pub ongoing_box_selection: Option<egui::Pos2>,
    /// The position of each node.
    pub node_positions: SecondaryMap<NodeId, egui::Pos2>,
    /// The node finder is used to create new nodes.
    // pub node_finder: Option<NodeFinder<NodeTemplate>>,
    /// The panning of the graph viewport.
    pub pan_zoom: PanZoom,
}

impl Default for GraphEditorState {
    fn default() -> Self {
        let _g = GraphImpl::from_graph(CURRENT_GRAPH.clone());
        let mut node_pos:SecondaryMap<NodeId, egui::Pos2> = SecondaryMap::new();

        _g.nodes.iter().enumerate().for_each(|(i, (id, _))|{
            let offset_y = (100 * i) as f32;
            let offset_x = DISTANCE_TO_CONNECT * i as f32 * 25_f32;
            node_pos.insert(id,  Pos2 { x:  100.0 +offset_x, y: 100.0 + offset_y});
        });
        
        Self {
            graph: _g.clone(),
            node_order: _g.nodes.iter().map(|(id, _)| id).collect(),
            connection_in_progress: Default::default(),
            selected_nodes: Default::default(),
            ongoing_box_selection: Default::default(),
            node_positions: node_pos,
            // node_finder: Default::default(),
            pan_zoom: PanZoom {
                pan: egui::Vec2::ZERO,
                zoom: 0.1,
            }
            // _user_state: Default::default(),
        }
    }
}

/// Nodes communicate certain events to the parent graph when drawn. There is
/// one special `User` variant which can be used by users as the return value
/// when executing some custom actions in the UI of the node.
#[derive(Clone, Debug)]
pub enum NodeResponse {
    ConnectEventStarted(NodeId, AnyParameterId),
    ConnectEventEnded {
        output: OutputId,
        input: InputId,
    },
    CreatedNode(NodeId),
    SelectNode(NodeId),
    /// As a user of this library, prefer listening for `DeleteNodeFull` which
    /// will also contain the user data for the deleted node.
    DeleteNodeUi(NodeId),
    /// Emitted when a node is deleted. The node will no longer exist in the
    /// graph after this response is returned from the draw function, but its
    /// contents are passed along with the event.
    DeleteNodeFull {
        node_id: NodeId,
        node: NodeData,
    },
    DisconnectEvent {
        output: OutputId,
        input: InputId,
    },
    /// Emitted when a node is interacted with, and should be raised
    RaiseNode(NodeId),
    MoveNode {
        node: NodeId,
        drag_delta: Vec2,
    },
    // User(UserResponse),
}
