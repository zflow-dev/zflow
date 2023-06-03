use std::{
    collections::HashMap,
    sync::{Arc, Mutex}, ops::Mul,
};

use crate::{
    connection::Connection,
    error::ZFlowGraphError,
    index_impls,
    node::{NodeData, NodeWidget},
    state::*,
    types::*, icons::IconLibrary,
};
use egui::{
    epaint::CubicBezierShape, CentralPanel, Color32, Context, Frame, Key, Painter, Pos2, Rect,
    Sense, Stroke, Ui, Vec2, Event, RawInput,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use slotmap::{SecondaryMap, SlotMap};
use std::collections::HashSet;
use zflow_graph::{Graph, Journal};
use zflow_runtime::{ip::IPType, port::PortOptions};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct InPortParams {
    pub node: NodeId,
    pub id: InputId,
    pub name: String,
    pub data: PortOptions,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct OutPortParams {
    pub node: NodeId,
    pub id: OutputId,
    pub name: String,
    pub data: PortOptions,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct GraphImpl {
    /// The [`Node`]s of the graph
    pub nodes: SlotMap<NodeId, NodeData>,
    /// The [`InputParam`]s of the graph
    pub inputs: SlotMap<InputId, (String, InPortParams)>,
    /// The [`OutputParam`]s of the graph
    pub outputs: SlotMap<OutputId, (String, OutPortParams)>,
    // Connects the input of a node, to the output of its predecessor that
    // produces it
    pub connections: SecondaryMap<InputId, OutputId>,
    pub icons: IconLibrary,
}

impl GraphImpl {
    pub fn from_graph(z_graph: Arc<Mutex<Graph>>) -> Self {
        let mut _nodes: SlotMap<NodeId, NodeData> = SlotMap::with_key();
        let mut _inputs: SlotMap<InputId, (String, InPortParams)> = SlotMap::with_key();
        let mut _outputs: SlotMap<OutputId, (String, OutPortParams)> = SlotMap::with_key();
        let mut _connections: SecondaryMap<InputId, OutputId> = SecondaryMap::new();
        let icons = IconLibrary::new();

        let mut ports: HashMap<String, AnyParameterId> = HashMap::new();
        if let Ok(_graph) = z_graph.lock() {
            for node in &_graph.nodes {
                let mut node_data = NodeData {
                    package_id: node.id.clone(),
                    name: node.component.clone(),
                    ..Default::default()
                };

                if let Some(meta) = &node.metadata {
                    if let Some(description) = meta.get("description") {
                        node_data.description = description.as_str().unwrap_or("").to_owned();
                    }
                    if let Some(icon) = meta.get("icon") {
                        node_data.icon = icon.as_str().unwrap_or("").to_owned();
                    }

                    if let Some(base_dir) = meta.get("base_dir") {
                        node_data.base_dir = base_dir.as_str().unwrap_or("").to_owned();
                    }

                    if let Some(source) = meta.get("source") {
                        node_data.source = source.as_str().unwrap_or("").to_owned();
                    }

                    if let Some(language) = meta.get("language") {
                        node_data.language = language.as_str().unwrap_or("").to_owned();
                    }

                    node_data.metadata = meta.clone();
                }
                _nodes.insert(node_data);
            }

            for (name, port) in _graph.inports.clone() {
                if let Some((node_id, node_data)) = _nodes
                    .iter_mut()
                    .find(|n| n.1.package_id == port.process)
                    .as_mut()
                {
                    let mut param = InPortParams {
                        data: PortOptions::default(),
                        node: *node_id,
                        name: port.port.clone(),
                        ..Default::default()
                    };

                    if let Some(meta) = port.metadata {
                        let value = json!(meta);
                        if let Ok(options) = PortOptions::deserialize(value) {
                            param.data = options;
                        }
                    }

                    let port_key = _inputs.insert((port.port.clone(), param));
                    _inputs[port_key.clone()].1.id = port_key.clone();

                    node_data
                        .inports
                        .push((port.port.clone(), port_key.clone()));

                    ports.insert(port.port.clone(), AnyParameterId::Input(port_key.clone()));
                }
            }

            // preload initial data for inports
            for iip in _graph.initializers.clone() {
                if let Some(to) = iip.to {
                    if let Some((node_id, node_data)) = _nodes
                        .iter_mut()
                        .find(|n| n.1.package_id == to.node_id)
                        .as_mut()
                    {
                        let mut param = InPortParams {
                            data: PortOptions::default(),
                            node: *node_id,
                            name: to.port.clone(),
                            ..Default::default()
                        };

                        if let Some(meta) = iip.metadata {
                            let value = json!(meta);
                            if let Ok(options) = PortOptions::deserialize(value) {
                                param.data = options;
                            }
                        }

                        if let Some(from) = iip.from {
                            param.data.data_type = IPType::Data(from.data);
                        }

                        let port_key = _inputs.insert((to.port.clone(), param));
                        _inputs[port_key.clone()].1.id = port_key.clone();

                        node_data.inports.push((to.port.clone(), port_key.clone()));

                        ports.insert(to.port.clone(), AnyParameterId::Input(port_key.clone()));
                    }
                }
            }

            for (name, port) in _graph.outports.clone() {
                if let Some((node_id, node_data)) = _nodes
                    .iter_mut()
                    .find(|n| n.1.package_id == port.process)
                    .as_mut()
                {
                    let params = OutPortParams {
                        data: PortOptions::default(),
                        node: *node_id,
                        name: port.port.clone(),
                        ..Default::default()
                    };
                    let port_key = _outputs.insert((port.port.clone(), params.clone()));

                    node_data
                        .outports
                        .push((port.port.clone(), port_key.clone()));

                    _outputs[port_key].1.id = port_key;

                    ports.insert(port.port.clone(), AnyParameterId::Output(port_key.clone()));
                }
            }

            for connection in &_graph.edges {
                let input_key = ports.get(&connection.to.port).expect("unknown inport");
                let output_key = ports.get(&connection.from.port).expect("unknown outport");
                _connections.insert(input_key.assume_input(), output_key.assume_output());
            }
        }

        Self {
            nodes: _nodes,
            inputs: _inputs,
            outputs: _outputs,
            connections: _connections,
            icons
        }
    }

    pub fn new() -> Self {
        Self {
            nodes: SlotMap::default(),
            inputs: SlotMap::default(),
            outputs: SlotMap::default(),
            connections: SecondaryMap::default(),
            icons: IconLibrary::default()
        }
    }

    pub fn add_node(
        &mut self,
        label: String,
        mut user_data: NodeData,
        f: impl FnOnce(&mut GraphImpl, NodeId),
    ) -> NodeId {
        let copy_data = user_data.clone();
        let node_id = self.nodes.insert_with_key(|id| {
            user_data.id = id;
            user_data
        });

        f(self, node_id);

        CURRENT_GRAPH
            .lock()
            .unwrap()
            .add_node(&label, &copy_data.name, Some(copy_data.metadata));
        node_id
    }

    pub fn add_input_param(&mut self, node_id: NodeId, name: String, data: PortOptions) -> InputId {
        let input_id = self.inputs.insert_with_key(|input_id| {
            (
                name.clone(),
                InPortParams {
                    node: node_id,
                    id: input_id,
                    name: name.clone(),
                    data: data.clone(),
                },
            )
        });
        self.nodes[node_id].inports.push((name.clone(), input_id));

        if let IPType::Data(v) = data.data_type {
            CURRENT_GRAPH.lock().unwrap().add_initial(
                v.clone(),
                &self.nodes[node_id].name,
                &name.clone(),
                None,
            );
        }

        input_id
    }

    pub fn replace_input_param_with_id(&mut self, id:InputId, node_id: NodeId, name: String, data: PortOptions) -> InputId {
        self.inputs[id.clone()].1.data = data.clone();
        let _graph = CURRENT_GRAPH.clone();
        let delay = std::time::Duration::from_millis(10);
        let node = self.nodes[node_id].name.clone();
        let port = name.clone();
        let debouncer = debounce::EventDebouncer::new(delay, move |data: IPType| {
            if let IPType::Data(v) = data {
                CURRENT_GRAPH.lock().unwrap().remove_initial(&node, &port);
                CURRENT_GRAPH.lock().unwrap().add_initial(
                    v.clone(),
                    &node,
                    &port,
                    None,
                );
            }
        });

        debouncer.put(data.data_type);
        
        id
    }

    pub fn remove_input_param(&mut self, param: InputId) {
        let node_id = self.inputs[param].1.node;
        // self.nodes[node_id].inports.retain(|(_, id)| *id != param);
        self.inputs.remove(param);
        self.connections.retain(|i, _| i != param);

        if let IPType::Data(_) = &self.inputs[param].1.data.data_type {
            CURRENT_GRAPH
                .lock()
                .unwrap()
                .remove_initial(&self.nodes[node_id].name, &self.inputs[param].0);
        }
    }

    pub fn remove_output_param(&mut self, param: OutputId) {
        // let node_id = self.outputs[param].1.node;
        // self.nodes[node_id].outports.retain(|(_, id)| *id != param);
        self.outputs.remove(param);
        self.connections.retain(|_, o| *o != param);
    }

    pub fn add_output_param(
        &mut self,
        node_id: NodeId,
        name: String,
        data: PortOptions,
    ) -> OutputId {
        let output_id = self.outputs.insert_with_key(|output_id| {
            (
                name.clone(),
                OutPortParams {
                    node: node_id,
                    id: output_id,
                    name: name.clone(),
                    data: data.clone(),
                },
            )
        });
        self.nodes[node_id].outports.push((name.clone(), output_id));

        output_id
    }

    /// Removes a node from the graph with given `node_id`. This also removes
    /// any incoming or outgoing connections from that node
    ///
    /// This function returns the list of connections that has been removed
    /// after deleting this node as input-output pairs. Note that one of the two
    /// ids in the pair (the one on `node_id`'s end) will be invalid after
    /// calling this function.
    pub fn remove_node(&mut self, node_id: NodeId) -> (NodeData, Vec<(InputId, OutputId)>) {
        let mut disconnect_events = vec![];

        self.connections.retain(|i, o| {
            if self.outputs[*o].1.node == node_id || self.inputs[i].1.node == node_id {
                disconnect_events.push((i, *o));
                false
            } else {
                true
            }
        });

        // NOTE: Collect is needed because we can't borrow the input ids while
        // we remove them inside the loop.
        for input in self[node_id].input_ids().collect::<SVec<_>>() {
            self.inputs.remove(input);
        }
        for output in self[node_id].output_ids().collect::<SVec<_>>() {
            self.outputs.remove(output);
        }
        let removed_node = self.nodes.remove(node_id).expect("Node should exist");

        (removed_node, disconnect_events)
    }

    pub fn remove_connection(&mut self, input_id: InputId) -> Option<OutputId> {
        self.connections.remove(input_id)
    }

    pub fn iter_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().map(|(id, _)| id)
    }

    pub fn add_connection(&mut self, output: OutputId, input: InputId) {
        self.connections.insert(input, output);
    }

    pub fn iter_connections(&self) -> impl Iterator<Item = (InputId, OutputId)> + '_ {
        self.connections.iter().map(|(o, i)| (o, *i))
    }

    pub fn connection(&self, input: InputId) -> Option<OutputId> {
        self.connections.get(input).copied()
    }

    pub fn any_param_type(&self, param: AnyParameterId) -> Result<&IPType, ZFlowGraphError> {
        match param {
            AnyParameterId::Input(input) => self.inputs.get(input).map(|x| &x.1.data.data_type),
            AnyParameterId::Output(output) => self.outputs.get(output).map(|x| &x.1.data.data_type),
        }
        .ok_or(ZFlowGraphError::InvalidParameterId(param))
    }

    pub fn get_input(&self, input: InputId) -> &(String, InPortParams) {
        &self.inputs[input]
    }

    pub fn try_get_input(&self, input: InputId) -> Option<&(String, InPortParams)> {
        self.inputs.get(input)
    }

    pub fn try_get_output(&self, output: OutputId) -> Option<&(String, OutPortParams)> {
        self.outputs.get(output)
    }

    pub fn get_output(&self, output: OutputId) -> &(String, OutPortParams) {
        &self.outputs[output]
    }
}

/// This trait can be implemented by any user type. The trait tells the library
/// how to enumerate the node templates it will present to the user as part of
/// the node finder.
pub trait NodeTemplateIter {
    type Item;
    fn all_kinds(&self) -> Vec<Self::Item>;
}

/// The return value of [`draw_graph_editor`]. This value can be used to make
/// user code react to specific events that happened when drawing the graph.
#[derive(Clone, Debug)]
pub struct GraphResponse {
    /// Events that occurred during this frame of rendering the graph. Check the
    /// [`UserResponse`] type for a description of each event.
    pub node_responses: Vec<NodeResponse>,
    /// Is the mouse currently hovering the graph editor? Note that the node
    /// finder is considered part of the graph editor, even when it floats
    /// outside the graph editor rect.
    pub cursor_in_editor: bool,
    /// Is the mouse currently hovering the node finder?
    pub cursor_in_finder: bool,
}

impl Default for GraphResponse {
    fn default() -> Self {
        Self {
            node_responses: Default::default(),
            cursor_in_editor: false,
            cursor_in_finder: false,
        }
    }
}

impl GraphEditorState {
    pub fn new(default_zoom: f32) -> Self {
        Self {
            pan_zoom: PanZoom {
                pan: egui::Vec2::ZERO,
                zoom: default_zoom,
            },
            ..Default::default()
        }
    }

    #[must_use]
    pub fn draw_graph_editor(
        &mut self,
        ui: &mut Ui,
        // all_kinds: impl NodeTemplateIter<Item = NodeTemplate>,
        // user_state: &mut UserState,
    ) -> GraphResponse {
        // This causes the graph editor to use as much free space as it can.
        // (so for windows it will use up to the resizeably set limit
        // and for a Panel it will fill it completely)
        let mut editor_rect = ui.max_rect();
        let resp = ui.allocate_rect(editor_rect, Sense::hover());

        let cursor_pos = ui
            .ctx()
            .input(|i| i.pointer.hover_pos().unwrap_or(Pos2::ZERO));
        let mut cursor_in_editor = resp.hovered();
        let mut cursor_in_finder = false;

        // Gets filled with the node metrics as they are drawn
        let mut port_locations = PortLocations::new();
        let mut node_rects = NodeRects::new();

        // The responses returned from node drawing have side effects that are best
        // executed at the end of this function.
        let mut delayed_responses: Vec<NodeResponse> = vec![];

        // Used to detect when the background was clicked
        let mut click_on_background = false;

        // Used to detect drag events in the background
        let mut drag_started_on_background = false;
        let mut drag_released_on_background = false;

        // debug_assert_eq!(
        //     self.node_order.iter().copied().collect::<HashSet<_>>(),
        //     self.graph.iter_nodes().collect::<HashSet<_>>(),
        //     "The node_order field of the GraphEditorself was left in an \
        // inconsistent self. It has either more or less values than the graph."
        // );

        // Allocate rect before the nodes, otherwise this will block the interaction
        // with the nodes.
        let r = ui.allocate_rect(ui.min_rect(), Sense::click().union(Sense::drag()));
        if r.clicked() {
            click_on_background = true;
        } else if r.drag_started() {
            drag_started_on_background = true;
        } else if r.drag_released() {
            drag_released_on_background = true;
        }

        /* Draw nodes */
        for node_id in self.node_order.iter().copied() {
            let responses = NodeWidget {
                position: self.node_positions.get_mut(node_id).unwrap(),
                graph: &mut self.graph,
                port_locations: &mut port_locations,
                node_rects: &mut node_rects,
                node_id,
                ongoing_drag: self.connection_in_progress,
                selected: self
                    .selected_nodes
                    .iter()
                    .any(|selected| *selected == node_id),
                pan: self.pan_zoom.pan + editor_rect.min.to_vec2(),
                zoom: self.pan_zoom.zoom,
                hide_attributes: self.hide_attributes,
            }
            .show(ui);

            // Actions executed later
            delayed_responses.extend(responses);
        }

        /* Draw the node finder, if open */
        // let mut should_close_node_finder = false;
        // if let Some(ref mut node_finder) = self.node_finder {
        //     let mut node_finder_area = Area::new("node_finder").order(Order::Foreground);
        //     if let Some(pos) = node_finder.position {
        //         node_finder_area = node_finder_area.current_pos(pos);
        //     }
        //     node_finder_area.show(ui.ctx(), |ui| {
        //         if let Some(node_kind) = node_finder.show(ui, all_kinds, user_state) {
        //             let new_node = self.graph.add_node(
        //                 node_kind.node_graph_label(user_state),
        //                 node_kind.user_data(user_state),
        //                 |graph, node_id| node_kind.build_node(graph, user_state, node_id),
        //             );
        //             self.node_positions.insert(
        //                 new_node,
        //                 cursor_pos - self.pan_zoom.pan - editor_rect.min.to_vec2(),
        //             );
        //             self.node_order.push(new_node);

        //             should_close_node_finder = true;
        //             delayed_responses.push(NodeResponse::CreatedNode(new_node));
        //         }
        //         let finder_rect = ui.min_rect();
        //         // If the cursor is not in the main editor, check if the cursor is in the finder
        //         // if the cursor is in the finder, then we can consider that also in the editor.
        //         if finder_rect.contains(cursor_pos) {
        //             cursor_in_editor = true;
        //             cursor_in_finder = true;
        //         }
        //     });
        // }
        // if should_close_node_finder {
        //     self.node_finder = None;
        // }

        /* Draw connections */
        if let Some((_, ref locator)) = self.connection_in_progress {
            let port_type = self.graph.any_param_type(*locator).unwrap();
            let connection_color = data_type_color(port_type);
            let start_pos = port_locations[locator];

            // Find a port to connect to
            fn snap_to_ports<Key: slotmap::Key + Into<AnyParameterId>, PortValue>(
                graph: &GraphImpl,
                port_type: &IPType,
                ports: &SlotMap<Key, PortValue>,
                port_locations: &PortLocations,
                cursor_pos: Pos2,
            ) -> Pos2 {
                ports
                    .iter()
                    .find_map(|(port_id, _)| {
                        let compatible_ports = graph
                            .any_param_type(port_id.into())
                            .map(|other| other == port_type)
                            .unwrap_or(false);

                        if compatible_ports {
                            port_locations.get(&port_id.into()).and_then(|port_pos| {
                                if port_pos.distance(cursor_pos) < DISTANCE_TO_CONNECT {
                                    Some(*port_pos)
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or(cursor_pos)
            }

            let (src_pos, dst_pos) = match locator {
                AnyParameterId::Output(_) => (
                    start_pos,
                    snap_to_ports(
                        &self.graph,
                        port_type,
                        &self.graph.inputs,
                        &port_locations,
                        cursor_pos,
                    ),
                ),
                AnyParameterId::Input(_) => (
                    snap_to_ports(
                        &self.graph,
                        port_type,
                        &self.graph.outputs,
                        &port_locations,
                        cursor_pos,
                    ),
                    start_pos,
                ),
            };
            draw_connection(ui.painter(), src_pos, dst_pos, connection_color);
        }

        for (input, output) in self.graph.iter_connections() {
            let port_type = self
                .graph
                .any_param_type(AnyParameterId::Output(output))
                .unwrap();
            let connection_color = data_type_color(port_type);
            let src_pos = port_locations[&AnyParameterId::Output(output)];
            let dst_pos = port_locations[&AnyParameterId::Input(input)];
            draw_connection(ui.painter(), src_pos, dst_pos, connection_color);
        }

        /* Handle responses from drawing nodes */

        // Some responses generate additional responses when processed. These
        // are stored here to report them back to the user.
        let mut extra_responses: Vec<NodeResponse> = Vec::new();

        for response in delayed_responses.iter() {
            match response {
                NodeResponse::ConnectEventStarted(node_id, port) => {
                    self.connection_in_progress = Some((*node_id, *port));
                    self.hide_attributes = false;
                }
                NodeResponse::ConnectEventEnded { input, output } => {
                    self.graph.add_connection(*output, *input);
                    self.hide_attributes = true;
                }
                NodeResponse::CreatedNode(_) => {
                    // Do something when node is added?
                }
                NodeResponse::SelectNode(node_id) => {
                    self.selected_nodes = Vec::from([*node_id]);
                    self.hide_attributes = false;
                }
                NodeResponse::DeleteNodeUi(node_id) => {
                    let (node, disc_events) = self.graph.remove_node(*node_id);
                    // Pass the disconnection responses first so user code can perform cleanup
                    // before node removal response.
                    extra_responses.extend(
                        disc_events
                            .into_iter()
                            .map(|(input, output)| NodeResponse::DisconnectEvent { input, output }),
                    );
                    // Pass the full node as a response so library users can
                    // listen for it and get their user data.
                    extra_responses.push(NodeResponse::DeleteNodeFull {
                        node_id: *node_id,
                        node,
                    });
                    self.node_positions.remove(*node_id);
                    // Make sure to not leave references to old nodes hanging
                    self.selected_nodes.retain(|id| *id != *node_id);
                    self.node_order.retain(|id| *id != *node_id);
                }
                NodeResponse::DisconnectEvent { input, output } => {
                    let other_node = self.graph.get_output(*output).1.node;
                    self.graph.remove_connection(*input);
                    self.connection_in_progress =
                        Some((other_node, AnyParameterId::Output(*output)));
                }
                NodeResponse::RaiseNode(node_id) => {
                    let old_pos = self
                        .node_order
                        .iter()
                        .position(|id| *id == *node_id)
                        .expect("Node to be raised should be in `node_order`");
                    self.node_order.remove(old_pos);
                    self.node_order.push(*node_id);
                }
                NodeResponse::MoveNode { node, drag_delta } => {
                    self.node_positions[*node] += *drag_delta;
                    // Handle multi-node selection movement
                    if self.selected_nodes.contains(node) && self.selected_nodes.len() > 1 {
                        for n in self.selected_nodes.iter().copied() {
                            if n != *node {
                                self.node_positions[n] += *drag_delta;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Handle box selection
        if let Some(box_start) = self.ongoing_box_selection {
            let selection_rect = Rect::from_two_pos(cursor_pos, box_start);
            let bg_color = Color32::from_rgba_unmultiplied(200, 200, 200, 20);
            let stroke_color = Color32::from_rgba_unmultiplied(200, 200, 200, 180);
            ui.painter().rect(
                selection_rect,
                2.0,
                bg_color,
                Stroke::new(3.0, stroke_color),
            );

            self.selected_nodes = node_rects
                .into_iter()
                .filter_map(|(node_id, rect)| {
                    if selection_rect.intersects(rect) {
                        Some(node_id)
                    } else {
                        None
                    }
                })
                .collect();
        }

        // Push any responses that were generated during response handling.
        // These are only informative for the end-user and need no special
        // treatment here.
        delayed_responses.extend(extra_responses);

        /* Mouse input handling */
        
        // This locks the context, so don't hold on to it for too long.
        let mouse = &ui.ctx().input(|i| i.pointer.clone());

        ui.ctx().input(|i| i.events.clone()).iter().for_each(|events| {
            match events {
                Event::Scroll(v) => {
                    self.pan_zoom.pan += v.clone();
                },
                Event::Zoom(zoom_delta) => {
                    self.pan_zoom.zoom = *zoom_delta;
                    // self.pan_zoom.adjust_zoom(*zoom_delta, resp.rect.size(), 0.5, 1.0);
                    
                    if *zoom_delta > 1.0 {
                        egui::gui_zoom::zoom_in(ui.ctx());
                    } else if *zoom_delta < 1.0 {
                        egui::gui_zoom::zoom_out(ui.ctx());
                    }
                }
                _=>{}
            }
        });

        

        if mouse.any_released() && self.connection_in_progress.is_some() {
            self.connection_in_progress = None;
        }

        if mouse.secondary_released() && cursor_in_editor && !cursor_in_finder {
            // self.node_finder = Some(NodeFinder::new_at(cursor_pos));
        }
        if ui.ctx().input(|i| i.key_pressed(Key::Escape)) {
            // self.node_finder = None;
        }

        if r.dragged() && ui.ctx().input(|i| i.pointer.middle_down()) {
            self.pan_zoom.pan += ui.ctx().input(|i| i.pointer.delta());
        }

    
        // Deselect and deactivate finder if the editor backround is clicked,
        // *or* if the the mouse clicks off the ui
        if click_on_background || (mouse.any_click() && !cursor_in_editor) {
            self.selected_nodes = Vec::new();
            self.hide_attributes = true;
            // self.node_finder = None;
        }

        if drag_started_on_background && mouse.primary_down() {
            self.ongoing_box_selection = Some(cursor_pos);
            self.hide_attributes = false;
        }
        if mouse.primary_released() || drag_released_on_background {
            self.ongoing_box_selection = None;
        }

        GraphResponse {
            node_responses: delayed_responses,
            cursor_in_editor,
            cursor_in_finder,
        }
    }
}

fn draw_connection(painter: &Painter, src_pos: Pos2, dst_pos: Pos2, color: Color32) {
    let connection_stroke = egui::Stroke { width: 2.0, color };

    let control_scale = ((dst_pos.x - src_pos.x) / 2.0).max(30.0);
    let src_control = src_pos + Vec2::X * control_scale;
    let dst_control = dst_pos - Vec2::X * control_scale;

    let bezier = CubicBezierShape::from_points_stroke(
        [src_pos, src_control, dst_control, dst_pos],
        false,
        Color32::TRANSPARENT,
        connection_stroke,
    );

    painter.add(bezier);
}
