<!-- ![Build](https://github.com/darmie/zflow/actions/workflows/build.yml/badge.svg) -->
![zflow_graph](https://github.com/darmie/zflow/actions/workflows/graph.yml/badge.svg) 
![zflow_untime](https://github.com/darmie/zflow/actions/workflows/runtime.yml/badge.svg)
![FBP Spec](https://github.com/darmie/zflow/actions/workflows/fbp.yml/badge.svg)

# ZFlow - Flow-Based Programming Library

This library provides a Rust implementation of a [Flow-Based Programming graphs](https://flow-based.org/) and a runtime for executing the graph. There are two areas covered:

* [Graph](https://github.com/darmie/zflow/blob/main/zflow_graph) - the actual graph library
* [Journal trait](https://github.com/darmie/zflow/blob/main/zflow_graph/src/journal.rs) - journal system for keeping track of graph changes and undo history
* [Graph Runtime](https://github.com/darmie/zflow/blob/main/zflow_runtime) - the process runtime to execute graph components

## Graph Usage 
```rust
let mut g = Graph::new("Foo bar", true);
// listen to the graph add_node event
g.connect("add_node", |this, data|{
    if let Ok(node) = GraphNode::deserialize(data){
        assert_eq!(node.id, "Foo");
        assert_eq!(node.component, "Bar");
    }
}, true);
// add a node
g.add_node("Foo", "Bar", None);

// listen to the add_edge event
g.connect("add_edge", |this, data|{
    if let Ok(edge) = GraphEdge::deserialize(data){
        assert_eq!(edge.from.node_id, "Foo");
        assert_eq!(edge.to.port, "In");
    }
});

// add node with ID `Foo` and Component named `foo`
g.add_node("Foo", "foo", None);
// add node with ID `Bar` and Component named `bar`
g.add_node("Bar", "bar", None);
// add a connection between `Foo` and `Bar` by their output port and input ports respectively.
g.add_edge("Foo", "Out", "Bar", "In", None);
```
See [graph_test.rs](https://github.com/darmie/zflow/blob/main/zflow_graph/src/graph_test.rs) for more usage examples

## Journal Usage
```rs
let mut graph = Graph::new("", false);
// start recording events in the graph to the memory journal
graph.start_journal(None);
graph.add_node("Foo", "Bar", None);
graph.add_node("Baz", "Foo", None);
graph.add_edge("Foo", "out", "Baz", "in", None);
graph.add_initial(json!(42), "Foo", "in", None);
graph.remove_node("Foo");

// move to initial state in journal history
graph.move_to_revision(0);
// move to second revision in journal history
graph.move_to_revision(2);
// move to fifth revision in journal history
graph.move_to_revision(5);
```
See [journal.rs](https://github.com/darmie/zflow/blob/main/zflow_graph/src/journal.rs#L1013) for more usage examples

## Graph Runtime example 
Some details may be hidden for brevity
```rs
// Create a component
let mut my_component = Component::new(ComponentOptions {
    forward_brackets:HashMap::new(),
    // set input port `in`
    in_ports: HashMap::from([(
        "in".to_string(),
        InPort::default(),
    )]),
    // set output port `out`
    out_ports: HashMap::from([(
        "out".to_string(),
        OutPort::default(),
    )]),
    process: Box::new(move |this| {
        if let Ok(handle) = this.try_lock().as_mut() {
            // get something from input port
            if let Some(input_data) = handle.input().get("in") {
                // <do stuff>
            }
            // send output
            handle.output().send(&("out", json!("Hello World!")));
        }
        Ok(ProcessResult::default())
    }),
    ..ComponentOptions::default()
});
```

Connect another component to `my_component` and run the network

```rs
// another component
let mut second_component = Component::new(ComponentOptions {...});

// setup the fbp graph
let mut graph = Graph::new("", false);
g.add_node("first_component", "first_component_process", None)
 .add_node("second_component", "second_component_process", None)
  // trigger the first component with an initial packet
 .add_initial(json!("start"), "first_component", "in", None)
  // send the output of `first_component` to the input of the `second_component`
 .add_edge("first_component", "out", "second_component", "in", None);

// create a network to run this graph
let mut network = Network::create(graph.clone(), NetworkOptions {
    subscribe_graph: false,
    delay: true,
    base_dir: "/".to_string(),
    ..Default::default()
});

// register the components to the node ids
let loader = network.get_loader();
loader.register_component("first_component", "first_component_process", my_component).unwrap();
loader.register_component("second_component", "second_component_process", second_component).unwrap();

// sync graph with network
if let Ok(nw) = network.connect().unwrap().try_lock() {
    // start the network
    nw.start().unwrap();
}
```

See [full example](https://github.com/darmie/zflow/blob/main/zflow_runtime/src/component_test.rs#L396) that demonstrates generation of an HTML code using ZFlow.
