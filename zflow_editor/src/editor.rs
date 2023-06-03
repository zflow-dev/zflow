use eframe::{run_native, App, CreationContext};
use egui::{Frame, Id, Pos2, Rect, Ui};
use serde_json::json;
use zflow_runtime::{ip::IPType, port::PortOptions};

use crate::{
    node::{NodeData, NodeWidget},
    state::{use_state, GraphEditorState, PanZoom},
};

const PERSISTENCE_KEY: &str = "zflow_node_graph_1";

pub struct EditorSettings {
    pub workspace: String,
}

#[derive(Default)]
pub struct Editor {
    state: GraphEditorState,
}

impl Editor {
    pub fn new(ctx: &CreationContext<'_>) -> Self {
        ctx.egui_ctx.set_pixels_per_point(1.4);
        // let state = ctx
        // .storage
        // .and_then(|storage| eframe::get_value(storage, PERSISTENCE_KEY))
        // .unwrap_or_default();
        Self {
            state: GraphEditorState::default(),
        }
    }
    pub fn run() {
        let options = eframe::NativeOptions {
            initial_window_size: Some(egui::vec2(1080.0, 640.0)),
            transparent: false,
            fullscreen: false,
            fullsize_content: true,
            decorated: true,
            vsync: true,
            hardware_acceleration: eframe::HardwareAcceleration::Preferred,
            ..Default::default()
        };
        let _ = run_native(
            "ZFlow Editor",
            options,
            Box::new(|cc| Box::new(Editor::new(cc))),
        );
    }
}

impl App for Editor {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PERSISTENCE_KEY, &self.state);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        catppuccin_egui::set_theme(ctx, catppuccin_egui::MOCHA);

        egui::CentralPanel::default().show(ctx, |ui| {
            let graph_response = self.state.draw_graph_editor(ui);

            // for node_response in graph_response.node_responses {
            // Here, we ignore all other graph events. But you may find
            // some use for them. For example, by playing a sound when a new
            // connection is created
            // if let NodeResponse::User(user_event) = node_response {
            //     match user_event {
            //         MyResponse::SetActiveNode(node) => self.user_state.active_node = Some(node),
            //         MyResponse::ClearActiveNode => self.user_state.active_node = None,
            //     }
            // }
            // }

            // if let Some(node) = self.user_state.active_node {
            //     if self.state.graph.nodes.contains_key(node) {
            //         let text = match evaluate_node(&self.state.graph, node, &mut HashMap::new()) {
            //             Ok(value) => format!("The result is: {:?}", value),
            //             Err(err) => format!("Execution error: {}", err),
            //         };
            //         ctx.debug_painter().text(
            //             egui::pos2(10.0, 35.0),
            //             egui::Align2::LEFT_TOP,
            //             text,
            //             TextStyle::Button.resolve(&ctx.style()),
            //             egui::Color32::WHITE,
            //         );
            //     } else {
            //         self.user_state.active_node = None;
            //     }
            // }
        });

        // egui::TopBottomPanel::top("ZFlow Editor")
        //     .show_separator_line(false)
        //     .frame(Frame {
        //         fill: catppuccin_egui::FRAPPE.base,
        //         ..Default::default()
        //     })
        //     .show(ctx, |ui| {
        //         ui.with_layout(
        //             egui::Layout::centered_and_justified(egui::Direction::RightToLeft),
        //             |ui| {
        //                 ui.set_height(32.0);
        //                 ui.label("ZFLow Editor");
        //             },
        //         );
        //     });
    }
}
