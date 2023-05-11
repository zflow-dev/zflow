[![Runtime](https://github.com/darmie/zflow/actions/workflows/runtime.yml/badge.svg)](https://github.com/darmie/zflow/actions/workflows/runtime.yml)

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
            if let Some(input_data) = handle.input().get("out") {
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
    process: Box::new(move |this| {
        if let Ok(handle) = this.try_lock().as_mut() {
            let ip_data = handle.input().get("tags").expect("expected inport data").datatype;

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
                        handle.output().send(&("html", json!(str.clone())));
                        str.push_str("");
                    }
                }
                _=>{}
            }
        }
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
    process: Box::new(|this| {
        if let Ok(handle) = this.try_lock().as_mut() {
            let mut output = handle.output();
            if let Some(_bang) = handle.input().get("bang") {
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
        }
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
