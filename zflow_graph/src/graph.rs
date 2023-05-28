///    FBP Graph Journal
///    (c) 2022-2023 Damilare Akinlaja
///    (c) 2013-2020 Flowhub UG
///    (c) 2011-2012 Henri Bergius, Nemein
///    FBP Graph may be freely distributed under the MIT license
use crate::types::{GraphEvents, TransactionId};
use foreach::ForEach;
use rayon::iter::ParallelIterator;
use rayon::prelude::IntoParallelRefIterator;
use redux_rs::Store;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

use super::journal::TransactionEntry;
use super::types::{
    GraphEdge, GraphEdgeJson, GraphExportedPort, GraphGroup, GraphIIP, GraphJson, GraphLeaf,
    GraphLeafJson, GraphNode, GraphNodeJson, GraphStub, GraphTransaction,
};

use rayon::iter::IntoParallelRefMutIterator;

#[derive(Clone, Default)]
pub struct GraphState {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub initializers: Vec<GraphIIP>,
    pub groups: Vec<GraphGroup>,
    pub inports: HashMap<String, GraphExportedPort>,
    pub outports: HashMap<String, GraphExportedPort>,
    pub properties: Map<String, Value>,
    pub transaction: Option<GraphTransaction>,
    pub last_revision: i32,
    pub transactions: Vec<Vec<TransactionEntry>>,
    pub subscribed: bool,
    pub current_revision: i32,
    pub reducers: Arc<Mutex<Vec<Box<dyn Fn(GraphState, GraphEvents)>>>>,
    pub last_action: GraphEvents, // pub(crate) entries: Vec<TransactionEntry>,
}

impl Debug for GraphState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphState")
            .field("nodes", &self.nodes)
            .field("edges", &self.edges)
            .field("initializers", &self.initializers)
            .field("groups", &self.groups)
            .field("inports", &self.inports)
            .field("outports", &self.outports)
            .field("properties", &self.properties)
            .field("transaction", &self.transaction)
            .field("last_revision", &self.last_revision)
            .field("transactions", &self.transactions)
            .field("subscribed", &self.subscribed)
            .field("current_revision", &self.current_revision)
            .field("reducers", &"[_]")
            .finish()
    }
}

unsafe impl Send for GraphState {}
unsafe impl Sync for GraphState {}
unsafe impl Send for GraphEvents {}
unsafe impl Sync for GraphEvents {}

/// This class represents an abstract FBP graph containing nodes
/// connected to each other with edges.
/// These graphs can be used for visualization and sketching, but
/// also are the way to start an FBP network.
pub struct Graph {
    pub name: String,
    pub(crate) store:
        Store<GraphState, GraphEvents, Box<dyn Fn(GraphState, GraphEvents) -> GraphState + Send>>,
    pub case_sensitive: bool,
    pub(crate) entries: Vec<TransactionEntry>,
}

impl Graph {
    pub fn new(name: &str, case_sensitive: bool) -> Self {
        Self {
            name: name.to_owned(),
            case_sensitive,
            // Redux store mutator
            store: Store::new(Box::new(
                |mut state: GraphState, action: GraphEvents| match action {
                    GraphEvents::AddNode(node) => GraphState {
                        nodes: {
                            state.nodes.push(node);
                            state.nodes
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::RemoveNode(id) => GraphState {
                        nodes: state
                            .nodes
                            .iter()
                            .filter(|node| node.id != id)
                            .map(|n| *n)
                            .collect(),
                        last_action: action,
                        ..state
                    },
                    GraphEvents::RenameNode {
                        node,
                        old_id,
                        new_id,
                    } => {
                        state.nodes = {
                            state.nodes.par_iter_mut().for_each(|node| {
                                if node.id == old_id {
                                    node.id = new_id;
                                }
                                node.id = new_id.to_owned();

                                state.edges.par_iter_mut().for_each(|edge| {
                                    if edge.from.node_id == old_id.to_owned() {
                                        (*edge).from.node_id = new_id.to_owned()
                                    }
                                    if edge.to.node_id == old_id.to_owned() {
                                        (*edge).to.node_id = new_id.to_owned()
                                    }
                                });

                                state.initializers.par_iter_mut().for_each(|iip| {
                                    if let Some(to) = iip.to.as_mut() {
                                        if to.node_id == old_id.to_owned() {
                                            (*to).node_id = new_id.to_owned()
                                        }
                                    }
                                });

                                state.inports.par_iter_mut().for_each(|(_, port)| {
                                    if port.process == old_id.to_owned() {
                                        port.process = new_id.to_owned();
                                    }
                                });
                                state.outports.par_iter_mut().for_each(|(_, port)| {
                                    if port.process == old_id.to_owned() {
                                        port.process = new_id.to_owned();
                                    }
                                });

                                state.groups.par_iter_mut().for_each(|group| {
                                    if let Some(index) = group
                                        .nodes
                                        .iter()
                                        .position(|n| n.to_owned() == old_id.to_owned())
                                    {
                                        group.nodes[index] = new_id.to_owned();
                                    }
                                });
                            });
                            state.nodes
                        };
                        state.last_action = action;
                        state
                    }
                    GraphEvents::ChangeNode {
                        mut node,
                        old_metadata,
                        new_metadata,
                        index,
                    } => GraphState {
                        nodes: {
                            if node.metadata.is_none() {
                                node.metadata = Some(Map::new());
                            }

                            if new_metadata.keys().len() == 0 {
                                node.metadata = Some(Map::new());
                            }

                            new_metadata.keys().for_each(|item| {
                                let meta = new_metadata.clone();
                                let val = meta.get(item);
                                if let Some(existing_meta) = node.metadata.as_mut() {
                                    if let Some(val) = val {
                                        (*existing_meta).insert(item.clone(), val.clone());
                                    } else {
                                        (*existing_meta).remove(item);
                                    }
                                }
                            });

                            state.nodes.insert(index, node);
                            state.nodes
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::AddEdge(edge) => GraphState {
                        edges: {
                            state.edges.push(edge);
                            state.edges
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::RemoveEdge(edge) => GraphState {
                        edges: {
                            let out_port = edge.from.port;
                            let in_port = edge.to.port;
                            let node2 = edge.to.node_id;
                            let node = edge.from.node_id;
                            state
                                .edges
                                .par_iter()
                                .filter(|edge| {
                                    if edge.from.node_id == node
                                        && edge.from.port == out_port
                                        && edge.to.node_id == node2
                                        && edge.to.port == in_port
                                    {
                                        return false;
                                    }
                                    true
                                })
                                .map(|edge| *edge)
                                .collect()
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::ChangeEdge {
                        edge: new_edge,
                        old_metadata,
                        new_metadata,
                        index,
                    } => GraphState {
                        edges: {
                            state.edges[index] = new_edge;
                            state.edges
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::AddInitial(_) => todo!(),
                    GraphEvents::RemoveInitial { id, port } => GraphState {
                        initializers: {
                            state
                                .initializers
                                .par_iter()
                                .filter(|iip| {
                                    if let Some(to) = iip.to.clone() {
                                        if to.node_id.as_str() == id && to.port == port {
                                            false;
                                        }
                                    }
                                    true
                                })
                                .map(|iip| *iip)
                                .collect()
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::ChangeProperties(_) => todo!(),
                    GraphEvents::AddGroup(_) => todo!(),
                    GraphEvents::RemoveGroup(_) => todo!(),
                    GraphEvents::RenameGroup(_) => todo!(),
                    GraphEvents::ChangeGroup(_) => todo!(),
                    GraphEvents::AddInport { inport, name } => GraphState {
                        inports: {
                            state.inports.insert(name, inport);
                            state.inports
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::RemoveInport(port) => GraphState {
                        inports: {
                            state.inports.remove(&port);
                            state.inports
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::RenameInport { old_name, new_name } => GraphState {
                        inports: {
                            if let Some(port) = state.inports.remove(&old_name) {
                                state.inports.insert(new_name, port.clone());
                            }
                            state.inports
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::ChangeInport {
                        name,
                        mut port,
                        old_metadata,
                        new_metadata,
                    } => GraphState {
                        inports: {
                            port.metadata = Some(new_metadata);
                            state.inports.insert(name, port);
                            state.inports
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::AddOutport { outport, name } => GraphState {
                        outports: {
                            state.outports.insert(name, outport);
                            state.outports
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::RemoveOutport(port) => GraphState {
                        outports: {
                            state.outports.remove(&port);
                            state.outports
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::RenameOutport { old_name, new_name } => GraphState {
                        outports: {
                            if let Some(port) = state.outports.remove(&old_name) {
                                state.outports.insert(new_name, port.clone());
                            }
                            state.outports
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::ChangeOutport {
                        name,
                        mut port,
                        old_metadata,
                        new_metadata,
                    } => GraphState {
                        outports: {
                            port.metadata = Some(new_metadata);
                            state.outports.insert(name, port);
                            state.outports
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::StartTransaction(id, metadata) => GraphState {
                        transaction: {
                            if state.transaction.is_some() {
                                panic!("Nested transactions not supported")
                            }
                            Some(GraphTransaction {
                                id: id.0.to_owned(),
                                depth: 1,
                                metadata,
                            })
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::IncTransaction => GraphState {
                        transaction: if let Some(transaction) = state.transaction {
                            Some(GraphTransaction {
                                depth: transaction.depth + 1,
                                ..transaction
                            })
                        } else {
                            None
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::DecTransaction => GraphState {
                        transaction: if let Some(transaction) = state.transaction {
                            Some(GraphTransaction {
                                depth: transaction.depth - 1,
                                ..transaction
                            })
                        } else {
                            None
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::EndTransaction(id, metadata) => GraphState {
                        transaction: {
                            if state.transaction.is_none() {
                                panic!("Attempted to end non-existing transaction")
                            }
                            None
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::RemoveNodeFromGroup { group, node_index } => GraphState {
                        groups: {
                            state.groups.par_iter_mut().for_each(|_group| {
                                if _group.name == group {
                                    _group.nodes.remove(node_index);
                                }
                            });
                            if let Some((i, group)) = state
                                .groups
                                .iter_mut()
                                .enumerate()
                                .find(|(i, _group)| _group.name == group)
                            {
                                if group.nodes.is_empty() {
                                    state.groups.remove(i);
                                }
                            }
                            state.groups
                        },
                        last_action: action,
                        ..state
                    },
                    GraphEvents::Transaction(_) => todo!(),
                    GraphEvents::Noop => state,
                },
            )),
            entries: Vec::new(),
        }
    }

    pub fn get_port_name(&self, port: &str) -> String {
        if self.case_sensitive {
            return port.to_string();
        }
        port.to_lowercase()
    }

    pub(crate) async fn check_transaction_start(&self) {
        match self
            .store
            .select(|state: &GraphState| state.transaction.clone())
            .await
        {
            None => {
                self.store
                    .dispatch(GraphEvents::StartTransaction(
                        TransactionId("implicit"),
                        Value::Null,
                    ))
                    .await;
            }
            Some(transaction) => {
                if transaction.id == "implicit" {
                    self.store.dispatch(GraphEvents::IncTransaction).await;
                }
            }
        }
    }
    pub(crate) async fn check_transaction_end(&self) {
        if let Some(transaction) = self
            .store
            .select(|state: &GraphState| state.transaction.clone())
            .await
        {
            if transaction.id == "implicit" {
                self.store.dispatch(GraphEvents::DecTransaction).await;
            }

            if transaction.depth == 0 {
                self.store
                    .dispatch(GraphEvents::EndTransaction(
                        TransactionId("implicit"),
                        Value::Null,
                    ))
                    .await;
            }
        }
    }

    /// Adding a node to the graph
    /// Nodes are identified by an ID unique to the graph. Additionally,
    /// a node may contain information on what FBP component it is and
    /// possible display coordinates.
    /// ```no_run
    /// let mut metadata = Map::new();
    /// metadata.insert("x".to_string(), 91);
    /// metadata.insert("y".to_string(), 154);
    /// my_graph.add_node("Read", "ReadFile", Some(metadata));
    /// ```
    pub fn add_node(
        &self,
        id: &str,
        component: &str,
        metadata: Option<Map<String, Value>>,
    ) -> &Self {
        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            self.check_transaction_start().await;
            self.store
                .dispatch(GraphEvents::AddNode(GraphNode {
                    id: id.to_owned(),
                    component: component.to_owned(),
                    metadata,
                }))
                .await;
            self.check_transaction_end().await;
        });
        self
    }

    /// Removing a node from the graph
    /// Existing nodes can be removed from a graph by their ID. This
    /// will remove the node and also remove all edges connected to it.
    /// ```no_run
    /// my_graph.remove_node("Read");
    /// ```
    /// Once the node has been removed, the `remove_node` event will be
    pub fn remove_node(&self, id: &str) -> &Self {
        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            self.check_transaction_start().await;
            if let Some(node) = self
                .store
                .select(|state: &GraphState| state.nodes.iter().find(|n| n.id == id.to_owned()))
                .await
            {
                let edges = self.store.select(|state: &GraphState| state.edges).await;
                edges.iter().for_each(|edge| {
                    if (edge.from.node_id == node.id) || (edge.to.node_id == node.id) {
                        self.remove_edge(
                            edge.from.node_id.as_str(),
                            edge.from.port.as_str(),
                            Some(edge.to.node_id.as_str()),
                            Some(edge.to.port.as_str()),
                        );
                    }
                });
                let iips = self
                    .store
                    .select(|state: &GraphState| state.initializers)
                    .await;

                iips.iter().for_each(|iip| {
                    if let Some(to) = iip.to {
                        if to.node_id == node.id {
                            self.store.dispatch(GraphEvents::RemoveInitial {
                                id: node.id,
                                port: to.port,
                            });
                        }
                    }
                });
                if let Some((_, inport)) = self
                    .store
                    .select(|state: &GraphState| state.inports.iter().find(|(port, _)| id == *port))
                    .await
                {
                    self.store
                        .dispatch(GraphEvents::RemoveInport(inport.port))
                        .await;
                }

                if let Some((_, outport)) = self
                    .store
                    .select(|state: &GraphState| {
                        state.outports.iter().find(|(port, _)| id == *port)
                    })
                    .await
                {
                    self.store
                        .dispatch(GraphEvents::RemoveOutport(outport.port))
                        .await;
                }

                let v = self
                    .store
                    .select(|state: &GraphState| {
                        state
                            .groups
                            .iter()
                            .map(|group| {
                                (group.name, group.nodes.iter().position(|node| *node == id))
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;

                for (group, Some(node_index)) in v {
                    self.store
                        .dispatch(GraphEvents::RemoveNodeFromGroup { group, node_index })
                        .await;
                }
            }
            self.check_transaction_end().await;
        });
        self
    }

    /// Renaming a node
    ///
    /// Nodes IDs can be changed by calling this method.
    pub fn rename_node(&self, old_id: &str, new_id: &str) -> &Self {
        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            if let Some(node) = self
                .store
                .select(|state: &GraphState| state.nodes.iter().find(|n| n.id == old_id.to_owned()))
                .await
            {
                self.check_transaction_start().await;
                self.store.dispatch(GraphEvents::RenameNode {
                    node: *node,
                    old_id: old_id.to_owned(),
                    new_id: new_id.to_owned(),
                });
                self.check_transaction_end().await;
            }
        });
        self
    }

    pub fn set_node_metadata(&self, id: &str, metadata: Map<String, Value>) -> &Self {
        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            if let Some(node) = self
                .store
                .select(|state: &GraphState| state.nodes.iter().find(|n| n.id == id.to_owned()))
                .await
            {
                self.check_transaction_start().await;
                let before = node.metadata.clone();
                let index = self
                    .store
                    .select(|state: &GraphState| {
                        state.nodes.iter().position(|n| n.id == id.to_owned())
                    })
                    .await
                    .unwrap();
                self.store
                    .dispatch(GraphEvents::ChangeNode {
                        node: node.clone(),
                        old_metadata: before,
                        new_metadata: metadata,
                        index,
                    })
                    .await;
                self.check_transaction_end().await;
            }
        });
        self
    }

    /// Removing Initial Information Packets
    ///
    /// IIPs can be removed by calling the `remove_initial` method.
    /// ```no_run
    /// my_graph.remove_initial("Read", "source");
    /// ```
    /// If the IIP was applied via the `add_graph_initial` or
    /// `add_graph_initial_index` functions, it can be removed using
    /// the `remove_graph_initial` method.
    /// ```no_run
    /// my_graph.remove_graph_initial("file");
    /// ```
    /// Remove an IIP will emit a `remove_initial` event.
    pub fn remove_initial(&self, id: &str, port: &str) -> &Self {
        let port_name = self.get_port_name(port);
        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            self.check_transaction_start().await;
            self.store.dispatch(GraphEvents::RemoveInitial {
                id: id.to_owned(),
                port: port_name,
            });
            self.check_transaction_end().await;
        });

        self.store.dispatch(GraphEvents::RemoveInitial {
            id: id.to_owned(),
            port: port.to_owned(),
        });
        self
    }

    /// Connecting nodes
    ///
    /// Nodes can be connected by adding edges between a node's outport
    ///	and another node's inport:
    /// ```no_run
    /// my_graph.add_edge("Read", "out", "Display", "in", None);
    /// my_graph.add_edge_index("Read", "out", None, "Display", "in", Some(2), None);
    /// ```
    /// Adding an edge will an event.
    pub fn add_edge(
        &self,
        out_node: &str,
        out_port: &str,
        in_node: &str,
        in_port: &str,
        mut metadata: Option<Map<String, Value>>,
    ) -> &Self {
        if metadata.is_none() {
            metadata = Some(Map::new());
        }
        let out_port_name = self.get_port_name(out_port);
        let in_port_name = self.get_port_name(in_port);

        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            let some = self
                .store
                .select(|store: &GraphState| store.edges)
                .await
                .iter()
                .find(|edge| {
                    edge.from.node_id.as_str() == out_node
                        && edge.from.port == out_port
                        && edge.to.node_id.as_str() == in_node
                        && edge.to.port == in_port
                });

            if some.is_some() {
                return;
            }
            if self
                .store
                .select(|state: &GraphState| {
                    state.nodes.iter().find(|n| n.id == out_node.to_owned())
                })
                .await
                .is_none()
            {
                return;
            }

            if self
                .store
                .select(|state: &GraphState| {
                    state.nodes.iter().find(|n| n.id == in_node.to_owned())
                })
                .await
                .is_none()
            {
                return;
            }

            self.check_transaction_start().await;
            let edge = GraphEdge {
                from: GraphLeaf {
                    port: out_port_name,
                    node_id: out_node.to_owned(),
                    index: None,
                },
                to: GraphLeaf {
                    port: in_port_name,
                    node_id: in_node.to_owned(),
                    index: None,
                },
                metadata,
            };

            self.store.dispatch(GraphEvents::AddEdge(edge)).await;
            self.check_transaction_end().await;
        });
        self
    }

    pub fn add_edge_index(
        &mut self,
        out_node: &str,
        out_port: &str,
        index_1: Option<usize>,
        in_node: &str,
        in_port: &str,
        index_2: Option<usize>,
        mut metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        if metadata.is_none() {
            metadata = Some(Map::new());
        }
        let out_port_name = self.get_port_name(out_port);
        let in_port_name = self.get_port_name(in_port);
        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            let some = self
                .store
                .select(|store: &GraphState| store.edges)
                .await
                .iter()
                .find(|edge| {
                    edge.from.node_id.as_str() == out_node
                        && edge.from.port == out_port
                        && edge.to.node_id.as_str() == in_node
                        && edge.to.port == in_port
                });

            if some.is_some() {
                return;
            }
            if self
                .store
                .select(|state: &GraphState| {
                    state.nodes.iter().find(|n| n.id == out_node.to_owned())
                })
                .await
                .is_none()
            {
                return;
            }

            if self
                .store
                .select(|state: &GraphState| {
                    state.nodes.iter().find(|n| n.id == in_node.to_owned())
                })
                .await
                .is_none()
            {
                return;
            }
            self.check_transaction_start().await;
            let edge = GraphEdge {
                from: GraphLeaf {
                    port: out_port_name.to_owned(),
                    node_id: out_node.to_owned(),
                    index: index_1,
                },
                to: GraphLeaf {
                    port: in_port_name.to_owned(),
                    node_id: in_node.to_owned(),
                    index: index_2,
                },
                metadata,
            };

            self.store.dispatch(GraphEvents::AddEdge(edge)).await;
            self.check_transaction_end().await;
        });
        self
    }

    /// Disconnected nodes
    ///
    /// Connections between nodes can be removed by providing the
    ///	nodes and ports to disconnect.
    /// ```no_run
    /// my_graph.remove_edge("Display", "out", "Foo", "in");
    /// ```
    /// Removing a connection will emit the `removeEdge` event.
    pub fn remove_edge(
        &self,
        node: &str,
        port: &str,
        node2: Option<&str>,
        port2: Option<&str>,
    ) -> &Self {
        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            let out_port = self.get_port_name(port);
            let in_port = self.get_port_name(port2.unwrap_or(""));

            if let Some(_) = self
                .store
                .select(|store: &GraphState| store.edges)
                .await
                .iter()
                .find(|edge| {
                    edge.from.node_id.as_str() == node
                        && edge.from.port == out_port
                        && edge.to.node_id.as_str() == node2.unwrap_or("")
                        && edge.to.port == in_port
                })
            {
                self.check_transaction_start().await;
                let out_port = self.get_port_name(port);
                let mut in_port = None;
                if let Some(port2) = port2 {
                    in_port = Some(self.get_port_name(port2));
                }

                self.store
                    .select(|store: &GraphState| store.edges)
                    .await
                    .iter()
                    .for_each(|edge| {
                        if in_port.is_some() && node2.is_some() {
                            if edge.from.node_id.as_str() == node
                                && edge.from.port == out_port
                                && edge.to.node_id.as_str() == node2.unwrap()
                                && edge.to.port == in_port.clone().unwrap()
                            {
                                self.set_edge_metadata(
                                    edge.from.node_id.as_str(),
                                    edge.from.port.as_str(),
                                    edge.to.node_id.as_str(),
                                    edge.to.port.as_str(),
                                    Map::new(),
                                );

                                self.store.dispatch(GraphEvents::RemoveEdge(*edge));
                                return;
                            }
                        } else if (edge.from.node_id.as_str() == node && edge.from.port == out_port)
                            || (edge.to.node_id.as_str() == node && edge.to.port == out_port)
                        {
                            self.set_edge_metadata(
                                edge.from.node_id.as_str(),
                                edge.from.port.as_str(),
                                edge.to.node_id.as_str(),
                                edge.to.port.as_str(),
                                Map::new(),
                            );
                        }
                    });
                self.check_transaction_end().await;
            }
        });

        self
    }

    /// Changing an edge's metadata
    ///
    /// Edge metadata can be set or changed by calling this method.
    pub fn set_edge_metadata(
        &self,
        node: &str,
        port: &str,
        node2: &str,
        port2: &str,
        metadata: Map<String, Value>,
    ) -> &Self {
        let handle = Runtime::new().expect("expected to start tokio runtime");
        let out_port = self.get_port_name(port);
        let in_port = self.get_port_name(port2);

        handle.block_on(async {
            if let Some(ref mut edge) = self
                .store
                .select(|state: &GraphState| {
                    state.edges.iter().find(|edge| {
                        edge.from.node_id.as_str() == node
                            && edge.from.port == out_port
                            && edge.to.node_id.as_str() == node2
                            && edge.to.port == in_port
                    })
                })
                .await
            {
                self.check_transaction_start().await;
                if edge.metadata.is_none() {
                    edge.metadata = Some(Map::new());
                }
                let before = edge.metadata.clone();
                for item in metadata.clone().keys() {
                    let val = metadata.get(item);
                    if let Some(edge_metadata) = edge.metadata.as_mut() {
                        if let Some(val) = val {
                            (*edge_metadata).insert(item.clone(), val.clone());
                        } else {
                            (*edge_metadata).remove(item);
                        }
                    }
                }

                let edge_index = self
                    .store
                    .select(|state: &GraphState| {
                        state
                            .edges
                            .iter()
                            .position(|edge| {
                                edge.from.node_id.as_str() == node
                                    && edge.from.port == port
                                    && edge.to.node_id.as_str() == node2
                                    && edge.to.port == port2
                            })
                            .unwrap()
                    })
                    .await;

                self.store.dispatch(GraphEvents::ChangeEdge {
                    edge: edge.clone(),
                    old_metadata: before,
                    new_metadata: metadata,
                    index: edge_index,
                }).await;

                self.check_transaction_end().await;
            }
        });
        self
    }

    pub fn add_inport(
        &mut self,
        public_port: &str,
        node_key: &str,
        port_key: &str,
        metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        let port_name = self.get_port_name(public_port);

        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            // Check that node exists
            if let Some(node) = self
                .store
                .select(|state: &GraphState| {
                    state.nodes.iter().find(|n| n.id == node_key.to_owned())
                })
                .await
            {
                self.check_transaction_start().await;
                let val = GraphExportedPort {
                    process: node_key.to_owned(),
                    port: self.get_port_name(port_key),
                    metadata,
                };
                self.store
                    .dispatch(GraphEvents::AddInport {
                        name: port_name,
                        inport: val,
                    })
                    .await;
                self.check_transaction_end().await;
            }
        });
        self
    }

    pub fn remove_inport(&mut self, public_port: &str) -> &mut Self {
        let port_name = self.get_port_name(public_port);
        let handle = Runtime::new().expect("expected to start tokio runtime");

        handle.block_on(async {
            if !self
                .store
                .select(|state: &GraphState| state.inports.contains_key(&port_name))
                .await
            {
                return;
            }

            self.check_transaction_start().await;

            self.set_inports_metadata(port_name.as_str(), Map::new());

            self.store
                .dispatch(GraphEvents::RemoveInport(public_port.to_owned()))
                .await;

            self.check_transaction_end().await;
        });

        self
    }

    pub fn set_inports_metadata(&self, public_port: &str, metadata: Map<String, Value>) -> &Self {
        let port_name = self.get_port_name(public_port);
        let handle = Runtime::new().expect("expected to start tokio runtime");

        handle.block_on(async {
            if !self
                .store
                .select(|state: &GraphState| state.inports.contains_key(&port_name))
                .await
            {
                return;
            }
            self.check_transaction_start().await;

            if let Some(p) = self
                .store
                .select(|state: &GraphState| state.inports.get(&port_name))
                .await
            {
                let mut p = p.clone();
                if p.metadata.is_none() {
                    p.metadata = Some(Map::new());
                }

                let before = p.metadata.clone();

                metadata.clone().keys().foreach(|item, _| {
                    let meta = metadata.clone();
                    let val = meta.get(item);
                    let mut existing_meta = p.metadata.clone();
                    if let Some(existing_meta) = existing_meta.as_mut() {
                        if let Some(val) = val {
                            existing_meta.insert(item.clone(), val.clone());
                        } else {
                            existing_meta.remove(item);
                        }
                        p.metadata = Some(existing_meta.clone());
                        // self.inports.insert(port_name.clone(), p.clone());
                    } else {
                        // iter.next();
                        return;
                    }
                });
                self.store
                    .dispatch(GraphEvents::ChangeInport {
                        name: port_name.clone(),
                        port: p.clone(),
                        old_metadata: before,
                        new_metadata: metadata,
                    })
                    .await;
            }

            self.check_transaction_end().await;
        });

        self
    }

    pub fn rename_inport(&self, old_port: &str, new_port: &str) -> &Self {
        let old_port_name = self.get_port_name(old_port);
        let new_port_name = self.get_port_name(new_port);
        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            if !self
                .store
                .select(|state: &GraphState| state.inports.contains_key(&old_port_name))
                .await
            {
                return;
            }

            if new_port_name == old_port_name {
                return;
            }

            self.check_transaction_start().await;

            self.store
                .dispatch(GraphEvents::RenameInport {
                    old_name: old_port_name,
                    new_name: new_port_name,
                })
                .await;

            self.check_transaction_end().await;
        });

        self
    }

    pub fn remove_outport(&mut self, public_port: &str) -> &mut Self {
        let port_name = self.get_port_name(public_port);
        let handle = Runtime::new().expect("expected to start tokio runtime");

        handle.block_on(async {
            if !self
                .store
                .select(|state: &GraphState| state.outports.contains_key(&port_name))
                .await
            {
                return;
            }

            self.check_transaction_start().await;

            self.set_outports_metadata(port_name.as_str(), Map::new());

            self.store
                .dispatch(GraphEvents::RemoveOutport(public_port.to_owned()))
                .await;

            self.check_transaction_end().await;
        });

        self
    }

    pub fn set_outports_metadata(&self, public_port: &str, metadata: Map<String, Value>) -> &Self {
        let port_name = self.get_port_name(public_port);
        let handle = Runtime::new().expect("expected to start tokio runtime");

        handle.block_on(async {
            if !self
                .store
                .select(|state: &GraphState| state.outports.contains_key(&port_name))
                .await
            {
                return;
            }
            self.check_transaction_start().await;

            if let Some(p) = self
                .store
                .select(|state: &GraphState| state.outports.get(&port_name))
                .await
            {
                let mut p = p.clone();
                if p.metadata.is_none() {
                    p.metadata = Some(Map::new());
                }

                let before = p.metadata.clone();

                metadata.clone().keys().foreach(|item, _| {
                    let meta = metadata.clone();
                    let val = meta.get(item);
                    let mut existing_meta = p.metadata.clone();
                    if let Some(existing_meta) = existing_meta.as_mut() {
                        if let Some(val) = val {
                            existing_meta.insert(item.clone(), val.clone());
                        } else {
                            existing_meta.remove(item);
                        }
                        p.metadata = Some(existing_meta.clone());
                    } else {
                        return;
                    }
                });
                self.store
                    .dispatch(GraphEvents::ChangeOutport {
                        name: port_name.clone(),
                        port: p.clone(),
                        old_metadata: before,
                        new_metadata: metadata,
                    })
                    .await;
            }

            self.check_transaction_end().await;
        });

        self
    }

    pub fn rename_outport(&self, old_port: &str, new_port: &str) -> &Self {
        let old_port_name = self.get_port_name(old_port);
        let new_port_name = self.get_port_name(new_port);
        let handle = Runtime::new().expect("expected to start tokio runtime");
        handle.block_on(async {
            if !self
                .store
                .select(|state: &GraphState| state.outports.contains_key(&old_port_name))
                .await
            {
                return;
            }

            if new_port_name == old_port_name {
                return;
            }

            self.check_transaction_start().await;

            self.store
                .dispatch(GraphEvents::RenameOutport {
                    old_name: old_port_name,
                    new_name: new_port_name,
                })
                .await;

            self.check_transaction_end().await;
        });

        self
    }
}


//     /// Adding Initial Information Packets
//     ///
//     /// Initial Information Packets (IIPs) can be used for sending data
//     /// to specified node inports without a sending node instance.
//     ///
//     /// IIPs are especially useful for sending configuration information
//     /// to components at FBP network start-up time. This could include
//     /// filenames to read, or network ports to listen to.
//     ///
//     /// ```no_run
//     /// my_graph.add_initial("somefile.txt", "Read", "source", None);
//     /// my_graph.add_initial_index("somefile.txt", "Read", "source", Some(2), None);
//     /// ```
//     /// If inports are defined on the graph, IIPs can be applied calling
//     /// the `add_graph_initial` or `add_graph_initial_index` methods.
//     /// ```no_run
//     /// my_graph.add_graph_initial("somefile.txt", "file", None);
//     ///	my_graph.add_graph_initial_index("somefile.txt", "file", Some(2), None);
//     /// ```
//     pub fn add_initial(
//         &mut self,
//         data: Value,
//         node: &str,
//         port: &str,
//         mut metadata: Option<Map<String, Value>>,
//     ) -> &mut Self {
//         if metadata.is_none() {
//             metadata = Some(Map::new());
//         }
//         if let Some(_node) = self.get_node(node) {
//             let port_name = self.get_port_name(port);
//             self.check_transaction_start();
//             let stub = GraphStub { data };
//             let initializer = GraphIIP {
//                 to: Some(GraphLeaf {
//                     port: port_name,
//                     node_id: node.to_owned(),
//                     index: None,
//                 }),
//                 from: Some(stub),
//                 metadata,
//             };
//             self.initializers.push(initializer.clone());
//             self.emit("add_initial", json!(initializer));
//             self.check_transaction_end();
//         }
//         self
//     }

//     pub fn add_initial_index(
//         &mut self,
//         data: Value,
//         node: &str,
//         port: &str,
//         index: Option<usize>,
//         mut metadata: Option<Map<String, Value>>,
//     ) -> &mut Self {
//         if metadata.is_none() {
//             metadata = Some(Map::new());
//         }
//         if let Some(_) = self.get_node(node) {
//             let port_name = self.get_port_name(port);
//             self.check_transaction_start();
//             let stub = GraphStub { data };
//             let initializer = GraphIIP {
//                 to: Some(GraphLeaf {
//                     port: port_name,
//                     node_id: node.to_owned(),
//                     index,
//                 }),
//                 from: Some(stub),
//                 metadata,
//             };
//             self.initializers.push(initializer.clone());
//             self.emit("add_initial", json!(initializer));
//             self.check_transaction_end();
//         }
//         self
//     }

//     pub fn add_graph_initial(
//         &mut self,
//         data: Value,
//         node: &str,
//         mut metadata: Option<Map<String, Value>>,
//     ) -> &mut Self {
//         if metadata.is_none() {
//             metadata = Some(Map::new());
//         }
//         if let Some(inport) = self.inports.clone().get(node) {
//             self.add_initial(data, &inport.process, &inport.port, metadata);
//         }
//         self
//     }

//     pub fn add_graph_initial_index(
//         &mut self,
//         data: Value,
//         node: &str,
//         index: Option<usize>,
//         mut metadata: Option<Map<String, Value>>,
//     ) -> &mut Self {
//         if metadata.is_none() {
//             metadata = Some(Map::new());
//         }
//         if let Some(inport) = self.inports.clone().get(node) {
//             self.add_initial_index(data, &inport.process, &inport.port, index, metadata);
//         }
//         self
//     }

//     /// Removing Initial Information Packets
//     ///
//     /// IIPs can be removed by calling the `remove_initial` method.
//     /// ```no_run
//     /// my_graph.remove_initial("Read", "source");
//     /// ```
//     /// If the IIP was applied via the `add_graph_initial` or
//     /// `add_graph_initial_index` functions, it can be removed using
//     /// the `remove_graph_initial` method.
//     /// ```no_run
//     /// my_graph.remove_graph_initial("file");
//     /// ```
//     /// Remove an IIP will emit a `remove_initial` event.
//     pub fn remove_initial(&mut self, id: &str, port: &str) -> &mut Self {
//         let port_name = self.get_port_name(port);
//         self.check_transaction_start();
//         let inits = self.initializers.clone();
//         let mut _initializers = Vec::new();
//         for iip in inits {
//             if let Some(to) = iip.to.clone() {
//                 if to.node_id.as_str() == id && to.port == port_name {
//                     self.emit("remove_initial", json!(iip));
//                 }
//             } else {
//                 _initializers.push(iip);
//             }
//         }
//         self.initializers = _initializers;
//         self.check_transaction_end();
//         self
//     }

//     pub fn remove_graph_initial(&mut self, id: &str) -> &mut Self {
//         if let Some(inport) = self.inports.clone().get(id) {
//             self.remove_initial(&inport.process, &inport.port);
//         }
//         self
//     }

//     pub fn to_json(&self) -> GraphJson {
//         let mut json = GraphJson {
//             case_sensitive: self.case_sensitive,
//             properties: Map::new(),
//             inports: self.inports.clone(),
//             outports: self.outports.clone(),
//             groups: Vec::new(),
//             processes: HashMap::new(),
//             connections: Vec::new(),
//         };

//         json.properties = self.properties.clone();
//         json.properties
//             .insert("name".to_owned(), Value::from(self.name.to_owned()));
//         json.properties.remove("baseDir");
//         json.properties.remove("componentLoader");

//         let _ = self.groups.iter().foreach(|group, _iter| {
//             let mut group_data = group.clone();
//             if let Some(metadata) = group.metadata.clone() {
//                 if !metadata.is_empty() {
//                     group_data.metadata = Some(metadata);
//                 }
//             }
//             json.groups.push(group_data);
//         });

//         let _ = self.nodes.iter().foreach(|node, _ter| {
//             json.processes.insert(
//                 node.id.clone(),
//                 GraphNodeJson {
//                     component: node.component.clone(),
//                     metadata: if node.metadata.is_none() {
//                         Some(Map::new())
//                     } else {
//                         node.metadata.clone()
//                     },
//                 },
//             );
//         });

//         let _ = self.edges.iter().foreach(|edge, _iter| {
//             let mut connection = GraphEdgeJson {
//                 src: Some(GraphLeafJson {
//                     process: edge.from.node_id.clone(),
//                     port: edge.from.port.clone(),
//                     index: edge.from.index,
//                 }),
//                 tgt: Some(GraphLeafJson {
//                     process: edge.to.node_id.clone(),
//                     port: edge.to.port.clone(),
//                     index: edge.to.index,
//                 }),
//                 metadata: None,
//                 data: None,
//             };
//             if let Some(metadata) = edge.metadata.clone() {
//                 if !metadata.is_empty() {
//                     connection.metadata = Some(metadata);
//                 }
//             }
//             json.connections.push(connection);
//         });

//         let _ = self.initializers.iter().foreach(|initializer, _iter| {
//             let mut iip = GraphEdgeJson {
//                 src: None,
//                 tgt: None,
//                 data: None,
//                 metadata: None,
//             };
//             if let Some(to) = initializer.to.clone() {
//                 iip.tgt = Some(GraphLeafJson {
//                     process: to.node_id,
//                     port: to.port,
//                     index: to.index,
//                 });
//             }

//             if let Some(from) = initializer.from.clone() {
//                 iip.data = Some(from.data);
//             }

//             if let Some(metadata) = initializer.metadata.clone() {
//                 if !metadata.is_empty() {
//                     iip.metadata = Some(metadata);
//                 }
//             }

//             json.connections.push(iip);
//         });

//         json
//     }

//     pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
//         serde_json::to_string(&self.to_json())
//     }

//     pub fn from_json(json: GraphJson, metadata: Option<Map<String, Value>>) -> Graph {
//         let mut graph = Graph::new(
//             json.properties
//                 .get("name")
//                 .or(Some(&Value::String("".to_string())))
//                 .unwrap()
//                 .as_str()
//                 .unwrap(),
//             json.case_sensitive,
//         );
//         graph.start_transaction("load_json", metadata.clone());

//         graph.set_properties(Map::from_iter(json.properties.clone().into_iter().filter(
//             |v| {
//                 if v.0 != "name" {
//                     return true;
//                 }
//                 return false;
//             },
//         )));

//         json.processes.keys().foreach(|prop, _iter| {
//             if let Some(def) = json.processes.clone().get(prop) {
//                 graph.add_node(prop.as_str(), &def.component, def.metadata.clone());
//             }
//         });

//         json.connections.clone().into_iter().foreach(|conn, _| {
//             if let Some(data) = conn.data {
//                 if let Some(tgt) = conn.tgt {
//                     if tgt.index.is_some() {
//                         graph.add_initial_index(
//                             data,
//                             &tgt.process,
//                             &graph.get_port_name(&tgt.port),
//                             tgt.index,
//                             conn.metadata,
//                         );
//                     } else {
//                         graph.add_initial(
//                             data,
//                             &tgt.process,
//                             &graph.get_port_name(&tgt.port),
//                             conn.metadata,
//                         );
//                     }
//                     // iter.next();
//                     return;
//                 }
//             }
//             if conn.src.clone().is_some() || conn.tgt.clone().is_some() {
//                 if conn.src.clone().unwrap().index.is_some()
//                     || conn.tgt.clone().unwrap().index.is_some()
//                 {
//                     graph.add_edge_index(
//                         &conn.src.clone().unwrap().process,
//                         &graph.get_port_name(&conn.src.clone().unwrap().port),
//                         conn.src.unwrap().index,
//                         &conn.tgt.clone().unwrap().process,
//                         &graph.get_port_name(&conn.tgt.clone().unwrap().port),
//                         conn.tgt.unwrap().index,
//                         conn.metadata,
//                     );
//                     // iter.next();
//                     return;
//                 }
//                 graph.add_edge(
//                     &conn.src.clone().unwrap().process,
//                     &graph.get_port_name(&conn.src.clone().unwrap().port),
//                     &conn.tgt.clone().unwrap().process,
//                     &graph.get_port_name(&conn.tgt.clone().unwrap().port),
//                     conn.metadata,
//                 );
//             }
//         });

//         json.inports.clone().keys().foreach(|inport, _iter| {
//             if let Some(pri) = json.inports.clone().get(inport) {
//                 graph.add_inport(
//                     inport,
//                     &pri.clone().process,
//                     &graph.get_port_name(&pri.port),
//                     pri.metadata.clone(),
//                 );
//             }
//         });
//         json.outports.clone().keys().foreach(|outport, _iter| {
//             if let Some(pri) = json.outports.clone().get(outport) {
//                 graph.add_outport(
//                     outport,
//                     &pri.clone().process,
//                     &graph.get_port_name(&pri.port),
//                     pri.metadata.clone(),
//                 );
//             }
//         });

//         for group in json.groups.clone() {
//             graph.add_group(&group.name, group.nodes, group.metadata);
//         }

//         graph.end_transaction("load_json", metadata.clone());

//         graph
//     }

//     pub fn from_json_string(
//         source: &str,
//         metadata: Option<Map<String, Value>>,
//     ) -> Result<Graph, io::Error> {
//         let json = serde_json::from_str::<GraphJson>(source)?;
//         let graph = Self::from_json(json, metadata);
//         Ok(graph)
//     }

//     /// Save Graph to file
//     pub fn save(&self, path: &str) -> Result<(), io::Error> {
//         let mut file_res = File::create(path);
//         if file_res.is_err() {
//             return Err(file_res.err().unwrap());
//         }
//         if let Ok(file) = file_res.as_mut() {
//             let json = self.to_json();
//             let data = serde_json::to_string(&json)?;
//             file.write_all(data.as_bytes())?;
//             return Ok(());
//         }

//         Err(io::Error::new(
//             io::ErrorKind::InvalidData,
//             "Can't save file",
//         ))
//     }

//     pub fn load_file(path: &str, metadata: Option<Map<String, Value>>) -> Result<Graph, io::Error> {
//         if let Ok(file) = File::open(path).as_mut() {
//             let mut json_str = String::from("");
//             file.read_to_string(&mut json_str)?;
//             return Graph::from_json_string(json_str.as_str(), metadata);
//         }

//         Err(io::Error::new(
//             io::ErrorKind::InvalidData,
//             "Can't load file",
//         ))
//     }

//     pub fn to_static(&self) -> &'static mut Graph {
//         Box::leak(Box::new(self.clone()))
//     }
// }
