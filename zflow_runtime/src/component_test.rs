#[cfg(test)]
mod tests {
    use beady::scenario;

    use crate::ip::{IPOptions, IP};
    use crate::sockets::SocketEvent;
    use crate::{
        component::{Component, ComponentEvent, ComponentOptions},
        ip::IPType,
        port::{BasePort, InPort, OutPort, PortOptions},
        process::{ProcessError, ProcessHandle, ProcessResult},
        sockets::InternalSocket,
    };
    use futures::executor::block_on;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::thread::sleep;
    use std::time::Duration;

    #[scenario]
    #[test]
    fn test_component() {
        // env_logger::init();
        'given_a_component: {
            'when_with_required_ports: {
                'then_it_should_throw_an_error_upon_sending_packet_to_an_unattached_required_port: {
                    let mut s = InternalSocket::create(None);
                    let mut c = Component::new(ComponentOptions {
                        out_ports: HashMap::from([
                            (
                                "required_port".to_string(),
                                OutPort::new(PortOptions {
                                    required: true,
                                    ..PortOptions::default()
                                }),
                            ),
                            (
                                "optional_port".to_string(),
                                OutPort::new(PortOptions::default()),
                            ),
                        ]),
                        ..ComponentOptions::default()
                    });
                    c.out_ports
                        .ports
                        .get_mut("optional_port")
                        .map(|val| val.attach(s, None));
                    c.out_ports.ports.get_mut("required_port").map(|val| {
                        if let Err(err) = block_on(val.send(&json!("foo"), None)) {
                            assert!(!err.is_empty());
                        } else {
                            assert!(true);
                        }
                    });
                }
                'then_it_should_be_cool_with_attached_port: {
                    let mut s1 = InternalSocket::create(None);
                    let mut s2 = InternalSocket::create(None);
                    let mut c = Component::new(ComponentOptions {
                        in_ports: HashMap::from([
                            (
                                "required_port".to_string(),
                                InPort::new(PortOptions {
                                    required: true,
                                    ..PortOptions::default()
                                }),
                            ),
                            (
                                "optional_port".to_string(),
                                InPort::new(PortOptions::default()),
                            ),
                        ]),
                        ..ComponentOptions::default()
                    });
                    c.in_ports
                        .ports
                        .get_mut("optional_port")
                        .map(|val| val.attach(s1.clone(), None));
                    c.in_ports
                        .ports
                        .get_mut("required_port")
                        .map(|val| val.attach(s2.clone(), None));

                    assert!(
                        block_on(s1.try_lock().unwrap().send(Some(&json!("some-more-data"))))
                            .is_ok()
                    );
                    assert!(
                        block_on(s2.try_lock().unwrap().send(Some(&json!("some-data")))).is_ok()
                    );
                }
            }
            'when_with_component_creation_shorthand: {
                'then_it_should_make_component_creation_easy: {
                    let mut c = Component::init(ComponentOptions {
                        in_ports: HashMap::from([
                            (
                                "in".to_string(),
                                InPort::new(PortOptions {
                                    required: true,
                                    ..PortOptions::default()
                                }),
                            ),
                            (
                                "just_processor".to_string(),
                                InPort::new(PortOptions::default()),
                            ),
                        ]),
                        process: Some(Box::new(|this: Arc<Mutex<ProcessHandle>>| {
                            if let Ok(handle) = this.try_lock().as_mut() {
                                if handle.input().has_data("in") {
                                    if let Some(packet) = handle.input().get("in") {
                                        match &packet.datatype {
                                            IPType::Data(packet) => {
                                                assert_eq!(packet, &json!("some-data"));
                                                handle.output().done(None);
                                                return Ok(ProcessResult::default());
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                if handle.input().has_data("just_processor") {
                                    if let Some(packet) = handle.input().get("just_processor") {
                                        match &packet.datatype {
                                            IPType::Data(packet) => {
                                                assert_eq!(packet, &json!("some-data"));
                                                handle.output().done(None);
                                                return Ok(ProcessResult::default());
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }

                            Ok(ProcessResult::default())
                        })),
                        ..ComponentOptions::default()
                    });
                    let mut s1 = InternalSocket::create(None);
                    c.clone()
                        .try_lock()
                        .unwrap()
                        .in_ports
                        .ports
                        .get_mut("in")
                        .map(|val| {
                            val.attach(s1.clone(), None);
                            // val.node_instance = c.clone();
                        });
                    let mut s2 = InternalSocket::create(None);
                    c.clone()
                        .try_lock()
                        .unwrap()
                        .in_ports
                        .ports
                        .get_mut("just_processor")
                        .map(|val| {
                            val.attach(s2.clone(), None);
                            // val.node_instance = c.clone();
                        });

                    block_on(async move {
                        let _ = s1
                            .clone()
                            .try_lock()
                            .unwrap()
                            .send(Some(&json!("some-data")))
                            .await;
                        let _ = s2
                            .clone()
                            .try_lock()
                            .unwrap()
                            .send(Some(&json!("some-data")))
                            .await;
                    });
                }
                'then_it_should_throw_error_if_there_is_no_error_port: {
                    let mut c = Component::init(ComponentOptions {
                        in_ports: HashMap::from([(
                            "in".to_string(),
                            InPort::new(PortOptions {
                                required: true,
                                ..PortOptions::default()
                            }),
                        )]),
                        process: Some(
                            // process
                            Box::new(|this: Arc<Mutex<ProcessHandle>>| {
                                if let Ok(handle) = this.try_lock().as_mut() {
                                    if let Some(packet) = handle.input().get_data("in") {
                                        assert_eq!(packet, json!("some-data"));
                                        assert!(handle
                                            .output()
                                            .error(&ProcessError("".to_string()))
                                            .is_err());
                                        return Ok(ProcessResult::default());
                                    }
                                }
                                Ok(ProcessResult::default())
                            }),
                        ),
                        ..ComponentOptions::default()
                    });
                    let mut s1 = InternalSocket::create(None);
                    c.clone()
                        .try_lock()
                        .unwrap()
                        .in_ports
                        .ports
                        .get_mut("in")
                        .map(|val| {
                            val.attach(s1.clone(), None);
                            // val.node_instance = c.clone();
                        });
                    let _ = block_on(
                        s1.clone()
                            .try_lock()
                            .unwrap()
                            .send(Some(&json!("some-data"))),
                    );
                }
                'then_it_should_not_throw_errors_if_there_is_a_non_required_error_port: {
                    let c = Component::init(ComponentOptions {
                        in_ports: HashMap::from([(
                            "in".to_string(),
                            InPort::new(PortOptions {
                                required: true,
                                ..PortOptions::default()
                            }),
                        )]),
                        out_ports: HashMap::from([(
                            "error".to_string(),
                            OutPort::new(PortOptions {
                                required: false,
                                ..PortOptions::default()
                            }),
                        )]),
                        // process
                        process: Some(Box::new(|this| {
                            if let Ok(handle) = this.try_lock().as_mut() {
                                if let Some(packet) = handle.input().get_data("in") {
                                    assert_eq!(packet, json!("some-data"));
                                }
                                assert!(handle
                                    .context
                                    .clone()
                                    .try_lock()
                                    .unwrap()
                                    .component
                                    .clone()
                                    .try_lock()
                                    .unwrap()
                                    .error(ProcessError(format!("Some Error")), vec![], None, None)
                                    .is_ok());
                            }

                            Ok(ProcessResult::default())
                        })),
                        ..ComponentOptions::default()
                    });

                    let mut s1 = InternalSocket::create(None);
                    c.clone()
                        .try_lock()
                        .unwrap()
                        .in_ports
                        .ports
                        .get_mut("in")
                        .map(|val| {
                            val.attach(s1.clone(), None);
                            // val.node_instance = c.clone();
                        });
                    let _ = block_on(
                        s1.clone()
                            .try_lock()
                            .unwrap()
                            .send(Some(&json!("some-data"))),
                    );
                }
            }
            'when_starting_a_component: {
                'then_should_flag_that_the_component_has_started_or_shutdown: {
                    let mut c = Component::new(ComponentOptions {
                        in_ports: HashMap::from([(
                            "in".to_string(),
                            InPort::new(PortOptions {
                                required: true,
                                ..PortOptions::default()
                            }),
                        )]),
                        ..ComponentOptions::default()
                    });

                    let mut s1 = InternalSocket::create(None);
                    c.in_ports.ports.get_mut("in").map(|val| {
                        val.attach(s1.clone(), None);
                    });
                    c.setup(|_| {
                        assert!(true);
                        Ok(())
                    });
                    c.teardown(|_| {
                        assert!(true);
                        Ok(())
                    });
                    c.bus
                        .clone()
                        .try_lock()
                        .unwrap()
                        .subscribe_fn(|event| match event.as_ref() {
                            ComponentEvent::Start => {
                                assert!(true);
                            }
                            _ => {}
                        });

                    let c = std::sync::Arc::new(std::sync::Mutex::new(c));
                    let _ = Component::start(c.clone());
                    assert!(c.clone().try_lock().unwrap().started);
                    let _ = Component::shutdown(c.clone());
                    sleep(Duration::from_millis(10));
                    assert!(!c.clone().try_lock().unwrap().is_started());
                }
            }
            'when_with_object_based_ips: {
                'then_it_should_speak_ip_objects: {
                    let mut c = Component::init(ComponentOptions {
                        in_ports: HashMap::from([("in".to_string(), InPort::default())]),
                        out_ports: HashMap::from([("out".to_string(), OutPort::default())]),
                        // process
                        process: Some(Box::new(|this| {
                            if let Ok(handle) = this.try_lock().as_mut() {
                                handle.output().send_done(
                                    &handle.input().get("in").expect("expected inport data"),
                                )?;
                            }
                            Ok(ProcessResult::default())
                        })),
                        ..ComponentOptions::default()
                    });
                    let mut s1 = InternalSocket::create(None);
                    let mut s2 = InternalSocket::create(None);

                    s2.clone()
                        .try_lock()
                        .unwrap()
                        .on(|event| match event.as_ref() {
                            SocketEvent::IP(ip, None) => {
                                assert_eq!(
                                    ip.userdata,
                                    json!({
                                        "groups": ["foo"]
                                    })
                                );
                                match &ip.datatype {
                                    IPType::Data(data) => {
                                        assert_eq!(data, &json!("some-data"));
                                        return;
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        });

                    if let Ok(component) = c.clone().try_lock().as_mut() {
                        component
                            .in_ports
                            .ports
                            .get_mut("in")
                            .map(|val| val.attach(s1.clone(), None));
                        component
                            .out_ports
                            .ports
                            .get_mut("out")
                            .map(|val| val.attach(s2.clone(), None));
                    }

                    let _ = s1.clone().try_lock().unwrap().post(
                        Some(IP::new(
                            IPType::Data(json!("some-data")),
                            IPOptions {
                                userdata: json!({
                                    "groups": ["foo"]
                                }),
                                ..IPOptions::default()
                            },
                        )),
                        true,
                    );
                }
                'then_it_should_support_substreams: {
                    let mut str = std::sync::Arc::new(std::sync::Mutex::new("".to_string()));
                    let mut level = 0;
                    let mut c = Component::init(ComponentOptions {
                        forward_brackets: HashMap::new(),
                        in_ports: HashMap::from([("tags".to_string(), InPort::default())]),
                        out_ports: HashMap::from([("html".to_string(), OutPort::default())]),
                        // process
                        process: Some(Box::new(move |this| {
                            let mut str = str.clone();
                            if let Ok(handle) = this.try_lock().as_mut() {
                                let ip_data = handle
                                    .input()
                                    .get("tags")
                                    .expect("expected inport data")
                                    .datatype;
                                if let Ok(str) = str.clone().try_lock().as_mut() {
                                    match ip_data {
                                        IPType::OpenBracket(data) => {
                                            str.push_str(
                                                format!("<{}>", data.as_str().unwrap()).as_str(),
                                            );
                                            level += 1;
                                        }
                                        IPType::Data(data) => {
                                            str.push_str(
                                                format!("{}", data.as_str().unwrap()).as_str(),
                                            );
                                        }
                                        IPType::CloseBracket(data) => {
                                            str.push_str(
                                                format!("</{}>", data.as_str().unwrap()).as_str(),
                                            );
                                            level -= 1;
                                            if level <= 0 {
                                                handle
                                                    .output()
                                                    .send(&("html", json!(*str.clone())))?;
                                                str.clear();
                                            }
                                        }
                                        _ => {}
                                    }
                                    handle.output().done(None);
                                }
                            }
                            Ok(ProcessResult::default())
                        })),
                        ..ComponentOptions::default()
                    });

                    let mut d = Component::init(ComponentOptions {
                            in_ports: HashMap::from([("bang".to_string(), InPort::default())]),
                            out_ports: HashMap::from([("tags".to_string(), OutPort::default())]),
                            process: // process
                            Some(Box::new(|this| {
                                if let Ok(handle) = this.try_lock().as_mut() {
                                    let mut output = handle.output();
                                    if let Some(_bang) = handle.input().get("bang") {
                                        output.send(&("tags", IPType::OpenBracket(json!("p"))))?;
                                        output.send(&("tags", IPType::OpenBracket(json!("em"))))?;
                                        output.send(&("tags", IPType::Data(json!("Hello"))))?;
                                        output.send(&("tags", IPType::CloseBracket(json!("em"))))?;
                                        output.send(&("tags", IPType::Data(json!(", "))))?;
                                        output.send(&("tags", IPType::OpenBracket(json!("strong"))))?;
                                        output.send(&("tags", IPType::Data(json!("World!"))))?;
                                        output.send(&("tags", IPType::CloseBracket(json!("strong"))))?;
                                        output.send(&("tags", IPType::CloseBracket(json!("p"))))?;
                                    }
                                    output.done(None);
                                }
                                Ok(ProcessResult::default())
                            })),
                            ..ComponentOptions::default()
                        });

                    let mut s1 = InternalSocket::create(None);
                    let mut s2 = InternalSocket::create(None);
                    let mut s3 = InternalSocket::create(None);

                    s3.clone().try_lock().unwrap().on(|event| {
                        match event.as_ref() {
                            SocketEvent::IP(ip, None) => match &ip.datatype {
                                IPType::Data(data) => {
                                    assert_eq!(
                                        data,
                                        &json!("<p><em>Hello</em>, <strong>World!</strong></p>")
                                    );
                                    return;
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    });

                    d.clone()
                        .try_lock()
                        .unwrap()
                        .get_inports_mut()
                        .ports
                        .get_mut("bang")
                        .map(|v| v.attach(s1.clone(), None));
                    d.clone()
                        .try_lock()
                        .unwrap()
                        .get_outports_mut()
                        .ports
                        .get_mut("tags")
                        .map(|v| v.attach(s2.clone(), None));
                    c.clone()
                        .try_lock()
                        .unwrap()
                        .get_inports_mut()
                        .ports
                        .get_mut("tags")
                        .map(|v| v.attach(s2.clone(), None));
                    c.clone()
                        .try_lock()
                        .unwrap()
                        .get_outports_mut()
                        .ports
                        .get_mut("html")
                        .map(|v| v.attach(s3.clone(), None));

                    let _ = s1.clone().try_lock().unwrap().post(
                        Some(IP::new(IPType::Data(json!("start")), IPOptions::default())),
                        true,
                    );
                }
                'then_should_be_able_to_send_ips_to_addressable_connections: {
                    let mut c = Component::init(ComponentOptions {
                            forward_brackets: HashMap::new(),
                            in_ports: HashMap::from([("foo".to_string(), InPort::default())]),
                            out_ports: HashMap::from([(
                                "baz".to_string(),
                                OutPort {
                                    options: PortOptions {
                                        addressable: true,
                                        ..PortOptions::default()
                                    },
                                    sockets: vec![InternalSocket::create(None); 2],
                                    ..OutPort::default()
                                },
                            )]),
                            process: // process
                            Some(Box::new(move |this| {
                                if let Ok(handle) = this.try_lock().as_mut() {
                                    if let Some(packet) = handle.input().get("foo") {
                                        let mut ip = packet;
                                        ip.index = if ip.datatype == IPType::Data(json!("first")) {
                                            Some(1)
                                        } else {
                                            Some(0)
                                        };
                                        ip.owner = None;
                                        handle.output().send_done(&ip)?;
                                    }
                                }
                                Ok(ProcessResult::default())
                            })),
                            ..ComponentOptions::default()
                        });
                    let mut sin1 = InternalSocket::create(None);
                    let mut sout1 = InternalSocket::create(None);
                    let mut sout2 = InternalSocket::create(None);
                    c.clone()
                        .try_lock()
                        .unwrap()
                        .get_inports_mut()
                        .ports
                        .get_mut("foo")
                        .map(|v| v.attach(sin1.clone(), None));
                    c.clone()
                        .try_lock()
                        .unwrap()
                        .get_outports_mut()
                        .ports
                        .get_mut("baz")
                        .map(|v| {
                            v.attach(sout1.clone(), Some(1));
                            v.attach(sout2.clone(), Some(0))
                        });

                    sout1.clone().try_lock().unwrap().on(move |event| {
                        if let SocketEvent::IP(ip, index) = event.as_ref() {
                            assert_json_diff::assert_json_eq!(
                                json!(ip.clone()),
                                json!(IP::new(
                                    IPType::Data(json!("first")),
                                    IPOptions {
                                        index: Some(1),
                                        ..IPOptions::default()
                                    }
                                ))
                            );
                        }
                    });
                    sout2.clone().try_lock().unwrap().on(move |event| {
                        if let SocketEvent::IP(ip, index) = event.as_ref() {
                            assert_json_diff::assert_json_eq!(
                                json!(ip.clone()),
                                json!(IP::new(
                                    IPType::Data(json!("second")),
                                    IPOptions {
                                        index: Some(0),
                                        ..IPOptions::default()
                                    }
                                ))
                            );
                        }
                    });

                    let _ = sin1.clone().try_lock().unwrap().post(
                        Some(IP::new(IPType::Data(json!("first")), IPOptions::default())),
                        true,
                    );
                    let _ = sin1.clone().try_lock().unwrap().post(
                        Some(IP::new(IPType::Data(json!("second")), IPOptions::default())),
                        true,
                    );
                }
            }
        }
    }
}
