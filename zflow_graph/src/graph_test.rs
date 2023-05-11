#[cfg(test)]
mod tests {

    use crate::Graph;
    use crate::types::{GraphEdge, GraphGroup, GraphIIP, GraphJson, GraphNode};
    use crate::internal::event_manager::EventListener;
    use assert_json_diff::assert_json_eq;
    use beady::scenario;

    use serde::Deserialize;
    use serde_json::{json, Value};

    #[scenario]
    #[test]
    fn fbp_graph() {
        'given_a_case_sensitive_graph: {
            'when_the_graph_instance_is_unnamed: {
                'then_it_should_have_an_empty_name: {
                    let g = Graph::new("", true);
                    assert_eq!(g.name.is_empty(), true);
                }
            }
            'when_a_new_instance_with_name: {
                let mut g = Graph::new("Foo bar", true);
                'then_it_should_get_a_name_from_instance: {
                    assert_eq!(g.name, "Foo bar");
                }
                'then_it_should_have_no_nodes_initially: {
                    assert_eq!(g.nodes.len(), 0);
                }
                'then_it_should_have_no_edhes_initially: {
                    assert_eq!(g.edges.len(), 0);
                }
                'then_it_should_have_no_initializers_initially: {
                    assert_eq!(g.initializers.len(), 0);
                }
                'then_it_should_have_no_inports_initially: {
                    assert_eq!(g.inports.keys().len(), 0);
                }
                'then_it_should_have_no_outports_initially: {
                    assert_eq!(g.outports.keys().len(), 0);
                }
                'and_when_given_a_new_node: {
                    'then_it_should_emit_an_event: {
                        g.connect(
                            "add_node",
                            |this, data| {
                                if let Ok(node) = GraphNode::deserialize(data) {
                                    assert_eq!(node.id, "Foo");
                                    assert_eq!(node.component, "Bar");
                                }
                            },
                            true,
                        );
                        g.add_node("Foo", "Bar", None);
                        'and_then_it_should_be_graph_list_of_nodes: {
                            assert_eq!(g.nodes.len(), 1);
                        }
                        'and_then_it_should_be_accessible_via_getter: {
                            let n = g.nodes.get(0);
                            let node = g.get_node("Foo");
                            if let Some(node) = node {
                                assert_eq!(node.id, "Foo");
                                assert_eq!(*node.uid, n.unwrap().uid);
                            }
                        }
                        'and_then_metadata_should_be_empty: {
                            let node = g.get_node("Foo");
                            if let Some(node) = node {
                                assert_eq!(node.metadata, None);
                            }
                        }
                        'and_then_it_should_be_available_in_json_export: {
                            let json = g.to_json();
                            assert_eq!(json.processes.get("Foo").is_none(), false);
                            if let Some(foo) = json.processes.get("Foo") {
                                assert_eq!(foo.component, "Bar");
                            }
                        }
                        'and_then_removing_a_node_should_emit_event: {
                            g.connect(
                                "remove_node",
                                |this, data| {
                                    if let Ok(node) = GraphNode::deserialize(data) {
                                        assert_eq!(node.id, "Foo", "GraphNode.id should be 'Foo'");
                                        assert_eq!(
                                            node.component, "Bar",
                                            "GraphNode.component should be 'Bar'"
                                        );
                                    }
                                },
                                true,
                            );
                            g.remove_node("Foo");

                            'and_then_node_should_not_be_available_after_removal: {
                                assert_eq!(g.get_node("Foo").is_none(), true);
                                assert_eq!(g.nodes.len(), 0);
                            }
                        }
                    }
                }
                'and_when_given_a_new_edge: {
                    'then_it_should_emit_an_event: {
                        g.connect(
                            "add_edge",
                            |this, data| {
                                if let Ok(edge) = GraphEdge::deserialize(data) {
                                    assert_eq!(edge.from.node_id, "Foo");
                                    assert_eq!(edge.to.port, "In");
                                }
                            },
                            true,
                        );
                        g.add_node("Foo", "foo", None);
                        g.add_node("Bar", "bar", None);
                        g.add_edge("Foo", "Out", "Bar", "In", None);

                        'and_then_it_should_add_an_edge: {
                            g.add_edge("Foo", "out", "Bar", "in2", None);
                            assert_eq!(g.edges.len(), 2);

                            'and_then_it_should_refuse_to_add_duplicate_edge: {
                                if let Some(edge) = g.edges.clone().get(0) {
                                    g.add_edge(
                                        &edge.from.node_id,
                                        &edge.from.port,
                                        &edge.to.node_id,
                                        &edge.to.port,
                                        None,
                                    );
                                    assert_eq!(g.edges.len(), 2);
                                }
                            }

                            'and_then_when_a_new_edge_with_index: {
                                'and_then_it_should_emit_an_event: {
                                    g.connect(
                                        "add_edge",
                                        |this, data| {
                                            if let Ok(edge) = GraphEdge::deserialize(data) {
                                                assert_eq!(edge.from.node_id, "Foo");
                                                assert_eq!(edge.to.port, "in");
                                                assert_eq!(edge.to.index, Some(1));
                                                assert_eq!(edge.from.index, None);
                                            }
                                        },
                                        true,
                                    );
                                    g.add_edge_index(
                                        "Foo",
                                        "out",
                                        None,
                                        "Bar",
                                        "in",
                                        Some(1),
                                        None,
                                    );
                                    assert_eq!(g.edges.len(), 3);

                                    'and_then_it_should_add_an_edge: {
                                        g.add_edge_index(
                                            "Foo",
                                            "out",
                                            Some(2),
                                            "Bar",
                                            "in2",
                                            None,
                                            None,
                                        );
                                        assert_eq!(g.edges.len(), 4);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            'when_given_a_json_string: {
                let json_string = "{\"caseSensitive\":true,\"properties\":{\"name\":\"Example\",\"foo\":\"Baz\",\"bar\":\"Foo\"},\"inports\":{\"inPut\":{\"process\":\"Foo\",\"port\":\"inPut\",\"metadata\":{\"x\":5,\"y\":100}}},\"outports\":{\"outPut\":{\"process\":\"Bar\",\"port\":\"outPut\",\"metadata\":{\"x\":500,\"y\":505}}},\"groups\":[{\"name\":\"first\",\"nodes\":[\"Foo\"],\"metadata\":{\"label\":\"Main\"}},{\"name\":\"second\",\"nodes\":[\"Foo2\",\"Bar2\"]}],\"processes\":{\"Foo\":{\"component\":\"Bar\",\"metadata\":{\"display\":{\"x\":100,\"y\":200},\"routes\":[\"one\",\"two\"],\"hello\":\"World\"}},\"Bar\":{\"component\":\"Baz\",\"metadata\":{}},\"Foo2\":{\"component\":\"foo\",\"metadata\":{}},\"Bar2\":{\"component\":\"bar\",\"metadata\":{}}},\"connections\":[{\"src\":{\"process\":\"Foo\",\"port\":\"outPut\"},\"tgt\":{\"process\":\"Bar\",\"port\":\"inPut\"},\"metadata\":{\"route\":\"foo\",\"hello\":\"World\"}},{\"src\":{\"process\":\"Foo\",\"port\":\"out2\"},\"tgt\":{\"process\":\"Bar\",\"port\":\"in2\",\"index\":2},\"metadata\":{\"route\":\"foo\",\"hello\":\"World\"}},{\"data\":\"Hello,world!\",\"tgt\":{\"process\":\"Foo\",\"port\":\"inPut\"}},{\"data\":\"Hello,world,2!\",\"tgt\":{\"process\":\"Foo\",\"port\":\"in2\"}},{\"data\":\"Cheers,world!\",\"tgt\":{\"process\":\"Foo\",\"port\":\"arr\",\"index\":0}},{\"data\":\"Cheers,world,2!\",\"tgt\":{\"process\":\"Foo\",\"port\":\"arr\",\"index\":1}}]}";

                'then_it_should_parse: {
                    let json: GraphJson = serde_json::from_str(json_string)
                        .or_else(|e| Err(e))
                        .unwrap();
                    assert_eq!(json.case_sensitive, true);

                    'and_then_it_should_produce_a_graph: {
                        let g = Graph::from_json_string(json_string, None);
                        assert_eq!(g.is_err(), false);
                    }
                    'and_then_it_should_produce_a_graph_from_json_object: {
                        let mut g = Graph::from_json(json.clone(), None);
                        assert_eq!(g.case_sensitive, true);

                        'and_then_it_should_have_a_name: {
                            assert_eq!(g.name, "Example");
                        }
                        'and_then_it_should_have_graph_metadata_intact: {
                            let properties = g.properties;
                            assert_eq!(properties.get("foo"), Some(&Value::from("Baz")));
                            assert_eq!(properties.get("bar"), Some(&Value::from("Foo")));
                        }
                        'and_then_it_should_produce_same_json_when_serialized: {
                            let a = json!(g.to_json());
                            let j = json.clone();
                            let b = json!(j);
                            assert_json_diff::assert_json_eq!(a, b);
                        }
                        'and_then_it_should_allow_modifying_graph_metadata: {
                            g.connect(
                                "change_properties",
                                |this, data| {
                                    if let Some(payload) = data.as_object() {
                                        let new_metadata = &payload["new"];
                                        let old_metadata = &payload["before"];

                                        assert_json_diff::assert_json_eq!(
                                            json!(this.properties),
                                            json!(new_metadata)
                                        );

                                        let want = json!({
                                            "foo": "Baz",
                                            "bar": "Bar",
                                            "hello": "World"
                                        });

                                        for key in this.properties.keys() {
                                            assert_eq!(this.properties[key], want[key]);
                                        }
                                    }
                                },
                                true,
                            );
                            let props = json!({
                                "hello": "World",
                                "bar": "Bar"
                            })
                            .as_object()
                            .unwrap()
                            .clone();
                            g.set_properties(props);
                        }
                        'and_then_it_should_contain_four_nodes: {
                            assert_eq!(g.nodes.len(), 4);
                        }
                        'and_then_the_first_node_should_have_its_metadata_intact: {
                            let node = g.get_node("Foo");
                            assert_ne!(node.is_none(), true);
                            if let Some(node) = node {
                                assert_ne!(node.metadata.is_none(), true);

                                if let Some(metadata) = node.metadata.clone() {
                                    assert_ne!(metadata.get("display").is_none(), true);
                                    if let Some(display) = metadata.get("display") {
                                        assert_ne!(display.as_object().is_none(), true);
                                        assert_eq!(*display.get("x").unwrap(), json!(100));
                                        assert_eq!(*display.get("y").unwrap(), json!(200));
                                    }
                                    assert_ne!(metadata.get("routes").is_none(), true);
                                    assert_eq!(metadata.get("routes").unwrap().is_array(), true);
                                    if let Some(routes) = metadata.get("routes").unwrap().as_array()
                                    {
                                        assert!(routes.contains(&json!("one")));
                                        assert!(routes.contains(&json!("two")));
                                    }
                                }
                            }
                        }
                        'and_then_it_should_allow_modifying_node_metadata: {
                            g.connect(
                                "change_node",
                                |this, value| {
                                    if let Some(payload) = value.as_object() {
                                        let node =
                                            GraphNode::deserialize(&payload["node"]).unwrap();

                                        assert_ne!(node.metadata.is_none(), true);
                                        if let Some(metadata) = node.metadata.clone() {
                                            assert_eq!(
                                                metadata.get("routes").unwrap().is_array(),
                                                true
                                            );
                                            if let Some(routes) =
                                                metadata.get("routes").unwrap().as_array()
                                            {
                                                assert!(routes.contains(&json!("one")));
                                                assert!(routes.contains(&json!("two")));
                                            }
                                            assert_eq!(
                                                *metadata.get("hello").unwrap(),
                                                json!("world")
                                            );
                                        }
                                    }
                                },
                                true,
                            );
                            g.set_node_metadata(
                                "Foo",
                                json!({
                                    "hello":"world"
                                })
                                .as_object()
                                .unwrap()
                                .clone(),
                            );
                            'and_then_it_should_contain_two_connections: {
                                assert!(g.edges.len() == 2);
                            }
                        }
                        'and_then_the_first_edge_should_have_metadata_intact: {
                            let edge = g.edges.get(0);
                            assert!(edge.is_some());
                            assert!(edge.unwrap().metadata.is_some());
                            if let Some(metadata) = edge.unwrap().metadata.clone() {
                                assert_eq!(*metadata.get("route").unwrap(), json!("foo"));
                            }
                        }
                        'and_then_it_should_allow_modifying_edge_metadata: {
                            g.connect(
                                "change_edge",
                                |this, value| {
                                    if let Some(payload) = value.as_object() {
                                        let edge =
                                            GraphEdge::deserialize(&payload["edge"]).unwrap();

                                        if let Some(metadata) = edge.metadata.clone() {
                                            assert_eq!(
                                                *metadata.get("route").unwrap(),
                                                json!("foo")
                                            );
                                            assert_eq!(
                                                *metadata.get("hello").unwrap(),
                                                json!("World")
                                            );
                                        }
                                    }
                                },
                                true,
                            );
                            if let Some(edge) = g.edges.clone().get(0) {
                                g.set_edge_metadata(
                                    &edge.from.node_id,
                                    &edge.from.port,
                                    &edge.to.node_id,
                                    &edge.to.port,
                                    json!({"hello": "World"}).as_object().unwrap().clone(),
                                );
                            }
                        }
                        'and_then_it_should_contain_four_iips: {
                            assert!(g.initializers.len() == 4);
                        }
                        'and_then_it_should_contain_one_published_inport: {
                            assert!(g.inports.len() == 1);
                        }
                        'and_then_it_should_contain_one_published_outport: {
                            assert!(g.outports.len() == 1);
                        }
                        'and_then_it_should_keep_output_export_metadata_intact: {
                            let exp = &g.outports["outPut"];
                            assert_eq!(exp.metadata.is_some(), true);
                            if let Some(metadata) = &exp.metadata {
                                assert_eq!(*metadata.get("x").unwrap(), json!(500));
                                assert_eq!(*metadata.get("y").unwrap(), json!(505));
                            }
                        }
                        'and_then_it_should_contain_two_groups: {
                            assert!(g.groups.len() == 2);
                        }
                        'and_then_it_should_allow_modifying_group_metadata: {
                            g.connect(
                                "change_group",
                                |this, value| {
                                    if let Some(payload) = value.as_object() {
                                        let group =
                                            GraphGroup::deserialize(&payload["group"]).unwrap();

                                        if let Some(metadata) = group.metadata.clone() {
                                            assert_eq!(
                                                *metadata.get("label").unwrap(),
                                                json!("Main")
                                            );
                                            assert_eq!(*metadata.get("foo").unwrap(), json!("Bar"));
                                        }
                                    }
                                },
                                true,
                            );
                            g.set_group_metadata(
                                "first",
                                json!({"foo": "Bar"}).as_object().unwrap().clone(),
                            );
                            if let Some(group) = g.groups.get(1) {
                                if let Some(metadata) = group.metadata.clone() {
                                    assert_json_diff::assert_json_eq!(json!(metadata), json!({}));
                                }
                            }
                        }
                        'and_then_it_should_allow_renaming_groups: {
                            if let Some(_) = g.groups.clone().get(0) {
                                g.connect(
                                    "rename_group",
                                    |this, value| {
                                        if let Some(payload) = value.as_object() {
                                            let old_name = payload["old"].as_str().unwrap();
                                            let new_name = payload["new"].as_str().unwrap();

                                            assert_eq!(old_name, "first".to_string());
                                            assert_eq!(new_name, "renamed".to_string());

                                            assert_eq!((&this.groups[0]).name, new_name);
                                        }
                                    },
                                    true,
                                );
                                g.rename_group("first", "renamed");
                            }
                        }
                        'and_then_it_should_allow_renaming_a_node: {
                            'and_then_it_should_emit_an_event: {
                                g.connect(
                                    "rename_node",
                                    |this, value| {
                                        if let Some(payload) = value.as_object() {
                                            let old_name = payload["old"].as_str().unwrap();
                                            let new_name = payload["new"].as_str().unwrap();

                                            assert_eq!(old_name, "Foo".to_string());
                                            assert_eq!(new_name, "Baz".to_string());
                                        }
                                    },
                                    true,
                                );
                                g.rename_node("Foo", "Baz");

                                'and_then_it_should_be_available_with_the_new_name: {
                                    assert_ne!(g.get_node("Baz").is_none(), true);
                                }
                                'and_then_it_should_not_be_available_with_the_old_name: {
                                    assert_ne!(g.get_node("Foo").is_some(), true);
                                }
                                'and_then_it_should_have_the_edge_still_going_from_it: {
                                    let has_connection =
                                        g.edges.iter().any(|edge| edge.from.node_id == "Baz");
                                    assert_eq!(has_connection, true);
                                }
                                'and_then_it_should_still_be_exported: {
                                    assert_eq!(g.inports["inPut"].process, "Baz");
                                }
                                'and_then_it_should_still_be_grouped: {
                                    let has_group = g
                                        .groups
                                        .iter()
                                        .any(|group| group.nodes.contains(&String::from("Baz")));
                                    assert_eq!(has_group, true);
                                }
                                'and_then_it_should_not_be_grouped_with_old_name: {
                                    let has_group = g
                                        .groups
                                        .iter()
                                        .any(|group| group.nodes.contains(&String::from("Foo")));
                                    assert_ne!(has_group, true);
                                }
                                'and_then_it_should_not_have_edges_with_the_old_name: {
                                    let has_edges_with_old_name = g.edges.iter().any(|edge| {
                                        edge.from.node_id == "Foo" || edge.to.node_id == "Foo"
                                    });
                                    assert_ne!(has_edges_with_old_name, true);
                                }
                                'and_then_it_should_have_iips_still_going_into_it: {
                                    let has_iip = g
                                        .initializers
                                        .iter()
                                        .filter(|iip| iip.to.is_some())
                                        .any(|iip| iip.to.as_ref().unwrap().node_id == "Baz");
                                    assert_eq!(has_iip, true);
                                }
                                'and_then_it_should_not_have_iips_still_going_to_the_old_name: {
                                    let has_iip = g
                                        .initializers
                                        .iter()
                                        .filter(|iip| iip.to.is_some())
                                        .any(|iip| iip.to.as_ref().unwrap().node_id == "Foo");
                                    assert_ne!(has_iip, true);
                                }

                                'and_then_it_should_allow_renaming_an_inport: {
                                    'and_then_it_should_emit_an_event: {
                                        g.connect(
                                            "rename_inport",
                                            |this, value| {
                                                if let Some(payload) = value.as_object() {
                                                    let old_name = payload["old"].as_str().unwrap();
                                                    let new_name = payload["new"].as_str().unwrap();

                                                    assert_eq!(old_name, "inPut".to_string());
                                                    assert_eq!(new_name, "opt".to_string());

                                                    assert!(this
                                                        .inports
                                                        .contains_key(&String::from("opt")));
                                                    if let Some(opt) =
                                                        this.inports.get(&String::from("opt"))
                                                    {
                                                        assert_eq!(opt.process, "Baz".to_string());
                                                        assert_eq!(opt.port, "inPut".to_string());
                                                    }
                                                }
                                            },
                                            true,
                                        );
                                        g.rename_inport("inPut", "opt");
                                    }
                                }
                                'and_then_it_should_allow_renaming_an_outport: {
                                    'and_then_it_should_emit_an_event: {
                                        g.connect(
                                            "rename_outport",
                                            |this, value| {
                                                if let Some(payload) = value.as_object() {
                                                    let old_name = payload["old"].as_str().unwrap();
                                                    let new_name = payload["new"].as_str().unwrap();
                                                    assert_eq!(old_name, "outPut".to_string());
                                                    assert_eq!(new_name, "foo".to_string());

                                                    assert!(this
                                                        .outports
                                                        .contains_key(&String::from("foo")));
                                                    if let Some(opt) =
                                                        this.outports.get(&String::from("foo"))
                                                    {
                                                        assert_eq!(opt.process, "Bar".to_string());
                                                        assert_eq!(opt.port, "outPut".to_string());
                                                    }
                                                }
                                            },
                                            true,
                                        );
                                        g.rename_outport("outPut", "foo");
                                    }
                                }

                                'and_then_it_should_allow_removing_a_node: {
                                    'and_then_it_should_emit_an_event: {
                                        g.connect(
                                            "rename_outport",
                                            |this, value| {
                                                if let Ok(node) = GraphNode::deserialize(value) {
                                                    assert_eq!(node.id, String::from("Baz"));
                                                }
                                            },
                                            true,
                                        );
                                        g.remove_node("Baz");

                                        'and_then_it_should_not_have_edges_behind: {
                                            let has_connection = g.edges.iter().any(|edge| {
                                                edge.from.node_id == "Baz"
                                                    || edge.to.node_id == "Baz"
                                            });
                                            assert_ne!(has_connection, true);
                                        }
                                        'and_then_it_should_not_have_iips_behind: {
                                            let has_connection = g
                                                .initializers
                                                .iter()
                                                .filter(|iip| iip.to.is_some())
                                                .any(|iip| {
                                                    iip.to.as_ref().unwrap().node_id == "Baz"
                                                });
                                            assert_ne!(has_connection, true);
                                        }
                                        'and_then_it_should_not_be_grouped: {
                                            let has_group = g.groups.iter().any(|group| {
                                                group.nodes.contains(&String::from("Baz"))
                                            });
                                            assert_ne!(has_group, true);
                                        }
                                        'and_then_it_should_not_affect_other_group: {
                                            assert_eq!(
                                                g.groups.clone().first().unwrap().nodes.len(),
                                                2
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            'when_given_a_multiple_connected_array_ports: {
                let mut g = Graph::new("", true);
                g.add_node("Split1", "Split", None);
                g.add_node("Split2", "Split", None);
                g.add_node("Merge1", "Merge", None);
                g.add_node("Merge2", "Merge", None);
                g.add_edge("Split1", "out", "Merge1", "in", None);
                g.add_edge("Split1", "out", "Merge2", "in", None);
                g.add_edge("Split2", "out", "Merge1", "in", None);
                g.add_edge("Split2", "out", "Merge2", "in", None);

                'then_it_should_contain_four_nodes: {
                    assert!(g.nodes.len() == 4);
                }
                'then_it_should_contain_four_edges: {
                    assert!(g.edges.len() == 4);
                }
                'then_it_should_allow_a_specific_edge_to_be_removed: {
                    g.remove_edge("Split1", "out", Some("Merge2"), Some("in"));
                    assert_eq!(g.edges.len(), 3);

                    'and_then_it_should_not_contain_removed_connection_from_split1: {
                        let has_connection = g.edges.iter().any(|edge| {
                            edge.from.node_id == String::from("Split1")
                                && edge.to.node_id == String::from("Merge2")
                        });
                        assert!(has_connection == false);
                    }
                    'and_then_it_should_contain_the_other_connection_from_split1: {
                        let has_connection = g.edges.iter().any(|edge| {
                            edge.from.node_id == String::from("Split1")
                                && edge.to.node_id == String::from("Merge1")
                        });
                        assert!(has_connection);
                    }
                }
            }
            'when_given_an_initial_information_packet: {
                let mut g = Graph::new("", true);
                g.add_node("Split", "Split", None);
                g.add_initial(Value::String("Foo".to_string()), "Split", "in", None);

                'then_it_should_contain_one_node: {
                    assert_eq!(g.nodes.len(), 1);
                }
                'then_it_should_contain_no_edges: {
                    assert_eq!(g.edges.len(), 0);
                }
                'then_it_should_contain_one_iip: {
                    assert_eq!(g.initializers.len(), 1);
                }

                'then_when_removing_that_iip: {
                    'and_then_it_should_emit_an_event: {
                        g.connect(
                            "remove_initial",
                            |this, value| {
                                if let Ok(iip) = GraphIIP::deserialize(value) {
                                    if let Some(from) = &iip.from {
                                        assert_eq!(from.data, Value::String("Foo".to_string()));
                                    }
                                    if let Some(to) = &iip.to {
                                        assert_eq!(to.node_id, "Split");
                                        assert_eq!(to.port, "in");
                                    }
                                }
                            },
                            true,
                        );
                        g.remove_initial("Split", "in");
                        'and_then_it_should_contain_no_iips: {
                            assert_eq!(g.initializers.len(), 0);
                        }
                    }
                }
            }
            'when_given_an_inport_initial_information_packet: {
                let mut g = Graph::new("", true);
                g.add_node("Split", "Split", None);
                g.add_inport("testinport", "Split", "in", None);
                g.add_graph_initial(Value::String("Foo".to_string()), "testinport", None);

                'then_it_should_contain_one_node: {
                    assert_eq!(g.nodes.len(), 1);
                }
                'then_it_should_contain_no_edges: {
                    assert_eq!(g.edges.len(), 0);
                }
                'then_it_should_contain_one_iip_for_the_correct_node: {
                    assert_eq!(g.initializers.len(), 1);
                    assert_eq!(
                        g.initializers.first().unwrap().from.as_ref().unwrap().data,
                        Value::String("Foo".to_string())
                    );
                    assert_eq!(
                        g.initializers.first().unwrap().to.as_ref().unwrap().node_id,
                        "Split"
                    );
                    assert_eq!(
                        g.initializers.first().unwrap().to.as_ref().unwrap().port,
                        "in"
                    );
                }

                'then_when_removing_that_iip: {
                    'and_then_it_should_emit_an_event: {
                        g.connect(
                            "remove_initial",
                            |this, value| {
                                if let Ok(iip) = GraphIIP::deserialize(value) {
                                    if let Some(from) = &iip.from {
                                        assert_eq!(from.data, Value::String("Foo".to_string()));
                                    }
                                    if let Some(to) = &iip.to {
                                        assert_eq!(to.node_id, "Split");
                                        assert_eq!(to.port, "in");
                                    }
                                }
                            },
                            true,
                        );
                        g.remove_graph_initial("testinport");

                        'and_then_it_should_contain_no_iips: {
                            assert_eq!(g.initializers.len(), 0);
                        }

                        'and_then_when_adding_iip_for_a_non_existent_inport: {
                            g.add_graph_initial(
                                Value::String("Bar".to_string()),
                                "nonexistent",
                                None,
                            );

                            'and_then_it_should_not_add_any_iip: {
                                assert_eq!(g.initializers.len(), 0);
                            }
                        }
                    }
                }
            }

            'when_given_an_indexed_inport_initial_information_packet: {
                let mut g = Graph::new("", true);
                g.add_node("Split", "Split", None);
                g.add_inport("testinport", "Split", "in", None);
                g.add_graph_initial_index(
                    Value::String("Foo".to_string()),
                    "testinport",
                    Some(1),
                    None,
                );

                'then_it_should_contain_one_node: {
                    assert_eq!(g.nodes.len(), 1);
                }
                'then_it_should_contain_no_edges: {
                    assert_eq!(g.edges.len(), 0);
                }
                'then_it_should_contain_one_iip_for_the_correct_node: {
                    assert_eq!(g.initializers.len(), 1);
                    assert_eq!(
                        g.initializers.first().unwrap().from.as_ref().unwrap().data,
                        Value::String("Foo".to_string())
                    );
                    assert_eq!(
                        g.initializers.first().unwrap().to.as_ref().unwrap().node_id,
                        "Split"
                    );
                    assert_eq!(
                        g.initializers.first().unwrap().to.as_ref().unwrap().port,
                        "in"
                    );
                    assert_eq!(
                        g.initializers.first().unwrap().to.as_ref().unwrap().index,
                        Some(1)
                    );
                }

                'then_when_removing_that_iip: {
                    'and_then_it_should_emit_an_event: {
                        g.connect(
                            "remove_initial",
                            |this, value| {
                                if let Ok(iip) = GraphIIP::deserialize(value) {
                                    if let Some(from) = &iip.from {
                                        assert_eq!(from.data, Value::String("Foo".to_string()));
                                    }
                                    if let Some(to) = &iip.to {
                                        assert_eq!(to.node_id, "Split");
                                        assert_eq!(to.port, "in");
                                    }
                                }
                            },
                            true,
                        );
                        g.remove_graph_initial("testinport");

                        'and_then_it_should_contain_no_iips: {
                            assert_eq!(g.initializers.len(), 0);
                        }

                        'and_then_when_adding_iip_for_a_non_existent_inport: {
                            g.add_graph_initial_index(
                                Value::String("Bar".to_string()),
                                "nonexistent",
                                Some(1),
                                None,
                            );

                            'and_then_it_should_not_add_any_iip: {
                                assert_eq!(g.initializers.len(), 0);
                            }
                        }
                    }
                }
            }
            'when_given_no_nodes: {
                let mut g = Graph::new("", true);
                'then_it_should_not_allow_adding_edges: {
                    g.add_edge("Foo", "out", "Bar", "in", None);
                    assert_eq!(g.edges.len(), 0);
                }
                'then_it_should_not_allow_adding_iips: {
                    g.add_initial(Value::String("Hello".to_string()), "Bar", "in", None);
                    assert_eq!(g.initializers.len(), 0);
                }
            }
            'when_saving_and_loading_files: {
                'then_when_saving_with_json_file_extension: {
                    let current_dir = std::env::current_dir()
                        .expect("Failed to get current dir")
                        .join("foo.json");
                    let graph_path = current_dir.to_str().unwrap();

                    'and_then_it_should_be_able_save_file: {
                        let mut g = Graph::new("", true);
                        g.add_node("Foo", "Bar", None);

                        assert_eq!(g.save(graph_path).is_err(), false);
                        assert_eq!(std::fs::File::open(graph_path).is_err(), false);

                        'and_then_it_should_be_possible_to_load_a_graph_from_file: {
                            if let Ok(result) = Graph::load_file(graph_path, None) {
                                let original_graph = g.to_json();
                                assert_json_eq!(json!(result.to_json()), json!(original_graph));
                            } else {
                                panic!("It did not load file")
                            }

                            let _ = std::fs::remove_file(graph_path);
                        }
                    }
                }
            }
        }
        'given_without_case_sensitivity: {
            'then_graph_operations_should_convert_port_names_to_lowercase: {
                let mut g = Graph::new("Hola", false);
                assert!(g.case_sensitive == false);

                'and_then_it_should_have_case_insensitive_ports_on_the_edges: {
                    g.connect(
                        "add_edge",
                        |this, value| {
                            if let Ok(edge) = GraphEdge::deserialize(value) {
                                assert_eq!(edge.from.node_id, "Foo");
                                assert_eq!(edge.to.port, "input");
                                assert_eq!(edge.from.port, "output");
                            }
                        },
                        true,
                    );
                    g.add_node("Foo", "foo", None)
                        .add_node("Bar", "bar", None)
                        .add_edge("Foo", "outPut", "Bar", "inPut", None);
                }
            }
        }
    }
}
