#[cfg(test)]
mod tests {
    use crate::{
        component::{Component, ComponentOptions},
        ip::IPType,
        network::{BaseNetwork, Network, NetworkEvent, NetworkOptions},
        port::{BasePort, InPort, OutPort, PortOptions},
        process::ProcessResult,
    };
    use assert_json_diff::assert_json_eq;
    use beady::scenario;
    use serde_json::json;
    use std::sync::Arc;
    use std::{
        collections::HashMap,
        sync::RwLock,
    };
    use zflow_graph::{
        types::{GraphEdge, GraphLeaf, GraphNode},
        Graph,
    };

    #[scenario]
    #[test]
    fn test_network() {
        'given_a_zflo_network: {
            let mut base_dir = std::env::current_dir().unwrap();
            base_dir.push("test_components");
            let base_dir = base_dir.to_str().unwrap();

            let split = Component::new(ComponentOptions {
                in_ports: HashMap::from_iter([("in".to_string(), InPort::default())]),
                out_ports: HashMap::from_iter([("out".to_string(), OutPort::default())]),
                process: Some(Box::new(|handle| {
                    if let Ok(handle) = handle.try_lock().as_mut() {
                        if let Some(data) = handle.input().get("in") {
                            handle.output().send_done(&json!({ "out": data }))?;
                        }
                    }
                    Ok(ProcessResult::default())
                })),
                ..ComponentOptions::default()
            });
            let merge = Component::new(ComponentOptions {
                in_ports: HashMap::from_iter([("in".to_string(), InPort::default())]),
                out_ports: HashMap::from_iter([("out".to_string(), OutPort::default())]),
                process: Some(Box::new(|handle| {
                    if let Ok(handle) = handle.try_lock().as_mut() {
                        if let Some(data) = handle.input().get("in") {
                            handle.output().send_done(&json!({ "out": data }))?;
                        }
                    }
                    Ok(ProcessResult::default())
                })),
                ..ComponentOptions::default()
            });
            let callback = Component::new(ComponentOptions {
                in_ports: HashMap::from_iter([
                    ("in".to_string(), InPort::default()),
                    (
                        "callback".to_string(),
                        InPort::new(PortOptions {
                            control: true,
                            ..PortOptions::default()
                        }),
                    ),
                ]),
                process: Some(Box::new(|handle| {
                    if let Ok(handle) = handle.try_lock().as_mut() {
                        if !handle.input().has_data("in") || !handle.input().has_data("callback") {
                            return Ok(ProcessResult::default());
                        }
                        handle.output().done(None);
                    }
                    Ok(ProcessResult::default())
                })),
                ..ComponentOptions::default()
            });
            'when_with_an_empty_graph: {
                let g = zflow_graph::Graph::new("my_graph", false);
                let mut n = Network::create(
                    g.clone(),
                    NetworkOptions {
                        subscribe_graph: false,
                        delay: true,
                        base_dir: base_dir.to_string(),
                        ..NetworkOptions::default()
                    },
                );

                let n = n.connect().unwrap();

                'then_it_should_initially_be_marked_as_stopped: {
                    let binding = n.clone();
                    let n = binding.try_lock().unwrap();
                    assert!(!n.is_started());
                }
                'then_it_should_initially_have_no_process: {
                    let binding = n.clone();
                    let n = binding.try_lock().unwrap();
                    assert!(n.get_processes().is_empty());
                }

                'then_it_should_initially_have_no_active_process: {
                    let binding = n.clone();
                    let n = binding.try_lock().unwrap();
                    assert!(n.get_active_processes().is_empty());
                }

                'then_it_should_initially_have_no_connections: {
                    let binding = n.clone();
                    let n = binding.try_lock().unwrap();
                    assert!(n.get_connections().is_empty());
                }

                'then_it_should_initially_have_no_iips: {
                    let binding = n.clone();
                    let n = binding.try_lock().unwrap();
                    assert!(n.get_initials().is_empty());
                }
                'then_it_should_have_reference_to_graph: {
                    let binding = n.clone();
                    let n = binding.try_lock().unwrap();
                    if let Ok(graph) = n.get_graph().try_lock() {
                        assert!(true);
                    } else {
                        assert!(false);
                    }
                }
                'then_it_know_its_base_dir: {
                    let binding = n.clone();
                    let n = binding.try_lock().unwrap();
                    assert!(!n.get_base_dir().is_empty());
                }
                'then_it_should_be_able_to_list_components: {
                    let binding = n.clone();
                    let mut binding = binding.try_lock();
                    let n = binding.as_mut().unwrap();
                    if let Ok(components) = n
                        .get_loader()
                        .list_components()
                        .expect("expected list of components")
                        .try_lock()
                    {
                        assert!(true);
                    } else {
                        assert!(false);
                    }
                }
                'then_it_should_have_an_uptime: {
                    let binding = n.clone();
                    let n = binding.try_lock().unwrap();
                    assert_eq!(n.uptime(), 0);
                }

                'then_when_with_new_node: {
                    'and_then_it_should_contain_node: {
                        let binding = n.clone();
                        let mut binding = binding.try_lock();
                        let n = binding.as_mut().unwrap();
                        let res = n.add_node(
                            GraphNode {
                                id: "add_wasm".to_string(),
                                component: "add_wasm".to_string(),
                                metadata: Some(json!({"foo": "Bar"}).as_object().cloned().unwrap()),
                                ..GraphNode::default()
                            },
                            None,
                        );
                        assert!(!res.is_err());

                        'and_then_it_should_have_registered_the_node_with_the_graph: {
                            let node = n.get_node("add_wasm");
                            assert!(node.is_some());
                            assert_eq!(node.unwrap().component_name, "add_wasm");
                        }
                        'and_then_it_should_have_transmitted_the_node_metadata_to_the_process: {
                            // let binding = n.clone();
                            // let n = binding.try_lock().unwrap();
                            if let Some(component) =
                                n.get_processes().get("add_wasm").unwrap().component.clone()
                            {
                                let meta = component.clone().try_lock().unwrap().metadata.clone();
                                assert!(meta.is_some());
                                assert_json_diff::assert_json_eq!(meta, json!({"foo": "Bar"}));
                            } else {
                                assert!(false);
                            }
                        }
                        'and_then_adding_the_same_node_again_should_be_a_noop: {
                            let processes = n.get_processes();
                            let original_process = processes.get("add_wasm").clone().unwrap();
                            let binding = n.get_graph();
                            let graph = binding.try_lock().unwrap();
                            let graph_node = graph.get_node("add_wasm").unwrap();

                            let binding = n.add_node(graph_node.clone(), None);
                            let res = binding.as_ref().unwrap();
                            let original_comp = original_process.component.as_ref().unwrap();
                            let other = res.component.as_ref().unwrap();
                            assert!(Arc::ptr_eq(original_comp, other));
                        }
                        'and_then_it_should_not_contain_the_node_after_removal: {
                            n.remove_node(GraphNode {
                                id: "add_wasm".to_string(),
                                ..GraphNode::default()
                            })
                            .unwrap();
                            assert!(n.get_processes().is_empty());

                            'and_then_it_should_have_removed_the_node_from_the_graph: {
                                if let Ok(graph) = n.get_graph().try_lock() {
                                    assert!(graph.get_node("add_wasm").is_none());
                                }
                            }
                            'and_then_it_should_fail_when_removing_the_removed_node_again: {
                                assert!(n
                                    .remove_node(GraphNode {
                                        id: "add_wasm".to_string(),
                                        ..GraphNode::default()
                                    })
                                    .is_err());
                            }
                        }
                    }
                }
                'then_when_with_new_edge: {
                    let binding = n.clone();
                    let mut binding = binding.try_lock();
                    let _n = binding.as_mut().unwrap();
                    _n.get_loader()
                        .register_component("", "split", split)
                        .expect("expected to register component");
                    // drop(binding);
                    _n.add_node(
                        GraphNode {
                            id: "A".to_string(),
                            component: "split".to_string(),
                            metadata: None,
                        },
                        None,
                    )
                    .unwrap();
                    _n.add_node(
                        GraphNode {
                            id: "B".to_string(),
                            component: "split".to_string(),
                            metadata: None,
                        },
                        None,
                    )
                    .unwrap();

                    'and_then_it_should_contain_edge: {
                        _n.add_edge(
                            GraphEdge {
                                from: GraphLeaf {
                                    node_id: "A".to_string(),
                                    port: "out".to_string(),
                                    index: None,
                                },
                                to: GraphLeaf {
                                    node_id: "B".to_string(),
                                    port: "in".to_string(),
                                    index: None,
                                },
                                metadata: None,
                            },
                            None,
                        )
                        .unwrap();

                      
                        assert!(!_n.get_connections().is_empty());
                        if let Ok(socket) = _n.get_connections()[0].clone().try_lock() {
                            let socket = socket.clone();
                            let from = socket.from.unwrap();
                            let to = socket.to.unwrap();
                            assert_eq!(from.port, "out");
                            assert_json_eq!(from.process.id, _n.get_node("A").unwrap().id);
                            assert!(Arc::ptr_eq(
                                from.process.component.as_ref().unwrap(),
                                _n.get_node("A").unwrap().component.as_ref().unwrap()
                            ));

                            assert_eq!(to.port, "in");
                            assert_json_eq!(to.process.id, _n.get_node("B").unwrap().id);
                            assert!(Arc::ptr_eq(
                                to.process.component.as_ref().unwrap(),
                                _n.get_node("B").unwrap().component.as_ref().unwrap()
                            ));
                        }

                        'and_then_it_should_have_registered_the_edge_with_the_graph: {
                            if let Ok(graph) = _n.get_graph().try_lock() {
                                let edge = graph.get_edge("A", "out", "B", "in");
                                assert!(edge.is_some());
                            }
                        }

                        'and_then_it_should_not_contain_edge_after_removal: {
                            _n.remove_edge(GraphEdge {
                                from: GraphLeaf {
                                    node_id: "A".to_string(),
                                    port: "out".to_string(),
                                    index: None,
                                },
                                to: GraphLeaf {
                                    node_id: "B".to_string(),
                                    port: "in".to_string(),
                                    index: None,
                                },
                                metadata: None,
                            })
                            .unwrap();
                            assert!(_n.get_connections().is_empty());

                            // cleanup
                            _n.remove_node(GraphNode {
                                id: "A".to_string(),
                                ..Default::default()
                            })
                            .unwrap();
                            _n.remove_node(GraphNode {
                                id: "B".to_string(),
                                ..Default::default()
                            })
                            .unwrap();
                        }
                    }
                }
                'then_when_with_simple_graph: {
                    let mut g = Graph::new("", false);
                    g.add_node("Merge", "Merge", None)
                        .add_node("Callback", "Callback", None)
                        .add_edge("Merge", "out", "Callback", "in", None)
                        .add_initial(json!("foo"), "Callback", "callback", None)
                        .add_initial(json!("Foo"), "Merge", "in", None);

                    let mut n = Network::create(
                        g.clone(),
                        NetworkOptions {
                            subscribe_graph: false,
                            delay: false,
                            base_dir: base_dir.to_string(),
                            ..NetworkOptions::default()
                        },
                    );

                    let loader = n.get_loader();
                    loader.register_component("", "Split", split).unwrap();
                    loader.register_component("", "Merge", merge).unwrap();
                    loader.register_component("", "Callback", callback).unwrap();

                    let n = n.connect().unwrap();

                    'and_then_it_should_send_some_initials_when_started: {
                        let binding = n.clone();
                        let mut binding = binding.try_lock();
                        let network = binding.as_mut().unwrap();
                        assert!(!network.get_initials().is_empty());
                        assert!(network.start().is_ok());
                        assert!(network.is_started());
                    }
                    'and_then_it_should_contain_two_processes: {
                        if let Ok(network) = n.clone().try_lock() {
                            assert!(!network.get_processes().is_empty());
                            assert!(network.get_processes().get("Merge").is_some());
                            assert!(network.get_processes().get("Callback").is_some());
                        }
                    }
                    'and_then_the_port_of_the_processes_should_know_the_node_names: {
                        if let Ok(network) = n.clone().try_lock() {
                            if let Ok(component) = network
                                .get_processes()
                                .get("Callback")
                                .unwrap()
                                .component
                                .clone()
                                .unwrap()
                                .try_lock()
                            {
                                component
                                    .get_inports()
                                    .ports
                                    .iter()
                                    .for_each(|(name, port)| {
                                        assert_eq!(&port.name, name);
                                        assert_eq!(&port.node, "Callback");
                                        assert_eq!(
                                            port.get_id(),
                                            format!("Callback {}", name.to_uppercase())
                                        );
                                    });

                                component
                                    .get_outports()
                                    .ports
                                    .iter()
                                    .for_each(|(name, port)| {
                                        assert_eq!(&port.name, name);
                                        assert_eq!(&port.node, "Callback");
                                        assert_eq!(
                                            port.get_id(),
                                            format!("Callback {}", name.to_uppercase())
                                        );
                                    });
                            }
                        }
                    }
                    'and_then_it_should_contain_1_connection_between_processes_and_2_for_iips: {
                        if let Ok(network) = n.clone().try_lock() {
                            assert!(!network.get_connections().is_empty());
                            assert_eq!(network.get_connections().len(), 3);
                        }
                    }

                    'and_then_with_renamed_node: {
                        'and_then_it_should_have_the_process_in_a_new_location: {
                            if let Ok(network) = n.clone().try_lock().as_mut() {
                                assert!(network.rename_node("Callback", "Func").is_ok());
                                assert!(network.get_processes().get("Func").is_some());
                            }
                            'and_then_it_should_not_have_the_process_in_the_old_location: {
                                if let Ok(network) = n.clone().try_lock().as_mut() {
                                    assert!(network.get_processes().get("Callback").is_none());
                                }
                            }
                            'and_then_it_should_fail_to_rename_with_the_old_name: {
                                if let Ok(network) = n.clone().try_lock().as_mut() {
                                    assert!(network.rename_node("Callback", "Func").is_err());
                                }
                            }
                            'and_then_it_should_have_informed_the_ports_of_their_new_node_name: {
                                if let Ok(network) = n.clone().try_lock().as_mut() {
                                    if let Ok(component) = network
                                        .get_processes()
                                        .get("Func")
                                        .unwrap()
                                        .component
                                        .clone()
                                        .unwrap()
                                        .try_lock()
                                    {
                                        component.get_inports().ports.iter().for_each(
                                            |(name, port)| {
                                                assert_eq!(&port.name, name);
                                                assert_eq!(&port.node, "Func");
                                                assert_eq!(
                                                    port.get_id(),
                                                    format!("Func {}", name.to_uppercase())
                                                );
                                            },
                                        );
                                        component.get_outports().ports.iter().for_each(
                                            |(name, port)| {
                                                assert_eq!(&port.name, name);
                                                assert_eq!(&port.node, "Func");
                                                assert_eq!(
                                                    port.get_id(),
                                                    format!("Func {}", name.to_uppercase())
                                                );
                                            },
                                        );
                                    }
                                }
                            }

                            'and_then_with_icon_change: {
                                'and_then_it_should_emit_an_icon_change_event: {
                                    let _binding = n.clone();
                                    let mut binding = _binding.lock();
                                    let network = binding.as_mut().unwrap();

                                    network.on(Box::new(|event| match event.as_ref() {
                                        NetworkEvent::Custom(ev, data) => {
                                            if ev == "icon" {
                                                assert!(data.is_object());
                                                if let Some(data) = data.as_object() {
                                                    assert!(data.contains_key("id"));
                                                    assert_eq!(
                                                        data.get("id"),
                                                        Some(&json!("Func"))
                                                    );
                                                    assert_eq!(
                                                        data.get("icon"),
                                                        Some(&json!("flask"))
                                                    );
                                                }
                                            }
                                        }
                                        _ => {}
                                    }));

                                    {
                                        let binding = network
                                            .get_processes()
                                            .get("Func")
                                            .unwrap()
                                            .component
                                            .clone()
                                            .unwrap();
                                        let mut binding = binding.try_lock();
                                        let component = binding.as_mut().unwrap();

                                        component.set_icon("flask");
                                        drop(network);
                                    }
                                }
                            }

                            'and_then_when_stopped: {
                                let _binding = n.clone();
                                let mut binding = _binding.lock();
                                let network = binding.as_mut().unwrap();
                                assert!(network.stop().is_ok());
                                assert!(network.is_stopped());
                                drop(binding);
                            }
                        }
                    }
                }

                'then_when_with_nodes_containing_default_ports: {
                    let found_data = Arc::new(RwLock::new(json!(null)));
                    let mut c = Component::new(ComponentOptions {
                        in_ports: HashMap::from_iter([(
                            "in".to_string(),
                            InPort::new(PortOptions {
                                required: true,
                                data_type: IPType::Data(json!("default-value")),
                                ..Default::default()
                            }),
                        )]),
                        out_ports: HashMap::from_iter([("out".to_string(), OutPort::default())]),
                        process: Some(Box::new(|handle| {
                            if let Ok(this) = handle.clone().try_lock().as_mut() {
                                this.output().send_done(&this.input().get("in").unwrap())?;
                            }
                            Ok(ProcessResult::default())
                        })),
                        ..Default::default()
                    });
                    let _found_data = found_data.clone();
                    let mut cb = Component::new(ComponentOptions {
                        in_ports: HashMap::from_iter([(
                            "in".to_string(),
                            InPort::new(PortOptions {
                                required: true,
                                ..Default::default()
                            }),
                        )]),
                        process: Some(Box::new(move |handle| {
                            if let Ok(this) = handle.clone().try_lock().as_mut() {
                                // this.output().send_done(&this.input().get("in").unwrap())?;
                                if let Some(data) = this.input().get("in") {
                                    match data.datatype {
                                        IPType::Data(value) => {
                                            let mut w = _found_data.write().unwrap();
                                            *w = value;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Ok(ProcessResult::default())
                        })),
                        ..Default::default()
                    });

                    let mut g = Graph::new("", true);
                    g.add_node("Def", "Def", None)
                        .add_node("Cb", "Cb", None)
                        .add_edge("Def", "out", "Cb", "in", None);

                    // This test blocks some times
                    'and_then_it_should_send_default_values_without_an_edge: {
                        let mut n = Network::create(
                            g.clone(),
                            NetworkOptions {
                                subscribe_graph: false,
                                delay: true,
                                base_dir: base_dir.to_string(),
                                ..Default::default()
                            },
                        );
                        n.get_loader()
                            .register_component("", "Def", c.clone())
                            .unwrap();
                        n.get_loader()
                            .register_component("", "Cb", cb.clone())
                            .unwrap();

                        let binding = n.connect().unwrap();
                        let mut binding = binding.try_lock();
                        let nw = binding.as_mut().unwrap();
                        let _ = nw.start();
                        // drop(binding);
                        assert_eq!(*found_data.clone().read().unwrap(), json!("default-value"));
                    }
                }
            }
        }
    }
}
