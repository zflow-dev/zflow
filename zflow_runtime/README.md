# ZFlow Process Runtime
The main runtime for the execution of the directed graph


## Graph Runtime example 
Some details may be hidden for brevity
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
        let ip_data = input...get("tags").expect("expected inport data").datatype;

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
                    if let Ok(output) = output.clone().try_lock().as_mut() {
                        output.send(&("html", json!(str.clone())));
                    }
                    str.push_str("");
                }
            }
            _=>{}
        }
        let mut output = output.try_lock().unwrap();
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
        let mut output = output.try_lock().unwrap();
        let mut input = input.try_lock().unwrap();
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

See [the runtime tests cases](https://github.com/darmie/zflow/blob/618f1ca4304d44400b6b7021d098d35240dedc62/zflow_runtime/src/component_test.rs) for more examples
