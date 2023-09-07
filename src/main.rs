use serde_json::json;
use zflow::{Graph, Network, NetworkOptions, BaseNetwork};

fn main() {
    let mut base_dir = std::env::current_dir().unwrap();
        base_dir.push("zflow_runtime/test_components");
        let base_dir = base_dir.to_str().unwrap();

        let mut graph = Graph::new("", false);
        graph
            .add_node("test/add", "add", None)
            .add_initial(json!(48), "test/add", "left", None)
            .add_initial(json!(2), "test/add", "right", None);

        let mut network = Network::create(
            graph.clone(),
            NetworkOptions {
                subscribe_graph: false,
                delay: true,
                base_dir: base_dir.to_string(),
                ..Default::default()
            },
        );

        if let Ok(nw) = network.connect().unwrap().try_lock().as_mut() {
            nw.start().unwrap();
        }
}
