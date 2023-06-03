use std::{env, path::PathBuf};

use zflow_editor::editor::Editor;
use serde_json::{json, Map};
use zflow_editor::state::CURRENT_GRAPH;


// #[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Ok(graph) = CURRENT_GRAPH.lock().as_mut() {
       
        let mut dir = env::current_dir().expect("expected to init current dir");
        dir.push("zflow_editor");

        graph.add_node("add", "Add", Some(Map::from_iter([
            ("icon".to_owned(), {
               let mut icon_dir =  dir.clone();
                icon_dir.push("icons/add.svg");
                json!(icon_dir.as_os_str().to_str().unwrap())
            })
        ])))
        .add_initial(json!(3), "add", "a", None)
        .add_initial(json!(2), "add", "b", None)
        .add_outport("result", "add", "result", None)
        .add_node("display", "Display", Some(Map::from_iter([
            ("icon".to_owned(), {
                let mut icon_dir =  dir.clone();
                icon_dir.push("icons/display.svg");
                json!(icon_dir.as_os_str().to_str().unwrap())
            })
        ])))
        .add_inport("input", "display", "input", None)
        .add_edge("add", "result", "display", "input", None);
    }
    Editor::run();
}