![Runtime](https://github.com/darmie/zflow/actions/workflows/runtime.yml/badge.svg)

# ZFlow Process Runtime

The main runtime for the execution of the directed acyclic graph

## Runtime example

Some details may be hidden for brevity

A simple component

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
if let Ok(nw) = network.connect().try_lock() {
    // start the network
    nw.start().unwrap();
}
```

See [full example](https://github.com/darmie/zflow/blob/main/zflow_runtime/src/component_test.rs#L396) that demonstrates generation of an HTML code using ZFlow.
