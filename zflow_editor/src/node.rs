use std::{cell::Cell, collections::HashMap, rc::Rc, sync::Arc};

use egui::{epaint::RectShape, *};
use egui_extras::{image::FitTo, RetainedImage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use zflow_graph::types::GraphNode;
use zflow_runtime::ip::IPType;

use crate::{
    error::ZFlowGraphError,
    graph::{GraphImpl, InPortParams, OutPortParams},
    icons::IconLibrary,
    state::{NodeId, NodeResponse, DISTANCE_TO_CONNECT},
    types::{data_type_color, AnyParameterId, InputId, OutputId},
};

pub type PortLocations = HashMap<AnyParameterId, Pos2>;
pub type NodeRects = HashMap<NodeId, Rect>;

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct NodeData {
    pub id: NodeId,
    pub name: String,
    pub inports: Vec<(String, InputId)>,
    pub outports: Vec<(String, OutputId)>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub base_dir: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub package_id: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl NodeData {
    pub fn to_graph_node() -> GraphNode {
        // Todo: convert to graph node
        GraphNode::default()
    }

    pub fn inputs<'a>(
        &'a self,
        graph: &'a GraphImpl,
    ) -> impl Iterator<Item = &(String, InPortParams)> + 'a {
        self.input_ids().map(|id| graph.get_input(id))
    }

    pub fn outputs<'a>(
        &'a self,
        graph: &'a GraphImpl,
    ) -> impl Iterator<Item = &(String, OutPortParams)> + 'a {
        self.output_ids().map(|id| graph.get_output(id))
    }

    pub fn input_ids(&self) -> impl Iterator<Item = InputId> + '_ {
        self.inports.iter().map(|(_name, id)| *id)
    }

    pub fn output_ids(&self) -> impl Iterator<Item = OutputId> + '_ {
        self.outports.iter().map(|(_name, id)| *id)
    }

    pub fn get_input(&self, name: &str) -> Result<InputId, ZFlowGraphError> {
        self.inports
            .iter()
            .find(|(param_name, _id)| param_name == name)
            .map(|x| x.1)
            .ok_or_else(|| ZFlowGraphError::NoParameterNamed(self.id, name.into()))
    }

    pub fn get_output(&self, name: &str) -> Result<OutputId, ZFlowGraphError> {
        self.outports
            .iter()
            .find(|(param_name, _id)| param_name == name)
            .map(|x| x.1)
            .ok_or_else(|| ZFlowGraphError::NoParameterNamed(self.id, name.into()))
    }
}

#[derive(Clone, Copy, Debug)]
struct OuterRectMemory(Rect);

pub struct NodeWidget<'a> {
    pub node_id: NodeId,
    pub graph: &'a mut GraphImpl,
    pub position: &'a mut Pos2,
    pub pan: egui::Vec2,
    pub zoom: f32,
    pub node_rects: &'a mut NodeRects,
    pub port_locations: &'a mut PortLocations,
    pub ongoing_drag: Option<(NodeId, AnyParameterId)>,
    pub selected: bool,
    pub hide_attributes: bool,
}

impl<'a> NodeWidget<'a> {
    pub const MAX_NODE_SIZE: [f32; 2] = [200.0, 200.0];

    pub fn show(self, ui: &mut Ui) -> Vec<NodeResponse> {
        let mut child_ui = ui.child_ui_with_id_source(
            Rect::from_min_size(*self.position + self.pan, Vec2::new(125.0, 125.0)),
            Layout::default(),
            self.node_id,
        );

        Self::show_graph_node(self, &mut child_ui)
    }

    fn show_graph_node(self, ui: &mut Ui) -> Vec<NodeResponse> {
        let mut responses = Vec::<NodeResponse>::new();

        let node_id = self.node_id;

        let outer_rect_bounds = ui.available_rect_before_wrap();
        let margin = egui::vec2(5.0, 5.0);
        let mut inner_rect = outer_rect_bounds.shrink2(margin);

        // Make sure we don't shrink to the negative:
        inner_rect.max.x = inner_rect.max.x.max(inner_rect.min.x);
        inner_rect.max.y = inner_rect.max.y.max(inner_rect.min.y);
        let mut child_ui = ui.child_ui(inner_rect, *ui.layout());

        let mut interaction_rect = ui
            .ctx()
            .memory_mut(|mem| {
                mem.data
                    .get_temp::<OuterRectMemory>(child_ui.id())
                    .map(|stored| stored.0)
            })
            .unwrap_or(outer_rect_bounds);

        let mut title_height = 0.0;

        let outline_shape = ui.painter().add(Shape::Noop);
        let background_shape = ui.painter().add(Shape::Noop);

        let mut input_port_heights = vec![];
        let mut output_port_heights = vec![];

        let window_response = ui.interact(
            interaction_rect,
            Id::new(node_id.clone()),
            Sense::click_and_drag(),
        );

        let _node_id = node_id.clone();
        // parent.memory_mut(|mem|{
        //     mem.data.insert_persisted(Id::new((_node_id, "pos2")), self.position + drag_delta);
        // });

        child_ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.with_layout(
                    egui::Layout::centered_and_justified(egui::Direction::RightToLeft),
                    |ui| {
                        ui.add(Label::new(
                            RichText::new(self.graph[node_id].name.clone())
                                .text_style(TextStyle::Button),
                        ));
                    },
                );

                // responses.extend(
                //     self.graph[self.node_id]
                //         .user_data
                //         .top_bar_ui(ui, self.node_id, self.graph, user_state)
                //         .into_iter(),
                // );
                // ui.add_space(10.0); // The size of the little cross icon
            });

            ui.add_space(margin.y);
            title_height = ui.min_size().y + 5.0;

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let size = self.graph[node_id].inports.len();
                    fn draw_inport(
                        ui: &mut egui::Ui,
                        graph: &mut GraphImpl,
                        node_id: NodeId,
                        id: InputId,
                        name: &str,
                        options: &mut InPortParams,
                        hide_attribute: bool,
                    ) {
                        ui.horizontal(|ui| {
                            if !hide_attribute {
                                ui.label(name.clone());

                                if let IPType::Data(value) = &options.clone().data.data_type {
                                    let mut new_value = value.clone();
                                    if let Some(ref mut num) = value.as_f64() {
                                        ui.add(DragValue::new(num));
                                        new_value = json!(num);
                                    }
                                    if let Some(ref mut v) = value.as_str() {
                                        ui.add_enabled(true, TextEdit::singleline(v));
                                        new_value = json!(v);
                                    }

                                    if let Some(ref mut selected) = value.as_bool() {
                                        ComboBox::from_label("Select one!")
                                            .selected_text(format!("{:?}", selected))
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(selected, true, "true");
                                                ui.selectable_value(selected, false, "false");
                                            });
                                        new_value = json!(selected);
                                    }

                                    options.data.data_type = IPType::Data(new_value);

                                    graph.replace_input_param_with_id(
                                        id.clone(),
                                        node_id,
                                        name.to_owned(),
                                        options.data.clone(),
                                    );
                                }
                            } else {
                                ui.label("");
                            }
                        });
                    }
                    if size > 1 {
                        for (input, id) in self.graph[node_id].inports.clone().iter() {
                            let (_, options) = &mut self.graph.inputs[*id].clone();
                            let height_before = ui.min_rect().bottom();
                            ui.with_layout(
                                Layout::centered_and_justified(Direction::LeftToRight),
                                |ui| {
                                    draw_inport(
                                        ui,
                                        self.graph,
                                        node_id,
                                        id.clone(),
                                        input,
                                        options,
                                        self.hide_attributes,
                                    );
                                },
                            );
                            let height_after = ui.min_rect().bottom();
                            input_port_heights.push((height_before + height_after) / 2.0);
                        }
                    } else {
                        if let Some((input, id)) = self.graph[node_id].clone().inports.first() {
                            let (_, options) = &mut self.graph.inputs[*id].clone();
                            let height_before = ui.min_rect().bottom();
                            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                                draw_inport(
                                    ui,
                                    self.graph,
                                    node_id,
                                    id.clone(),
                                    input,
                                    options,
                                    self.hide_attributes,
                                );
                            });
                            let height_after = ui.min_rect().bottom();
                            input_port_heights.push((height_before + height_after) / 2.0);
                        }
                    }
                });
                ui.vertical(|ui| {
                    let size = self.graph[node_id].outports.len();
                    if size > 1 {
                        for (output, id) in self.graph[node_id].outports.iter() {
                            let (_, options) = &self.graph.outputs[*id];
                            let height_before = ui.min_rect().bottom();

                            ui.horizontal(|ui| {
                                if !self.hide_attributes {
                                    ui.label(output);
                                } else {
                                    ui.label("");
                                }
                            });
                            let height_after = ui.min_rect().bottom();
                            output_port_heights.push((height_before + height_after) / 2.0);
                        }
                    } else {
                        if let Some((output, id)) = self.graph[node_id].outports.first() {
                            let height_before = ui.min_rect().bottom();
                            ui.with_layout(
                                Layout::with_main_align(
                                    Layout::centered_and_justified(egui::Direction::RightToLeft),
                                    Align::Center,
                                ),
                                |ui| {
                                    // let (_, options) = &self.graph.outputs[*id];
                                    ui.horizontal(|ui| {
                                        if !self.hide_attributes {
                                            ui.label(output);
                                        } else {
                                            ui.label("");
                                        }
                                    });
                                },
                            );
                            let height_after = ui.min_rect().bottom();
                            output_port_heights.push((height_before + height_after) / 2.0);
                        }
                    }
                });
            });
        });

        if !self.selected || self.hide_attributes {
            let mut icon_ui = ui.child_ui(child_ui.min_rect(), *child_ui.layout());
            icon_ui.with_layout(Layout::centered_and_justified(Direction::TopDown), |ui| {
                if let Some(icon_data) = self.graph.icons.get(&node_id) {
                    if let Ok(icon) = RetainedImage::from_svg_bytes_with_size(
                        format!("{:?}", node_id),
                        icon_data.to_vec().as_slice(),
                        FitTo::Zoom(self.zoom),
                    ) {
                        ui.add_space(title_height);
                        icon.show(ui);
                        ui.add_space(title_height);
                    }
                }
            });
            child_ui.add_space(10.0);
        }

        // Second pass, iterate again to draw the ports. This happens outside
        // the child_ui because we want ports to overflow the node background.

        let outer_rect = child_ui.min_rect().expand2(margin);

        let port_left = outer_rect.left();
        let port_right = outer_rect.right();

        // Save expanded rect to memory.
        ui.ctx().memory_mut(|mem| {
            mem.data
                .insert_temp(child_ui.id(), OuterRectMemory(outer_rect))
        });

        #[allow(clippy::too_many_arguments)]
        fn draw_port(
            ui: &mut Ui,
            graph: &GraphImpl,
            node_id: NodeId,
            port_pos: Pos2,
            responses: &mut Vec<NodeResponse>,
            param_id: AnyParameterId,
            port_locations: &mut PortLocations,
            ongoing_drag: Option<(NodeId, AnyParameterId)>,
            is_connected_input: bool,
        ) {
            let port_type = graph.any_param_type(param_id).unwrap();

            let port_rect = Rect::from_center_size(port_pos, egui::vec2(5.0, 5.0));

            let sense = if ongoing_drag.is_some() {
                Sense::hover()
            } else {
                Sense::click_and_drag()
            };

            let resp = ui.allocate_rect(port_rect, sense);

            // Check if the distance between the port and the mouse is the distance to connect
            let close_enough = if let Some(pointer_pos) = ui.ctx().pointer_hover_pos() {
                port_rect.center().distance(pointer_pos) < DISTANCE_TO_CONNECT
            } else {
                false
            };

            let port_color = if close_enough {
                Color32::BLUE
            } else {
                data_type_color(port_type)
            };

            ui.painter()
                .circle(port_rect.center(), 3.0, port_color, Stroke::NONE);

            if resp.drag_started() {
                if is_connected_input {
                    let input = param_id.assume_input();
                    let corresp_output = graph
                        .connection(input)
                        .expect("Connection data should be valid");
                    responses.push(NodeResponse::DisconnectEvent {
                        input: param_id.assume_input(),
                        output: corresp_output,
                    });
                } else {
                    responses.push(NodeResponse::ConnectEventStarted(node_id, param_id));
                }
            }
            if let Some((origin_node, origin_param)) = ongoing_drag {
                if origin_node != node_id {
                    // Don't allow self-loops
                    if graph.any_param_type(origin_param).unwrap() == port_type
                        && close_enough
                        && ui.input(|i| i.pointer.any_released())
                    {
                        match (param_id, origin_param) {
                            (AnyParameterId::Input(input), AnyParameterId::Output(output))
                            | (AnyParameterId::Output(output), AnyParameterId::Input(input)) => {
                                responses.push(NodeResponse::ConnectEventEnded { input, output });
                            }
                            _ => { /* Ignore in-in or out-out connections */ }
                        }
                    }
                }
            }

            port_locations.insert(param_id, port_rect.center());
        }

        // Input ports
        for ((_, param), port_height) in self.graph[self.node_id]
            .inports
            .iter()
            .zip(input_port_heights.into_iter())
        {
            // let should_draw = match self.graph[*param].kind() {
            //     InputParamKind::ConnectionOnly => true,
            //     InputParamKind::ConstantOnly => false,
            //     InputParamKind::ConnectionOrConstant => true,
            // };

            let pos_left = pos2(port_left, port_height);
            draw_port(
                ui,
                self.graph,
                self.node_id,
                pos_left,
                &mut responses,
                AnyParameterId::Input(*param),
                self.port_locations,
                self.ongoing_drag,
                self.graph.connection(*param).is_some(),
            );
        }

        // Output ports
        for ((_, param), port_height) in self.graph[self.node_id]
            .outports
            .iter()
            .zip(output_port_heights.into_iter())
        {
            let pos_right = pos2(port_right, port_height);
            draw_port(
                ui,
                self.graph,
                self.node_id,
                pos_right,
                &mut responses,
                AnyParameterId::Output(*param),
                self.port_locations,
                self.ongoing_drag,
                false,
            );
        }

        // Draw the background shape.
        // NOTE: This code is a bit more involved than it needs to be because egui
        // does not support drawing rectangles with asymmetrical round corners.

        let (shape, outline) = {
            let rounding_radius = 10.0;
            let rounding = Rounding::same(rounding_radius);

            let titlebar_height = title_height + margin.y;
            let titlebar_rect =
                Rect::from_min_size(outer_rect.min, vec2(outer_rect.width(), titlebar_height));
            let titlebar = Shape::Rect(RectShape {
                rect: titlebar_rect,
                rounding,
                fill: catppuccin_egui::MOCHA.overlay0.gamma_multiply(0.8),
                stroke: Stroke::NONE,
            });

            let body_rect = Rect::from_min_size(
                outer_rect.min + vec2(0.0, titlebar_height - rounding_radius),
                vec2(outer_rect.width(), outer_rect.height() - titlebar_height),
            );
            let body = Shape::Rect(RectShape {
                rect: body_rect,
                rounding: Rounding::none(),
                fill: catppuccin_egui::MOCHA.overlay0,
                stroke: Stroke::NONE,
            });

            let bottom_body_rect = Rect::from_min_size(
                body_rect.min + vec2(0.0, body_rect.height() - titlebar_height * 0.5),
                vec2(outer_rect.width(), titlebar_height),
            );
            let bottom_body = Shape::Rect(RectShape {
                rect: bottom_body_rect,
                rounding,
                fill: catppuccin_egui::MOCHA.overlay0,
                stroke: Stroke::NONE,
            });

            let node_rect = titlebar_rect.union(body_rect).union(bottom_body_rect);
            let outline = if self.selected {
                Shape::Rect(RectShape {
                    rect: node_rect.expand(1.0),
                    rounding,
                    fill: Color32::WHITE.gamma_multiply(0.8),
                    stroke: Stroke::NONE,
                })
            } else {
                Shape::Noop
            };

            // Take note of the node rect, so the editor can use it later to compute intersections.
            self.node_rects.insert(self.node_id, node_rect);

            (Shape::Vec(vec![titlebar, body, bottom_body]), outline)
        };

        ui.painter().set(background_shape, shape);
        ui.painter().set(outline_shape, outline);

        // --- Interaction ---

        // Titlebar buttons
        // if Self::close_button(ui, outer_rect).clicked() {
        //     responses.push(NodeResponse::DeleteNodeUi(self.node_id));
        // };

        // Movement
        let drag_delta = window_response.drag_delta();
        if drag_delta.length_sq() > 0.0 {
            responses.push(NodeResponse::MoveNode {
                node: self.node_id,
                drag_delta,
            });
            responses.push(NodeResponse::RaiseNode(self.node_id));
        }

        // Node selection
        //
        // HACK: Only set the select response when no other response is active.
        // This prevents some issues.
        if responses.is_empty() && window_response.clicked_by(PointerButton::Primary) {
            responses.push(NodeResponse::SelectNode(self.node_id));
            responses.push(NodeResponse::RaiseNode(self.node_id));
        }

        responses
    }

    fn close_button(ui: &mut Ui, node_rect: Rect) -> Response {
        // Measurements
        let margin = 8.0;
        let size = 10.0;
        let stroke_width = 2.0;
        let offs = margin + size / 2.0;

        let position = pos2(node_rect.right() - offs, node_rect.top() + offs);
        let rect = Rect::from_center_size(position, vec2(size, size));
        let resp = ui.allocate_rect(rect, Sense::click());

        let color = if resp.clicked() {
            // if dark_mode {
            //     color_from_hex("#ffffff").unwrap()
            // } else {
            //     color_from_hex("#000000").unwrap()
            // }
        } else if resp.hovered() {
            // if dark_mode {
            //     color_from_hex("#dddddd").unwrap()
            // } else {
            //     color_from_hex("#222222").unwrap()
            // }
        } else {
            // #[allow(clippy::collapsible_else_if)]
            // if dark_mode {
            //     color_from_hex("#aaaaaa").unwrap()
            // } else {
            //     color_from_hex("#555555").unwrap()
            // }
        };
        let stroke = Stroke {
            width: stroke_width,
            color: catppuccin_egui::MOCHA.overlay2, // Todo: replace with theme colors
        };

        ui.painter()
            .line_segment([rect.left_top(), rect.right_bottom()], stroke);
        ui.painter()
            .line_segment([rect.right_top(), rect.left_bottom()], stroke);

        resp
    }
}
