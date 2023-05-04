#[cfg(test)]
mod tests {
    use crate::component::Component;
    use crate::ip::{IPOptions, IPType, IP};
    use crate::port::{
        BasePort, InPort, InPorts, InPortsOptions, OutPort, PortOptions, PortsTrait,
    };
    use crate::sockets::{InternalSocket, SocketEvent};
    use beady::scenario;
    use futures::executor::block_on;
    use serde_json::{json, Value};

    #[scenario]
    #[test]
    fn test_inport() {
        'given_an_inport_port: {
            'when_with_default_options: {
                let p = InPort::new(PortOptions::default());

                'then_it_should_be_of_datatype_all: {
                    assert_eq!(p.get_data_type(), IPType::All(Value::default()));
                }
                'then_it_should_not_be_required: {
                    assert!(!p.is_required());
                }
                'then_it_should_not_be_addressable: {
                    assert!(!p.is_addressable());
                }
                'then_it_should_not_be_buffered: {
                    assert!(!p.is_buffered());
                }
            }
            'when_with_custom_data: {
                let p = InPort::new(PortOptions {
                    schema: "text/url".to_string(),
                    data_type: IPType::All(json!("string")),
                    ..PortOptions::default()
                });
                'then_it_should_retain_the_type: {
                    assert_eq!(p.get_data_type(), IPType::All(json!("string")));
                    assert_eq!(p.get_schema(), "text/url");
                }
            }
            'when_without_attached_sockets: {
                let p = InPort::new(PortOptions::default());
                'then_it_should_not_be_attached: {
                    assert!(!p.is_attached(None));
                    assert!(p.list_attached().is_empty());
                }
                'then_it_should_allow_attaching: {
                    assert!(p.can_attach());
                }
                'then_it_should_not_connected_initially: {
                    assert!(!p.is_connected(None).expect("expect not to throw"));
                }
                'then_it_should_contain_sockets_initially: {
                    assert!(p.sockets.is_empty());
                }
            }
            'when_with_processing_function_called_with_port_as_context: {
                'then_it_should_set_context_to_port_itself: {
                    let socket = InternalSocket::create(None);
                    let mut p = InPort::new(PortOptions::default());

                    p.on(|event| {
                        match event.as_ref() {
                            SocketEvent::IP(data, None) => match &data.datatype {
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("some-data"));
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    });
                    p.attach(socket.clone(), None);
                    let _ = block_on(
                        socket
                            .clone()
                            .lock()
                            .unwrap()
                            .send(Some(&json!("some-data"))),
                    );
                }
            }
            'when_with_default_value: {
                'then_it_should_send_the_default_value_as_packet: {
                    let socket = InternalSocket::create(None);
                    let mut p = InPort::new(PortOptions {
                        data_type: IPType::All(json!("default-value")),
                        ..PortOptions::default()
                    });
                    p.attach(socket.clone(), None);
                    p.on(|event| {
                        match event.as_ref() {
                            SocketEvent::IP(data, None) => match &data.datatype {
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("default-value"));
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    });

                    let _ = block_on(socket.clone().lock().unwrap().send_defaults());
                }
                'then_it_should_send_the_default_value_before_iip: {
                    let socket = InternalSocket::create(None);
                    let mut p = InPort::new(PortOptions {
                        data_type: IPType::All(json!("default-value")),
                        ..PortOptions::default()
                    });
                    p.attach(socket.clone(), None);
                    let mut recieved = vec!["default-value", "some-iip"];
                    p.on(move |event| match event.as_ref() {
                        SocketEvent::IP(data, None) => match &data.datatype {
                            IPType::Data(data) => {
                                assert_eq!(data, recieved.remove(0));
                            }
                            _ => {}
                        },
                        _ => {}
                    });

                    block_on(async move {
                        let _ = socket.clone().lock().unwrap().send_defaults().await;
                        let _ = socket
                            .clone()
                            .lock()
                            .unwrap()
                            .send(Some(&json!("some-iip")))
                            .await;
                    });
                }
            }
            'when_with_processing_shorthand: {
                'then_it_should_accept_metadata_when_provided: {
                    let socket = InternalSocket::create(None);
                    let mut inports = InPorts::new(InPortsOptions::default());
                    inports
                        .add(
                            "in",
                            &PortOptions {
                                schema: "text/plain".to_string(),
                                required: true,
                                ..PortOptions::default()
                            },
                        )
                        .expect("expected to add metadata to inports");
                    let mut _in = inports.get_mut("in").expect("expected inport");
                    _in.on(move |event| match event.as_ref() {
                        SocketEvent::IP(ip, None) => match &(*ip).datatype {
                            IPType::Data(data) => {
                                assert_eq!(*data, json!("some-data"));
                                assert_eq!((*ip).schema, "text/plain");
                            }
                            _ => {}
                        },
                        _ => {}
                    });
                    _in.attach(socket.clone(), None);
                    assert!(!_in.list_attached().is_empty());
                    block_on(async move {
                        if let Err(msg) = socket
                            .clone()
                            .try_lock()
                            .expect("expected socket instance")
                            .send(Some(&json!("some-data")))
                            .await
                        {
                            panic!("{}", msg);
                        }
                        let _ = socket
                            .clone()
                            .try_lock()
                            .expect("expected socket instance")
                            .disconnect();
                    });
                }
                'then_it_should_stamp_an_ip_object_schema_if_already_set: {
                    let socket = InternalSocket::create(None);
                    let mut inports = InPorts::new(InPortsOptions::default());
                    let mut p = InPort::new(PortOptions {
                        schema: "text/markdown".to_string(),
                        ..PortOptions::default()
                    });

                    p.on(move |event| match event.as_ref() {
                        SocketEvent::IP(ip, None) => match &(*ip).datatype {
                            IPType::Data(data) => {
                                assert_eq!(*data, json!("Hello"));
                                assert_eq!((*ip).schema, "text/plain");
                            }
                            _ => {}
                        },
                        _ => {}
                    });

                    p.handle_ip(
                        IP::new(
                            IPType::Data(json!("Hello")),
                            IPOptions {
                                schema: "text/plain".to_string(),
                                ..IPOptions::default()
                            },
                        ),
                        None,
                    );
                }
            }
        }
    }

    #[scenario]
    #[test]
    fn test_outport() {
        'given_an_outport_port: {
            'when_with_addressable_ports: {
                'then_it_should_be_able_to_send_to_a_specific_port: {
                    let s1 = InternalSocket::create(None);
                    let s2 = InternalSocket::create(None);
                    let s3 = InternalSocket::create(None);

                    let mut p = OutPort::new(PortOptions {
                        addressable: true,
                        ..PortOptions::default()
                    });
                    p.attach(s1.clone(), None);
                    p.attach(s2.clone(), None);
                    p.attach(s3.clone(), None);

                    assert_eq!(p.list_attached(), vec![0_usize, 1_usize, 2_usize]);

                    s1.clone()
                        .try_lock()
                        .expect("expected instance of socket s1")
                        .on(|event| match event.as_ref() {
                            SocketEvent::IP(data, _) => {
                                panic!()
                            }
                            _ => {}
                        });

                    s2.clone()
                        .try_lock()
                        .expect("expected instance of socket s2")
                        .on(|event| match event.as_ref() {
                            SocketEvent::IP(data, _) => match &data.datatype {
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("some-data"));
                                }
                                _ => {}
                            },
                            _ => {}
                        });

                    s3.clone()
                        .try_lock()
                        .expect("expected instance of socket s3")
                        .on(|event| match event.as_ref() {
                            SocketEvent::IP(data, _) => {
                                panic!()
                            }
                            _ => {}
                        });

                    let _ = block_on(p.send(&json!("some-data"), Some(1)));
                }
                'then_it_should_be_able_to_send_to_index_0: {
                    let s1 = InternalSocket::create(None);
                    let mut p = OutPort::new(PortOptions {
                        addressable: true,
                        ..PortOptions::default()
                    });
                    p.attach(s1.clone(), None);
                    s1.clone()
                        .try_lock()
                        .expect("expected instance of socket s1")
                        .on(|event| match event.as_ref() {
                            SocketEvent::IP(data, _) => match &data.datatype {
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("my-data"));
                                }
                                _ => {}
                            },
                            _ => {}
                        });
                    let _ = block_on(p.send(&json!("my-data"), Some(0)));
                }
                'then_it_should_throw_error_when_sent_data_without_an_address: {
                    let mut p = OutPort::new(PortOptions {
                        addressable: true,
                        ..PortOptions::default()
                    });
                    let s1 = InternalSocket::create(None);
                    p.attach(s1.clone(), None);
                    if let Err(msg) = block_on(p.send(&json!("some-data"), None)) {
                        assert!(!msg.is_empty());
                    }
                }
                'then_it_should_throw_error_when_specific_port_is_requested_with_non_addressable_port: {
                    let s1 = InternalSocket::create(None);
                    let s2 = InternalSocket::create(None);
                    let s3 = InternalSocket::create(None);

                    let mut p = OutPort::new(PortOptions::default());
                    p.attach(s1.clone(), None);
                    p.attach(s2.clone(), None);
                    p.attach(s3.clone(), None);

                    if let Err(msg) = block_on(p.send(&json!("some-data"), Some(1))) {
                        assert!(!msg.is_empty());
                    }
                }
                'then_it_should_give_correct_port_index_when_detaching_a_connection: {
                    let mut p = OutPort::new(PortOptions {
                        addressable: true,
                        ..PortOptions::default()
                    });
                    let s1 = InternalSocket::create(None);
                    let s2 = InternalSocket::create(None);
                    let s3 = InternalSocket::create(None);

                    p.attach(s1.clone(), None);
                    p.attach(s2.clone(), None);
                    p.attach(s3.clone(), None);

                    let s2_index = s2.clone().try_lock().unwrap().index;
                    let s3_index = s3.clone().try_lock().unwrap().index;
                    let mut expected_sockets = vec![s2_index, s3_index];

                    p.on(move |event| match event.as_ref() {
                        SocketEvent::Detach(index) => {
                            let attached_index = expected_sockets.remove(0);
                            assert_eq!(attached_index, *index);
                        }
                        _ => {}
                    });
                    p.detach(s2_index);
                    p.detach(s3_index);
                }
            }
            'when_with_caching_ports: {
                'then_it_should_repeat_previously_sent_value_on_attach_event: {
                    let s1 = InternalSocket::create(None);
                    let s2 = InternalSocket::create(None);

                    let mut p = OutPort::new(PortOptions {
                        caching: true,
                        ..PortOptions::default()
                    });
                    s1.clone()
                        .try_lock()
                        .unwrap()
                        .on(move |event| match event.as_ref() {
                            SocketEvent::IP(data, _) => match &data.datatype {
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("foo"));
                                }
                                _ => {}
                            },
                            _ => {}
                        });

                    let expected = vec![json!("foo"), json!("bar")];

                    s2.clone()
                        .try_lock()
                        .unwrap()
                        .on(move |event| match event.as_ref() {
                            SocketEvent::IP(data, _) => match &data.datatype {
                                IPType::Data(data) => {
                                    assert!(expected.contains(data));
                                }
                                _ => {}
                            },
                            _ => {}
                        });
                    p.attach(s1.clone(), None);
                    let _ = block_on(p.send(&json!("foo"), None));
                    p.detach(0);
                    p.attach(s2.clone(), None);
                    let _ = block_on(p.send(&json!("bar"), None));
                    let _ = block_on(p.disconnect(None));
                }
                'then_it_should_support_addressable_ports: {
                    let s1 = InternalSocket::create(None);
                    let s2 = InternalSocket::create(None);
                    let s3 = InternalSocket::create(None);

                    let mut p = OutPort::new(PortOptions {
                        caching: true,
                        addressable: true,
                        ..PortOptions::default()
                    });

                    p.attach(s1.clone(), None);
                    p.attach(s2.clone(), None);

                    let s2_index = s2.clone().try_lock().unwrap().index;

                    s1.clone()
                        .try_lock()
                        .unwrap()
                        .on(move |event| match event.as_ref() {
                            SocketEvent::IP(data, _) => {
                                panic!("should not show");
                            }
                            _ => {}
                        });
                    s2.clone()
                        .try_lock()
                        .unwrap()
                        .on(move |event| match event.as_ref() {
                            SocketEvent::IP(data, _) => match &data.datatype {
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("some-data"));
                                }
                                _ => {}
                            },
                            _ => {}
                        });

                    s3.clone()
                        .try_lock()
                        .unwrap()
                        .on(move |event| match event.as_ref() {
                            SocketEvent::IP(data, _) => match &data.datatype {
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("some-data"));
                                }
                                _ => {}
                            },
                            _ => {}
                        });

                    let _ = block_on(p.send(&json!("some-data"), Some(1)));
                    let _ = block_on(p.disconnect(Some(1)));
                    p.detach(s2_index);
                    p.attach(s3.clone(), Some(1));
                }
            }
            'when_with_ip_objects: {
                'then_should_send_data_ips_and_substreams: {
                    let s1 = InternalSocket::create(None);
                    let mut p = OutPort::new(PortOptions::default());
                    p.attach(s1.clone(), None);

                    let mut expected_events = vec!["data", "openBracket", "data", "closeBracket"];

                    s1.clone()
                        .try_lock()
                        .unwrap()
                        .on(move |event| match event.as_ref() {
                            SocketEvent::IP(data, _) => match &data.clone().datatype {
                                IPType::OpenBracket(_) => {
                                    assert_eq!("openBracket", expected_events.remove(0));
                                }
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("my-data"));
                                    assert_eq!("data", expected_events.remove(0));
                                }
                                IPType::CloseBracket(_) => {
                                    assert_eq!("closeBracket", expected_events.remove(0));
                                }
                                _ => {}
                            },
                            _ => {}
                        });

                    p.data(json!("my-data"), IPOptions::default(), None);
                    p.open_bracket(json!("null"), IPOptions::default(), None);
                    p.data(json!("my-data"), IPOptions::default(), None);
                    p.close_bracket(json!("null"), IPOptions::default(), None);
                }
                'then_it_should_stamp_an_ip_object_schema: {
                    let socket = InternalSocket::create(None);

                    let mut p = OutPort::new(PortOptions {
                        schema: "text/markdown".to_string(),
                        ..PortOptions::default()
                    });

                    p.attach(socket.clone(), None);

                    socket
                        .clone()
                        .try_lock()
                        .unwrap()
                        .on(move |event| match event.as_ref() {
                            SocketEvent::IP(ip, None) => match &(*ip).datatype {
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("Hello"));
                                    assert_eq!((*ip).schema, "text/markdown");
                                }
                                _ => {}
                            },
                            _ => {}
                        });

                    p.send_ip(&json!("Hello"), None, true);
                }
                'then_it_should_stamp_an_ip_object_schema_if_already_set: {
                    let socket = InternalSocket::create(None);

                    let mut p = OutPort::new(PortOptions {
                        schema: "text/markdown".to_string(),
                        ..PortOptions::default()
                    });

                    p.attach(socket.clone(), None);

                    socket
                        .clone()
                        .try_lock()
                        .unwrap()
                        .on(move |event| match event.as_ref() {
                            SocketEvent::IP(ip, None) => match &(*ip).datatype {
                                IPType::Data(data) => {
                                    assert_eq!(*data, json!("Hello"));
                                    assert_eq!((*ip).schema, "text/plain");
                                }
                                _ => {}
                            },
                            _ => {}
                        });

                    p.send_ip(
                        &IP::new(
                        IPType::Data(json!("Hello")),
                        IPOptions {
                            schema: "text/plain".to_string(),
                            ..IPOptions::default()
                        }),
                        None,
                        true,
                    );
                }
            }
        }
    }
}
