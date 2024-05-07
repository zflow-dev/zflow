#[cfg(test)]
mod tests {
    use crate::{
        component::{Component, ComponentOptions},
        ip::IPType,
        network::{Network, NetworkEvent, NetworkOptions},
        port::{BasePort, InPort, OutPort, PortOptions},
        process::ProcessResult,
    };
    use assert_json_diff::assert_json_eq;
    use beady::scenario;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use zflow_graph::{
        types::{GraphEdge, GraphLeaf, GraphNode},
        Graph,
    };

    use std::collections::HashMap;

    use crate::network::BaseNetwork;
    #[scenario]
    #[test]
    fn test_network() {
        'given_a_zflo_network: {
            let mut base_dir = std::env::current_dir().unwrap();
            base_dir.push("../test_components");
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
                        workspace_dir: base_dir.to_string(),
                        ..NetworkOptions::default()
                    },
                );

                let n = n.connect().unwrap();

                'then_it_should_initially_be_marked_as_stopped: {
                    let manager = n.manager.clone();
                    let m = manager.try_lock().unwrap();
                    assert!(!(*m).is_started());
                }
                'then_it_should_initially_have_no_process: {
                    assert!(n.get_processes().is_empty());
                }

                'then_it_should_initially_have_no_active_process: {
                    assert!(n.get_active_processes().is_empty());
                }

                'then_it_should_initially_have_no_connections: {
                    assert!(n.get_connections().is_empty());
                }

                'then_it_should_initially_have_no_iips: {
                    assert!(n.get_initials().is_empty());
                }
                'then_it_should_have_reference_to_graph: {
                    if let Ok(graph) = n.get_graph().try_lock() {
                        assert!(true);
                    } else {
                        assert!(false);
                    }
                }
                'then_it_know_its_base_dir: {
                    assert!(!n.get_workspace_dir().is_empty());
                }
                'then_it_should_be_able_to_list_components: {
                    assert!(!n.list_components().is_empty())
                }
                'then_it_should_have_an_uptime: {
                    assert_eq!(n.uptime(), 0);
                }

                'then_when_with_new_node: {
                    'and_then_it_should_contain_node: {
                        let res = n.add_node(
                            GraphNode {
                                id: "test/add".to_string(),
                                component: "add".to_string(),
                                metadata: Some(json!({"foo": "Bar"}).as_object().cloned().unwrap()),
                                ..GraphNode::default()
                            },
                            None,
                        );

                        assert!(!res.is_err());

                        'and_then_it_should_have_registered_the_node_with_the_graph: {
                            let node = n.get_node("test/add");
                            assert!(node.is_some());
                            assert_eq!(node.unwrap().component_name, "add");
                        }
                        'and_then_it_should_have_transmitted_the_node_metadata_to_the_process: {
                            if let Some(component) =
                                n.get_processes().get("test/add").unwrap().component.clone()
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
                            let original_process = processes.get("test/add").clone().unwrap();
                            let binding = n.get_graph();
                            let graph = binding.try_lock().unwrap();
                            let graph_node = graph.get_node("test/add").unwrap();

                            let binding = n.add_node(graph_node.clone(), None);
                            let res = binding.as_ref().unwrap();
                            let original_comp = original_process.component.as_ref().unwrap();
                            let other = res.component.as_ref().unwrap();
                            assert!(Arc::ptr_eq(original_comp, other));
                        }
                        'and_then_it_should_not_contain_the_node_after_removal: {
                            n.remove_node(GraphNode {
                                id: "test/add".to_string(),
                                ..GraphNode::default()
                            })
                            .unwrap();
                            assert!(n.get_processes().is_empty());

                            'and_then_it_should_have_removed_the_node_from_the_graph: {
                                if let Ok(graph) = n.get_graph().try_lock() {
                                    assert!(graph.get_node("test/add").is_none());
                                }
                            }
                            'and_then_it_should_fail_when_removing_the_removed_node_again: {
                                assert!(n
                                    .remove_node(GraphNode {
                                        id: "test/add".to_string(),
                                        ..GraphNode::default()
                                    })
                                    .is_err());
                            }
                        }
                    }
                }
                'then_when_with_new_edge: {
                    let mut n = Network::create(
                        g.clone(),
                        NetworkOptions {
                            subscribe_graph: false,
                            delay: true,
                            workspace_dir: base_dir.to_string(),
                            ..NetworkOptions::default()
                        },
                    );

                    n.register_component("A", "split", split.clone())
                        .expect("expected to register component");
                    n.register_component("B", "split", split.clone())
                        .expect("expected to register component");

                    n.add_node(
                        GraphNode {
                            id: "A/split".to_string(),
                            component: "split".to_string(),
                            metadata: None,
                        },
                        None,
                    )
                    .unwrap();
                    n.add_node(
                        GraphNode {
                            id: "B/split".to_string(),
                            component: "split".to_string(),
                            metadata: None,
                        },
                        None,
                    )
                    .unwrap();

                    'and_then_it_should_contain_edge: {
                        n.add_edge(
                            GraphEdge {
                                from: GraphLeaf {
                                    node_id: "A/split".to_string(),
                                    port: "out".to_string(),
                                    index: None,
                                },
                                to: GraphLeaf {
                                    node_id: "B/split".to_string(),
                                    port: "in".to_string(),
                                    index: None,
                                },
                                metadata: None,
                            },
                            None,
                        )
                        .unwrap();

                        assert!(!n.get_connections().is_empty());
                        if let Ok(socket) = n.get_connections()[0].clone().try_lock() {
                            let socket = socket.clone();
                            let from = socket.from.unwrap();
                            let to = socket.to.unwrap();
                            assert_eq!(from.port, "out");
                            assert_json_eq!(from.process.id, n.get_node("A/split").unwrap().id);
                            assert!(Arc::ptr_eq(
                                from.process.component.as_ref().unwrap(),
                                n.get_node("A/split").unwrap().component.as_ref().unwrap()
                            ));

                            assert_eq!(to.port, "in");
                            assert_json_eq!(to.process.id, n.get_node("B/split").unwrap().id);
                            assert!(Arc::ptr_eq(
                                to.process.component.as_ref().unwrap(),
                                n.get_node("B/split").unwrap().component.as_ref().unwrap()
                            ));
                        }

                        'and_then_it_should_have_registered_the_edge_with_the_graph: {
                            if let Ok(graph) = n.get_graph().try_lock() {
                                let edge = graph.get_edge("A/split", "out", "B/split", "in");
                                assert!(edge.is_some());
                            }
                        }

                        'and_then_it_should_not_contain_edge_after_removal: {
                            n.remove_edge(GraphEdge {
                                from: GraphLeaf {
                                    node_id: "A/split".to_string(),
                                    port: "out".to_string(),
                                    index: None,
                                },
                                to: GraphLeaf {
                                    node_id: "B/split".to_string(),
                                    port: "in".to_string(),
                                    index: None,
                                },
                                metadata: None,
                            })
                            .unwrap();
                            assert!(n.get_connections().is_empty());

                            // cleanup
                            n.remove_node(GraphNode {
                                id: "A/split".to_string(),
                                ..Default::default()
                            })
                            .unwrap();
                            n.remove_node(GraphNode {
                                id: "B/split".to_string(),
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
                            // base_dir: base_dir.to_string(),
                            ..NetworkOptions::default()
                        },
                    );

                    n.register_component("", "Split", split.clone()).unwrap();
                    n.register_component("", "Merge", merge.clone()).unwrap();
                    n.register_component("", "Callback", callback.clone()).unwrap();

                    let n = n.connect();
                    assert!(!n.is_err());
                    let n = n.unwrap();

                    'and_then_it_should_send_some_initials_when_started: {
                        assert!(!n.get_initials().is_empty());
                        assert!(n.start().is_ok());
                        let manager = n.manager.clone();
                        let m = manager.try_lock().unwrap();
                        assert!((m).is_started());
                    }
                    'and_then_it_should_contain_two_processes: {
                        assert!(!n.get_processes().is_empty());
                        assert!(n.get_processes().get("Merge").is_some());
                        assert!(n.get_processes().get("Callback").is_some());
                    }
                    'and_then_the_port_of_the_processes_should_know_the_node_names: {
                        if let Ok(component) = n
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
                    'and_then_it_should_contain_1_connection_between_processes_and_2_for_iips: {
                        assert!(!n.get_connections().is_empty());
                        assert_eq!(n.get_connections().len(), 3);
                    }

                    'and_then_with_renamed_node: {
                        'and_then_it_should_have_the_process_in_a_new_location: {
                            assert!(n.rename_node("Callback", "Func").is_ok());
                            assert!(n.get_processes().get("Func").is_some());

                            'and_then_it_should_not_have_the_process_in_the_old_location: {
                                assert!(n.get_processes().get("Callback").is_none());
                            }
                            'and_then_it_should_fail_to_rename_with_the_old_name: {
                                assert!(n.rename_node("Callback", "Func").is_err());
                            }
                            'and_then_it_should_have_informed_the_ports_of_their_new_node_name: {
                                if let Ok(component) = n
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

                            'and_then_with_icon_change: {
                                'and_then_it_should_emit_an_icon_change_event: {
                                    n.on(Box::new(|event| match event.as_ref() {
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
                                        let binding = n
                                            .get_processes()
                                            .get("Func")
                                            .unwrap()
                                            .component
                                            .clone()
                                            .unwrap();
                                        let mut binding = binding.try_lock();
                                        let component = binding.as_mut().unwrap();

                                        component.set_icon("flask");
                                    }
                                }
                            }

                            'and_then_when_stopped: {
                                assert!(n.stop().is_ok());
                                let manager = n.manager.clone();
                                let m = manager.try_lock().unwrap();
                                assert!((m).is_stopped());
                            }
                        }
                    }
                }

                'then_when_with_nodes_containing_default_ports: {
                    let found_data = Arc::new(Mutex::new(json!(null)));
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
                                            let fdata = _found_data.clone();
                                            let mut _found_data = fdata.lock().unwrap();
                                            *_found_data = value.into();
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

                    let data_binding = found_data.clone();

                    // This test blocks some times
                    'and_then_it_should_send_default_values_without_an_edge: {
                        let mut n = Network::create(
                            g.clone(),
                            NetworkOptions {
                                subscribe_graph: false,
                                delay: true,
                                workspace_dir: base_dir.to_string(),
                                ..Default::default()
                            },
                        );

                    
                        n.register_component("", "Def", c.clone()).unwrap();
                        n.register_component("", "Cb", cb.clone()).unwrap();
                     

                        let nw = n.connect().unwrap();

                        let _ = nw.start();
                        // drop(binding);

                        let mut data = data_binding.lock().unwrap();
                        assert_eq!(data.take(), json!("default-value"));
                        let _ = nw.stop();
                    }
                }
            }
        }
    }
}
