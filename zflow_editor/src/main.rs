use zflow_editor::editor::Editor;
use serde_json::json;
use zflow_editor::state::CURRENT_GRAPH;


// #[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Ok(graph) = CURRENT_GRAPH.lock().as_mut() {
        graph.add_node("add", "Add", None)
        .add_initial(json!(3), "add", "a", None)
        .add_initial(json!(2), "add", "b", None)
        .add_outport("result", "add", "result", None)
        .add_node("display", "Display", None)
        .add_inport("input", "display", "input", None)
        .add_edge("add", "result", "display", "input", None);
    }
    Editor::run();
}