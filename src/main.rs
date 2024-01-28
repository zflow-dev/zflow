use serde_json::json;
use zflow::{BaseNetwork, Graph, Network, NetworkOptions};

fn main() {
    std::env::set_var("ZFLOW_QUICKJS_RUNNER_LIB","zflow_js_runner/bin/libzflow_js_runner.dylib");
    let mut base_dir = std::env::current_dir().unwrap();
    base_dir.push("zflow_runtime/test_components");
    let base_dir = base_dir.to_str().unwrap();

    let mut graph = Graph::new("", false);
    graph
        .add_node("test/add", "add", None)
        .add_initial(json!(48), "test/add", "a", None)
        .add_initial(json!(2), "test/add", "b", None);

    let mut network = Network::create(
        graph.clone(),
        NetworkOptions {
            subscribe_graph: false,
            delay: true,
            workspace_dir: base_dir.to_owned(),
            ..Default::default()
        },
    );

    let nw = network.connect().unwrap();
    nw.start().unwrap();
}
