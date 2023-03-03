# ZFlow - FBP Graph Library for Rust

This library provides a Rust implementation of [Flow-Based Programming graphs](https://flow-based.org/). There are two areas covered:

* [Graph](https://github.com/darmie/zflow/blob/main/src/graph/graph.rs) - the actual graph library
* [Journal trait](https://github.com/darmie/zflow/blob/main/src/graph/journal.rs) - journal system for keeping track of graph changes and undo history
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
See [graph_test.rs](https://github.com/darmie/zflow/blob/main/src/graph/graph_test.rs) for more usage examples

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
See [journal.rs](https://github.com/darmie/zflow/blob/main/src/graph/journal.rs#L1013) for more usage examples

## Graph Runtime example 
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
    process: Box::new(move |context, input, output| {
        // get something from input port
        if let Some(input_data) = input.get("out") {
            // <do stuff>
        }
        // send output
        output.send(&("out", json!("Hello World!")));
        Ok(ProcessResult::default())
    }),
    ..ComponentOptions::default()
});
```

Connect another component to `my_component`

```rs
let mut trigger = InternalSocket::create(None);
let mut output_connector = InternalSocket::create(None);
my_component...get_inports_mut().ports.get_mut("in").map(|v| v.attach(trigger.clone(), None));
my_component...get_outports_mut().ports.get_mut("out").map(|v| v.attach(output_connector.clone(), None));

// connect the output_connector from `my_component` out put port to `second_component`'s input port
let mut second_component = Component::new(ComponentOptions {...});
second_component...get_outports_mut().ports.get_mut("in").map(|v| v.attach(output_connector.clone(), None));

// Send data to my_component's `in` port to trigger its process function
let _ = trigger...post(
    Some(IP::new(
        IPType::Data(json!("start")),
        IPOptions::default(),
    )),
    true,
);
```

Full example that demonstrates generation of an HTML code using ZFlow.

```rs
let mut str = "".to_string();
let mut level = 0;

// Create the C component
let mut c = Component::new(ComponentOptions {
    forward_brackets:HashMap::new(),
    // set input port `tags`
    in_ports: HashMap::from([(
        "tags".to_string(),
        InPort::default(),
    )]),
    // set output port `html`
    out_ports: HashMap::from([(
        "html".to_string(),
        OutPort::default(),
    )]),
    // the process function that takes the input data and transform them into an html string that is then sent out via the output port
    process: Box::new(move |context, input, output| {
        let ip_data = input.get("tags").expect("expected inport data").datatype;

        match ip_data {
            IPType::OpenBracket(data) =>{
                str.push_str(format!("<{}>", data.as_str().unwrap()).as_str());
                level += 1;
            }
            IPType::Data(data) =>{
                str.push_str(format!("{}", data.as_str().unwrap()).as_str());
            }
            IPType::CloseBracket(data) =>{
                str.push_str(format!("</{}>", data.as_str().unwrap()).as_str());
                level -= 1;
                if level <= 0 {
                    output.send(&("html", json!(str.clone())));
                    str.push_str("");
                }
            }
            _=>{}
        }
        output.done(None);
        Ok(ProcessResult::default())
    }),
    ..ComponentOptions::default()
});

// Create the D component
let mut d = Component::new(ComponentOptions {
    // set input port `bang`
    in_ports: HashMap::from([(
        "bang".to_string(),
        InPort::default(),
    )]),
    // set output port `tags`
    out_ports: HashMap::from([(
        "tags".to_string(),
        OutPort::default(),
    )]),
    // the process function that generates the html tags that we would send to the C component
    process: Box::new(|context, input, output| {
        if let Some(_bang) = input.get("bang") {
            output.send(&("tags", IPType::OpenBracket(json!("p"))));
            output.send(&("tags", IPType::OpenBracket(json!("em"))));
            output.send(&("tags", IPType::Data(json!("Hello"))));
            output.send(&("tags", IPType::CloseBracket(json!("em"))));
            output.send(&("tags", IPType::Data(json!(", "))));
            output.send(&("tags", IPType::OpenBracket(json!("strong"))));
            output.send(&("tags", IPType::Data(json!("World!"))));
            output.send(&("tags", IPType::CloseBracket(json!("strong"))));
            output.send(&("tags", IPType::CloseBracket(json!("p"))));
        }
        output.done(None);
        Ok(ProcessResult::default())
    }),
..ComponentOptions::default()});

// create internal sockets that will connect our components together via their ports
let mut s1 = InternalSocket::create(None);
let mut s2 = InternalSocket::create(None);
let mut s3 = InternalSocket::create(None);

// collect the final result via the 3rd internal socket
s3...on(|event| {
    match event.as_ref() {
        SocketEvent::IP(ip, None) => {
            match &ip.datatype {
                IPType::Data(data) => {
                    assert_eq!(data, &json!("<p><em>Hello</em>, <strong>World!</strong></p>"));
                    return;
                }
                _ => {}
            }
        }
        _ => {}
    }
});

// attach the sockets to the respective input and output ports,
// this will allow communication between components
// through the attached internal sockets
d...get_inports_mut().ports.get_mut("bang").map(|v| v.attach(s1.clone(), None));
d...get_outports_mut().ports.get_mut("tags").map(|v| v.attach(s2.clone(), None));
c...get_inports_mut().ports.get_mut("tags").map(|v| v.attach(s2.clone(), None));
c...get_outports_mut().ports.get_mut("html").map(|v| v.attach(s3.clone(), None));

// Send data to trigger the D component's process function
let _ = s1...post(
    Some(IP::new(
        IPType::Data(json!("start")),
        IPOptions::default(),
    )),
    true,
);
```

See [the runtime tests cases](https://github.com/darmie/zflow/blob/main/zflow_runtime/src/component_test.rs) for more examples