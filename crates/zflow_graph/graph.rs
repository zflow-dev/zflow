///    FBP Graph Journal
///    (c) 2022 Damilare Akinlaja
///    (c) 2013-2020 Flowhub UG
///    (c) 2011-2012 Henri Bergius, Nemein
///    FBP Graph may be freely distributed under the MIT license
use crate::internal;
use crate::internal::event_manager::{EventActor, EventListener, EventManager};
use foreach::ForEach;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::iter::FromIterator;
use std::process::exit;
use std::sync::{Arc, Mutex};

use super::types::{
    GraphEdge, GraphExportedPort, GraphGroup, GraphIIP, GraphJson, GraphLeaf, GraphNode, GraphStub,
    GraphTransaction, TransactionEntry, GraphNodeJson, GraphEdgeJson, GraphLeafJson
};

#[cfg(feature = "build_wasm")]
use gloo_utils::format::JsValueSerdeExt;
#[cfg(feature = "build_wasm")]
use wasm_bindgen::prelude::*;

// pub static mut GRAPH_PUBLISHER: Publisher<GraphEvents> = Publisher::new(1024);

/// This class represents an abstract FBP graph containing nodes
/// connected to each other with edges.
/// These graphs can be used for visualization and sketching, but
/// also are the way to start an FBP network.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub name: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub initializers: Vec<GraphIIP>,
    pub groups: Vec<GraphGroup>,
    pub inports: HashMap<String, GraphExportedPort>,
    pub outports: HashMap<String, GraphExportedPort>,
    pub properties: Map<String, Value>,
    pub transaction: Option<GraphTransaction>,
    pub case_sensitive: bool,
    listeners: HashMap<&'static str, Vec<EventActor<Self>>>,
    pub last_revision: i32,
    pub transactions: Vec<Vec<TransactionEntry>>,
    pub subscribed: bool,
    pub current_revision: i32,
    pub(crate) entries: Vec<TransactionEntry>,
}

impl EventListener for Graph {
    /// Attach listener to an event
    fn connect(
        &mut self,
        name: &'static str,
        rec: impl FnMut(&mut Self, Value) -> () + Send + Sync + 'static,
        once: bool,
    ) {
        if !self.listeners.contains_key(name) {
            self.listeners.insert(name, Vec::new());
        }
        if let Some(v) = self.listeners.get_mut(name) {
            v.push(EventActor {
                once,
                callback: Arc::new(Mutex::new(rec)),
            });
        }
    }
}

impl EventManager for Graph {
    /// Send event
    fn emit(&mut self, name: &'static str, data: Value) {
        self.listeners
            .clone()
            .get_mut(name)
            .iter()
            .for_each(|actions| {
                (*actions).iter().enumerate().foreach(|actor, _| {
                    if actor.1.once {
                        self.listeners.get_mut(name).unwrap().remove(actor.0);
                    }
                    if let Ok(mut callback) = actor.1.callback.lock() {
                        callback(self, data.clone());
                    }
                })
            });
    }
    /// Remove listeners from event
    fn disconnect(&mut self, name: &str) {
        self.listeners.remove(name);
    }
    /// Check if we have events
    fn has_event(&self, name: &str) -> bool {
        self.listeners.contains_key(name)
    }
}

impl Graph {
    pub fn new(name: &str, case_sensitive: bool) -> Self {
        Self {
            name: name.to_owned(),
            nodes: Vec::new(),
            edges: Vec::new(),
            initializers: Vec::new(),
            groups: Vec::new(),
            inports: HashMap::new(),
            outports: HashMap::new(),
            properties: Map::new(),
            transaction: None,
            case_sensitive,
            listeners: HashMap::new(),
            last_revision: 0,
            transactions: Vec::new(),
            subscribed: true,
            current_revision: -1,
            entries: Vec::new(),
        }
    }

    pub fn get_port_name(&self, port: &str) -> String {
        if self.case_sensitive {
            return port.to_string();
        }
        port.to_lowercase()
    }

   
    pub fn start_transaction(
        &mut self,
        id: &str,
        metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        if self.transaction.is_some() {
            log::error!("Nested transactions not supported");
            exit(1)
        };

        self.transaction = Some(GraphTransaction {
            id: id.to_string(),
            depth: 1,
        });

        #[cfg(not(feature = "build_wasm"))]
        self.emit(
            "start_transaction",
            json!({
                "id": id.to_string(),
                "metadata": json!(metadata)
            }),
        );
        #[cfg(feature = "build_wasm")]
        {
            let meta = json!(metadata);
            let _data = json!({
                "id": id,
                "metadata": meta
            });
    
            self.emit("start_transaction", JsValue::from_serde(&_data).unwrap());
        }

        self
    }

    
    

   
    pub fn end_transaction(&mut self, id: &str, metadata: Option<Map<String, Value>>) -> &mut Self {
        if self.transaction.is_none() {
            log::error!("Attempted to end non-existing transaction");
            exit(1)
        };

        self.transaction = None;

        #[cfg(not(feature = "build_wasm"))]
        self.emit(
            "end_transaction",
            json!({
                "id": id.to_string(),
                "metadata": json!(metadata)
            }),
        );
        #[cfg(feature = "build_wasm")]
        {
            let meta = json!(metadata);
            let _data = json!({
                "id": id,
                "metadata": meta
            });
    
            self.emit("end_transaction", JsValue::from_serde(&_data).unwrap());
        }
        self
    }
   
    pub fn check_transaction_start(&mut self) -> &mut Self {
        match self.transaction {
            None => {
                self.start_transaction("implicit", None);
            }
            Some(ref mut transaction) => {
                if transaction.id == "implicit" {
                    transaction.depth += 1;
                }
            }
        }
        self
    }

    pub fn check_transaction_end(&mut self) -> &mut Self {
        if let Some(ref mut transaction) = self.transaction {
            if transaction.id == "implicit" {
                transaction.depth -= 1;
            }

            if transaction.depth == 0 {
                self.end_transaction("implicit", None);
            }
        }

        self
    }

    /// This method allows changing properties of the graph.
    pub fn set_properties(&mut self, properties: Map<String, Value>) -> &mut Self {
        self.check_transaction_start();
        let before = self.properties.clone();

        for item in properties.keys() {
            let val = properties.get(item);
            if let Some(val) = val {
                self.properties.insert(item.to_string(), val.clone());
            }
        }

        #[cfg(feature = "build_wasm")]
        self.emit(
            "change_properties",
            JsValue::from_serde(&json!({
                "new": self.properties.clone(),
                "before": before
            }))
            .unwrap(),
        );
        #[cfg(not(feature = "build_wasm"))]
        self.emit(
            "change_properties",
            json!({
                "new": self.properties.clone(),
                "before": before
            }),
        );

        self.check_transaction_end();

        self
    }

    /// Nodes objects can be retrieved from the graph by their ID:
    /// ```no_run
    /// let node = my_graph.get_node('Read').unwrap();
    /// ```
    pub fn get_node(&self, key: &str) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.id == key.to_owned())
    }
    pub fn get_node_mut(&mut self, key: &str) -> Option<&mut GraphNode> {
        self.nodes.iter_mut().find(|n| n.id == key.to_owned())
    }

    pub fn add_inport(
        &mut self,
        public_port: &str,
        node_key: &str,
        port_key: &str,
        metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        // Check that node exists
        if self.get_node(node_key).is_none() {
            return self;
        }

        let port_name = self.get_port_name(public_port);

        self.check_transaction_start();

        let val = GraphExportedPort {
            process: node_key.to_owned(),
            port: self.get_port_name(port_key),
            metadata,
        };
        self.inports.insert(port_name.to_owned(), val.clone());

        #[cfg(not(feature = "build_wasm"))]
        self.emit(
            "add_inport",
            json!({
                "name": port_name,
                "port": val
            }),
        );

        #[cfg(feature = "build_wasm")]
        self.emit(
            "add_inport",
            JsValue::from_serde(&json!({
                "name": port_name,
                "port": val
            }))
            .unwrap(),
        );

        self.check_transaction_end();

        self
    }

    pub fn remove_inport(&mut self, public_port: &str) -> &mut Self {
        let port_name = self.get_port_name(public_port);

        if !self.inports.contains_key(&(port_name.clone())) {
            return self;
        }
        self.check_transaction_start();

        let inp = self.inports.clone();

        self.set_inports_metadata(port_name.as_str(), Map::new());

        self.inports.remove(&(port_name.clone()));

        #[cfg(not(feature = "build_wasm"))]
        self.emit(
            "remove_inport",
            json!({
                "name": port_name.clone(),
                "port": inp.get(&(port_name.clone()))
            }),
        );
        #[cfg(feature = "build_wasm")]
        self.emit(
            "remove_inport",
            JsValue::from_serde(&json!({
                "name": port_name.clone(),
                "port": inp.get(&(port_name.clone()))
            }))
            .unwrap(),
        );

        self.check_transaction_end();

        self
    }

    pub fn rename_inport(&mut self, old_port: &str, new_port: &str) -> &mut Self {
        let old_port_name = self.get_port_name(old_port);
        let new_port_name = self.get_port_name(new_port);
        if !self.inports.contains_key(&(old_port_name.clone())) {
            return self;
        }

        if new_port_name == old_port_name {
            return self;
        }

        self.check_transaction_start();

        if let Some(old_port) = self.inports.clone().get(&old_port_name) {
            self.inports.insert(new_port_name.clone(), old_port.clone());
            self.inports.remove(&old_port_name);
            #[cfg(not(feature = "build_wasm"))]
            self.emit(
                "rename_inport",
                json!({
                    "old": old_port_name.clone(),
                    "new": new_port_name.clone()
                }),
            );
            #[cfg(feature = "build_wasm")]
            self.emit(
                "rename_inport",
                JsValue::from_serde(&json!({
                    "old": old_port_name.clone(),
                    "new": new_port_name.clone()
                }))
                .unwrap(),
            );
        }

        self.check_transaction_end();

        self
    }

    pub fn add_outport(
        &mut self,
        public_port: &str,
        node_key: &str,
        port_key: &str,
        metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        // Check that node exists
        if self.get_node(node_key).is_none() {
            return self;
        }

        let port_name = self.get_port_name(public_port);

        self.check_transaction_start();

        let val = GraphExportedPort {
            process: node_key.to_owned(),
            port: self.get_port_name(port_key),
            metadata,
        };
        self.outports.insert(port_name.to_owned(), val.clone());
        #[cfg(not(feature = "build_wasm"))]
        self.emit(
            "add_outport",
            json!({
                "name": port_name,
                "port": val
            }),
        );
        #[cfg(feature = "build_wasm")]
        self.emit(
            "add_outport",
            JsValue::from_serde(&json!({
                "name": port_name,
                "port": val
            }))
            .unwrap(),
        );

        self.check_transaction_end();
        self
    }

    pub fn remove_outport(&mut self, public_port: &str) -> &mut Self {
        let port_name = self.get_port_name(public_port);

        if !self.outports.contains_key(&(port_name.clone())) {
            return self;
        }
        self.check_transaction_start();

        let oup = self.outports.clone();

        self.set_outports_metadata(port_name.as_str(), Map::new());

        self.outports.remove(&(port_name.clone()));

        #[cfg(not(feature = "build_wasm"))]
        self.emit(
            "remove_outport",
            json!({
                "name": port_name.clone(),
                "port": oup.get(&(port_name.clone()))
            }),
        );
        #[cfg(feature = "build_wasm")]
        self.emit(
            "remove_outport",
            JsValue::from_serde(&json!({
                "name": port_name.clone(),
                "port": oup.get(&(port_name.clone()))
            }))
            .unwrap(),
        );

        self.check_transaction_end();

        self
    }

    pub fn rename_outport(&mut self, old_port: &str, new_port: &str) -> &mut Self {
        let old_port_name = self.get_port_name(old_port);
        let new_port_name = self.get_port_name(new_port);
        if !self.outports.contains_key(&(old_port_name.clone())) {
            return self;
        }

        if new_port_name == old_port_name {
            return self;
        }

        self.check_transaction_start();

        if let Some(old_port) = self.outports.clone().get(&old_port_name) {
            self.outports
                .insert(new_port_name.clone(), old_port.clone());
            self.outports.remove(&old_port_name);
            #[cfg(not(feature = "build_wasm"))]
            self.emit(
                "rename_outport",
                json!({
                    "old": old_port_name.clone(),
                    "new": new_port_name.clone()
                }),
            );
            #[cfg(feature = "build_wasm")]
            self.emit(
                "rename_outport",
                JsValue::from_serde(&json!({
                    "old": old_port_name.clone(),
                    "new": new_port_name.clone()
                }))
                .unwrap(),
            );
        }

        self.check_transaction_end();

        self
    }

    pub fn set_inports_metadata(
        &mut self,
        public_port: &str,
        metadata: Map<String, Value>,
    ) -> &mut Self {
        let port_name = self.get_port_name(public_port);
        if !self.inports.contains_key(&(port_name.clone())) {
            return self;
        }

        self.check_transaction_start();

        if let Some(p) = self.inports.get(&(port_name.clone())) {
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
                    self.inports.insert(port_name.clone(), p.clone());
                } else {
                    // iter.next();
                    return;
                }
            });

            #[cfg(not(feature = "build_wasm"))]
            self.emit(
                "change_inport",
                json!({
                    "name": port_name.clone(),
                    "port": p.clone(),
                    "old_metadata": before,
                    "new_metadata": metadata
                }),
            );

            #[cfg(feature = "build_wasm")]
            self.emit(
                "change_inport",
                JsValue::from_serde(&json!({
                    "name": port_name.clone(),
                    "port": p.clone(),
                    "old_metadata": before,
                    "new_metadata": metadata
                }))
                .unwrap(),
            );
        }

        self.check_transaction_end();

        self
    }

    pub fn set_outports_metadata(
        &mut self,
        public_port: &str,
        metadata: Map<String, Value>,
    ) -> &mut Self {
        let port_name = self.get_port_name(public_port);
        if !self.outports.contains_key(&(port_name.clone())) {
            return self;
        }

        self.check_transaction_start();

        if let Some(p) = self.outports.get(&(port_name.clone())) {
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
                    self.outports.insert(port_name.clone(), p.clone());
                } else {
                    // iter.next();
                    return;
                }
            });

            #[cfg(not(feature = "build_wasm"))]
            self.emit(
                "change_outport",
                json!({
                    "name": port_name.clone(),
                    "port": p.clone(),
                    "old_metadata": before,
                    "new_metadata": metadata
                }),
            );
            #[cfg(feature = "build_wasm")]
            self.emit(
                "change_outport",
                JsValue::from_serde(&json!({
                    "name": port_name.clone(),
                    "port": p.clone(),
                    "old_metadata": before,
                    "new_metadata": metadata
                }))
                .unwrap(),
            );
        }

        self.check_transaction_end();

        self
    }

    /// Grouping nodes in a graph
    pub fn add_group(
        &mut self,
        group: &str,
        nodes: Vec<String>,
        metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        self.check_transaction_start();
        let g = &GraphGroup {
            name: group.to_owned(),
            nodes,
            metadata,
        };
        self.groups.push(g.clone());
        #[cfg(not(feature = "build_wasm"))]
        self.emit("add_group", json!(g));
        #[cfg(feature = "build_wasm")]
        self.emit("add_group", JsValue::from_serde(&json!(g)).unwrap());
        self.check_transaction_end();
        self
    }

    pub fn rename_group(&mut self, old_name: &str, new_name: &str) -> &mut Self {
        self.check_transaction_start();
        for i in 0..self.groups.len() {
            let group = &mut self.groups[i];
            if group.name == old_name {
                (*group).name = new_name.to_owned();
                #[cfg(not(feature = "build_wasm"))]
                self.emit(
                    "rename_group",
                    json!({
                        "old": old_name,
                        "new": new_name
                    }),
                );
                #[cfg(feature = "build_wasm")]
                self.emit(
                    "rename_group",
                    JsValue::from_serde(&json!({
                        "old": old_name,
                        "new": new_name
                    }))
                    .unwrap(),
                );
            }
        }
        self.check_transaction_end();
        self
    }

    pub fn remove_group(&mut self, group_name: &str) -> &mut Self {
        self.check_transaction_start();

        self.groups = self
            .groups
            .clone()
            .iter()
            .filter(|v| {
                if v.name == group_name.to_owned() {
                    self.set_group_metadata(group_name, Map::new());
                    #[cfg(not(feature = "build_wasm"))]
                    self.emit("remove_group", json!(v));
                    #[cfg(feature = "build_wasm")]
                    self.emit("remove_group", JsValue::from_serde(&json!(v)).unwrap());
                    return false;
                }
                return true;
            })
            .map(|v| v.clone())
            .collect();
        self.check_transaction_end();
        self
    }

    pub fn set_group_metadata(
        &mut self,
        group_name: &str,
        metadata: Map<String, Value>,
    ) -> &mut Self {
        self.check_transaction_start();
        for (i, group) in self.groups.clone().iter_mut().enumerate() {
            if group.name != group_name.to_owned() {
                continue;
            }
            let before = group.metadata.clone();
            for item in metadata.clone().keys() {
                if let Some(meta) = group.metadata.as_mut() {
                    if let Some(val) = metadata.get(item) {
                        meta.insert(item.to_owned(), val.clone());
                    } else {
                        meta.remove(item);
                    }
                }
            }
            self.groups[i] = group.clone();
            #[cfg(not(feature = "build_wasm"))]
            self.emit(
                "change_group",
                json!({
                    "group": group.clone(),
                    "old_metadata": before,
                    "new_metadata": metadata
                }),
            );
            #[cfg(feature = "build_wasm")]
            self.emit(
                "change_group",
                JsValue::from_serde(&json!({
                    "group": group.clone(),
                    "old_metadata": before,
                    "new_metadata": metadata
                }))
                .unwrap(),
            );
        }

        self.check_transaction_end();
        self
    }

    /// Adding a node to the graph
    /// Nodes are identified by an ID unique to the graph. Additionally,
    /// a node may contain information on what FBP component it is and
    /// possibly display coordinates.
    /// ```no_run
    /// let mut metadata = Map::new();
    /// metadata.insert("x".to_string(), 91);
    /// metadata.insert("y".to_string(), 154);
    /// my_graph.add_node("Read", "ReadFile", Some(metadata));
    /// ```
    pub fn add_node(
        &mut self,
        id: &str,
        component: &str,
        metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        self.check_transaction_start();
        let node = GraphNode {
            id: id.to_owned(),
            component: component.to_owned(),
            metadata,
        };
        self.nodes.push(node.clone());
        #[cfg(not(feature = "build_wasm"))]
        self.emit("add_node", json!(node));
        #[cfg(feature = "build_wasm")]
        self.emit("add_node", JsValue::from_serde(&json!(node)).unwrap());
        self.check_transaction_end();
        self
    }

    /// Removing a node from the graph
    /// Existing nodes can be removed from a graph by their ID. This
    /// will remove the node and also remove all edges connected to it.
    /// ```no_run
    /// my_graph.remove_node("Read");
    /// ```
    /// Once the node has been removed, the `remove_node` event will be
    pub fn remove_node(&mut self, id: &str) -> &mut Self {
        self.check_transaction_start();
        if let Some(node) = self.get_node(id).cloned() {
            for edge in self.edges.clone() {
                if (edge.from.node_id == node.id) || (edge.to.node_id == node.id) {
                    self.remove_edge(
                        edge.from.node_id.as_str(),
                        edge.from.port.as_str(),
                        Some(edge.to.node_id.as_str()),
                        Some(edge.to.port.as_str()),
                    );
                }
            }

            self.initializers.clone().iter().foreach(|iip, _iter| {
                if let Some(to) = iip.to.clone() {
                    if to.node_id == node.id {
                        self.remove_initial(to.node_id.as_str(), to.port.as_str());
                    }
                }
            });
            self.inports.clone().keys().foreach(|port, _iter| {
                if let Some(private) = self.inports.clone().get(port) {
                    if private.process == id {
                        self.remove_inport(port);
                    }
                }
            });
            self.outports.clone().keys().foreach(|port, _iter| {
                if let Some(private) = self.outports.clone().get(port) {
                    if private.process == id {
                        self.remove_outport(port);
                    }
                }
            });

            for (i, group) in self.groups.clone().iter_mut().enumerate() {
                if group.nodes.contains(&id.to_string()) {
                    if let Some(index) = group.nodes.iter().position(|node| node == id) {
                        self.groups[i].nodes.remove(index);
                        if self.groups[i].nodes.is_empty() {
                            self.remove_group(&group.name);
                        }
                    }
                }
            }
        }

        self.set_node_metadata(id, Map::new());
        if let Some(index) = self.nodes.iter().position(|n| n.id == id) {
            let node = self.nodes.remove(index);
            #[cfg(not(feature = "build_wasm"))]
            self.emit("remove_node", json!(node));
            #[cfg(feature = "build_wasm")]
            self.emit("remove_node", JsValue::from_serde(&json!(node)).unwrap());
        }

        self.check_transaction_end();

        self
    }

    /// Renaming a node
    ///
    /// Nodes IDs can be changed by calling this method.
    pub fn rename_node(&mut self, old_id: &str, new_id: &str) -> &mut Self {
        self.check_transaction_start();
        if let Some(node) = self.get_node_mut(old_id) {
            (*node).id = new_id.to_owned();

            // let node_index = self
            //     .nodes
            //     .iter()
            //     .position(|n| n.id == old_id.to_owned())
            //     .unwrap();
            // self.nodes[node_index] = node.clone();

            let _ = self.edges.iter_mut().foreach(|edge, _iter| {
                if edge.from.node_id == old_id.to_owned() {
                    (*edge).from.node_id = new_id.to_owned()
                }
                if edge.to.node_id == old_id.to_owned() {
                    (*edge).to.node_id = new_id.to_owned()
                }
            });

            let _ = self.initializers.iter_mut().foreach(|iip, _iter| {
                if let Some(to) = iip.to.as_mut() {
                    if to.node_id == old_id.to_owned() {
                        (*to).node_id = new_id.to_owned()
                    }
                }
            });

            let _ = self.inports.clone().keys().foreach(|port, _iter| {
                if let Some(private) = self.inports.get_mut(port) {
                    if private.process == old_id.to_owned() {
                        private.process = new_id.to_owned();
                    }
                }
            });
            let _ = self.outports.clone().keys().foreach(|port, _iter| {
                if let Some(private) = self.outports.get_mut(port) {
                    if private.process == old_id.to_owned() {
                        private.process = new_id.to_owned();
                    }
                }
            });

            let _ = self.groups.iter_mut().foreach(|group, _iter| {
                if let Some(index) = group
                    .nodes
                    .iter()
                    .position(|n| n.to_owned() == old_id.to_owned())
                {
                    group.nodes[index] = new_id.to_owned();
                }
            });

            #[cfg(not(feature = "build_wasm"))]
            self.emit(
                "rename_node",
                json!({
                    "old": old_id,
                    "new": new_id,
                }),
            );
            #[cfg(feature = "build_wasm")]
            self.emit(
                "rename_node",
                JsValue::from_serde(&json!({
                    "old": old_id,
                    "new": new_id,
                }))
                .unwrap(),
            );
            self.check_transaction_end();
        }
        self
    }

    pub fn set_node_metadata(&mut self, id: &str, metadata: Map<String, Value>) -> &mut Self {
        self.check_transaction_start();
        if let Some(node) = self.get_node_mut(id) {
            let before = node.metadata.clone();

            if node.metadata.is_none() {
                (*node).metadata = Some(Map::new());
            }

            if metadata.keys().len() == 0 {
                (*node).metadata = Some(Map::new());
            }

            let _ = metadata.clone().keys().foreach(|item, _iter| {
                let meta = metadata.clone();
                let val = meta.get(item);

                if let Some(existing_meta) = node.metadata.as_mut() {
                    if let Some(val) = val {
                        (*existing_meta).insert(item.clone(), val.clone());
                    } else {
                        (*existing_meta).remove(item);
                    }
                }
            });
            let _node = node.clone();
            #[cfg(not(feature = "build_wasm"))]
            self.emit(
                "change_node",
                json!({
                    "node": _node,
                    "old_metadata": before,
                    "new_metadata": metadata
                }),
            );
            #[cfg(feature = "build_wasm")]
            self.emit(
                "change_node",
                JsValue::from_serde(&json!({
                    "node": _node,
                    "old_metadata": before,
                    "new_metadata": metadata
                }))
                .unwrap(),
            );
            // let node_index = self
            //     .nodes
            //     .iter()
            //     .position(|n| n.id == id.to_owned())
            //     .unwrap();
            // self.nodes[node_index] = node.clone();
        }
        self.check_transaction_end();
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
    /// Adding an edge will emit the `addEdge` event.
    pub fn add_edge(
        &mut self,
        out_node: &str,
        out_port: &str,
        in_node: &str,
        in_port: &str,
        mut metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        if metadata.is_none() {
            metadata = Some(Map::new());
        }
        let out_port_name = self.get_port_name(out_port);
        let in_port_name = self.get_port_name(in_port);
        let some = self.edges.iter().find(|edge| {
            (edge.from.node_id == out_node.to_owned())
                && (edge.from.port == out_port_name.to_owned())
                && (edge.to.node_id == in_node.to_owned())
                && (edge.to.port == in_port_name.to_owned())
        });
        if some.is_some() {
            return self;
        }
        if self.get_node(out_node).is_none() {
            return self;
        }
        if self.get_node(in_node).is_none() {
            return self;
        }
        self.check_transaction_start();
        let edge = GraphEdge {
            from: GraphLeaf {
                port: out_port_name.to_owned(),
                node_id: out_node.to_owned(),
                index: None,
            },
            to: GraphLeaf {
                port: in_port_name.to_owned(),
                node_id: in_node.to_owned(),
                index: None,
            },
            metadata,
        };
        self.edges.push(edge.clone());
        #[cfg(not(feature = "build_wasm"))]
        self.emit("add_edge", json!(&edge));
        #[cfg(feature = "build_wasm")]
        self.emit("add_edge", JsValue::from_serde(&json!(&edge)).unwrap());
        self.check_transaction_end();
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
        if self
            .edges
            .clone()
            .iter()
            .find(|edge| {
                // don't add a duplicate edge
                if (edge.from.node_id == out_node.to_owned())
                    && (edge.from.port == out_port_name.to_owned())
                    && (edge.to.node_id == in_node.to_owned())
                    && (edge.to.port == in_port_name.to_owned())
                {
                    if index_1 == edge.from.index && index_2 == edge.to.index {
                        return true;
                    }
                }
                return false;
            })
            .is_some()
        {
            return self;
        }

        if self.get_node(out_node).is_none() {
            return self;
        }
        if self.get_node(in_node).is_none() {
            return self;
        }
        self.check_transaction_start();
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
        self.edges.push(edge.clone());
        #[cfg(not(feature = "build_wasm"))]
        self.emit("add_edge", json!(&edge));
        #[cfg(feature = "build_wasm")]
        self.emit("add_edge", JsValue::from_serde(&json!(&edge)).unwrap());

        self.check_transaction_end();
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
        &mut self,
        node: &str,
        port: &str,
        node2: Option<&str>,
        port2: Option<&str>,
    ) -> &mut Self {
        if self
            .get_edge(
                node,
                port,
                node2.or(Some("")).unwrap(),
                port2.or(Some("")).unwrap(),
            )
            .is_none()
        {
            return self;
        }

        self.check_transaction_start();
        let out_port = self.get_port_name(port);
        let mut in_port = None;
        if let Some(port2) = port2 {
            in_port = Some(self.get_port_name(port2));
        }

        self.edges = self
            .edges
            .clone()
            .iter()
            .filter(|edge| {
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
                        #[cfg(not(feature = "build_wasm"))]
                        self.emit("remove_edge", json!(edge));
                        #[cfg(feature = "build_wasm")]
                        self.emit("remove_edge", JsValue::from_serde(&json!(edge)).unwrap());
                        return false;
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

                    #[cfg(not(feature = "build_wasm"))]
                    self.emit("remove_edge", json!(edge));
                    #[cfg(feature = "build_wasm")]
                    self.emit("remove_edge", JsValue::from_serde(&json!(edge)).unwrap());
                    return false;
                }
                true
            })
            .map(|edge| edge.clone())
            .collect();

        self.check_transaction_end();

        self
    }

    /// Getting an edge
    ///
    /// Edge objects can be retrieved from the graph by the node and port IDs:
    /// ```no_run
    /// my_edge = my_graph.get_edge("Read", "out", "Write", "in");
    /// ```
    pub fn get_edge(&self, node: &str, port: &str, node2: &str, port2: &str) -> Option<GraphEdge> {
        let out_port = self.get_port_name(port);
        let in_port = self.get_port_name(port2);
        self.edges
            .iter()
            .find(|edge| {
                edge.from.node_id.as_str() == node
                    && edge.from.port == out_port
                    && edge.to.node_id.as_str() == node2
                    && edge.to.port == in_port
            })
            .cloned()
    }
    pub fn get_edge_mut(
        &mut self,
        node: &str,
        port: &str,
        node2: &str,
        port2: &str,
    ) -> Option<&mut GraphEdge> {
        let out_port = self.get_port_name(port);
        let in_port = self.get_port_name(port2);
        self.edges.iter_mut().find(|edge| {
            edge.from.node_id.as_str() == node
                && edge.from.port == out_port
                && edge.to.node_id.as_str() == node2
                && edge.to.port == in_port
        })
    }

    /// Changing an edge's metadata
    ///
    /// Edge metadata can be set or changed by calling this method.
    pub fn set_edge_metadata(
        &mut self,
        node: &str,
        port: &str,
        node2: &str,
        port2: &str,
        metadata: Map<String, Value>,
    ) -> &mut Self {
        self.check_transaction_start();
        let edge = self.get_edge_mut(node, port, node2, port2);
        if edge.is_none() {
            self.check_transaction_end();
            return self;
        }

        let edge = edge.unwrap();
        if edge.metadata.is_none() {
            (*edge).metadata = Some(Map::new());
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
        let _edge = edge.clone();
        #[cfg(not(feature = "build_wasm"))]
        self.emit(
            "change_edge",
            json!({
                "edge": _edge,
                "old_metadata": before,
                "new_metadata": metadata
            }),
        );
        #[cfg(feature = "build_wasm")]
        self.emit(
            "change_edge",
            JsValue::from_serde(&json!({
                "edge": _edge,
                "old_metadata": before,
                "new_metadata": metadata
            }))
            .unwrap(),
        );
        // let edge_index = self
        //     .edges
        //     .iter()
        //     .position(|edge| {
        //         edge.from.node_id.as_str() == node
        //             && edge.from.port == port
        //             && edge.to.node_id.as_str() == node2
        //             && edge.to.port == port2
        //     })
        //     .unwrap();
        // self.edges[edge_index] = edge.clone();

        self.check_transaction_end();
        self
    }

    /// Adding Initial Information Packets
    ///
    /// Initial Information Packets (IIPs) can be used for sending data
    /// to specified node inports without a sending node instance.
    ///
    /// IIPs are especially useful for sending configuration information
    /// to components at FBP network start-up time. This could include
    /// filenames to read, or network ports to listen to.
    ///
    /// ```no_run
    /// my_graph.add_initial("somefile.txt", "Read", "source", None);
    /// my_graph.add_initial_index("somefile.txt", "Read", "source", Some(2), None);
    /// ```
    /// If inports are defined on the graph, IIPs can be applied calling
    /// the `add_graph_initial` or `add_graph_initial_index` methods.
    /// ```no_run
    /// my_graph.add_graph_initial("somefile.txt", "file", None);
    ///	my_graph.add_graph_initial_index("somefile.txt", "file", Some(2), None);
    /// ```
    ///
    pub fn add_initial(
        &mut self,
        data: Value,
        node: &str,
        port: &str,
        mut metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        if metadata.is_none() {
            metadata = Some(Map::new());
        }
        if let Some(_node) = self.get_node(node) {
            let port_name = self.get_port_name(port);
            self.check_transaction_start();
            let stub = GraphStub { data };
            let initializer = GraphIIP {
                to: Some(GraphLeaf {
                    port: port_name,
                    node_id: node.to_owned(),
                    index: None,
                }),
                from: Some(stub),
                metadata,
            };
            self.initializers.push(initializer.clone());
            #[cfg(not(feature = "build_wasm"))]
            self.emit("add_initial", json!(initializer));
            #[cfg(feature = "build_wasm")]
            self.emit(
                "add_initial",
                JsValue::from_serde(&json!(initializer)).unwrap(),
            );
            self.check_transaction_end();
        }
        self
    }

    pub fn add_initial_index(
        &mut self,
        data: Value,
        node: &str,
        port: &str,
        index: Option<usize>,
        mut metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        if metadata.is_none() {
            metadata = Some(Map::new());
        }
        if let Some(_) = self.get_node(node) {
            let port_name = self.get_port_name(port);
            self.check_transaction_start();
            let stub = GraphStub { data };
            let initializer = GraphIIP {
                to: Some(GraphLeaf {
                    port: port_name,
                    node_id: node.to_owned(),
                    index,
                }),
                from: Some(stub),
                metadata,
            };
            self.initializers.push(initializer.clone());
            #[cfg(not(feature = "build_wasm"))]
            self.emit("add_initial", json!(initializer));
            #[cfg(feature = "build_wasm")]
            self.emit(
                "add_initial",
                JsValue::from_serde(&json!(initializer)).unwrap(),
            );
            self.check_transaction_end();
        }
        self
    }

    pub fn add_graph_initial(
        &mut self,
        data: Value,
        node: &str,
        mut metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        if metadata.is_none() {
            metadata = Some(Map::new());
        }
        if let Some(inport) = self.inports.clone().get(node) {
            self.add_initial(data, &inport.process, &inport.port, metadata);
        }
        self
    }

    pub fn add_graph_initial_index(
        &mut self,
        data: Value,
        node: &str,
        index: Option<usize>,
        mut metadata: Option<Map<String, Value>>,
    ) -> &mut Self {
        if metadata.is_none() {
            metadata = Some(Map::new());
        }
        if let Some(inport) = self.inports.clone().get(node) {
            self.add_initial_index(data, &inport.process, &inport.port, index, metadata);
        }
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
    pub fn remove_initial(&mut self, id: &str, port: &str) -> &mut Self {
        let port_name = self.get_port_name(port);
        self.check_transaction_start();
        let inits = self.initializers.clone();
        let mut _initializers = Vec::new();
        for iip in inits {
            if let Some(to) = iip.to.clone() {
                if to.node_id.as_str() == id && to.port == port_name {
                    #[cfg(not(feature = "build_wasm"))]
                    self.emit("remove_initial", json!(iip));
                    #[cfg(feature = "build_wasm")]
                    self.emit("remove_initial", JsValue::from_serde(&json!(iip)).unwrap());
                }
            } else {
                _initializers.push(iip);
            }
        }
        self.initializers = _initializers;
        self.check_transaction_end();
        self
    }

    pub fn remove_graph_initial(&mut self, id: &str) -> &mut Self {
        if let Some(inport) = self.inports.clone().get(id) {
            self.remove_initial(&inport.process, &inport.port);
        }
        self
    }

    pub fn to_json(&self) -> GraphJson {
        let mut json = GraphJson {
            case_sensitive: self.case_sensitive,
            properties: Map::new(),
            inports: self.inports.clone(),
            outports: self.outports.clone(),
            groups: Vec::new(),
            processes: HashMap::new(),
            connections: Vec::new(),
        };

        json.properties = self.properties.clone();
        json.properties
            .insert("name".to_owned(), Value::from(self.name.to_owned()));
        json.properties.remove("baseDir");
        json.properties.remove("componentLoader");

        let _ = self.groups.iter().foreach(|group, _iter| {
            let mut group_data = group.clone();
            if let Some(metadata) = group.metadata.clone() {
                if !metadata.is_empty() {
                    group_data.metadata = Some(metadata);
                }
            }
            json.groups.push(group_data);
        });

        let _ = self.nodes.iter().foreach(|node, _ter| {
            json.processes.insert(
                node.id.clone(),
                GraphNodeJson {
                    component: node.component.clone(),
                    metadata: if node.metadata.is_none() {
                        Some(Map::new())
                    } else {
                        node.metadata.clone()
                    },
                },
            );
        });

        let _ = self.edges.iter().foreach(|edge, _iter| {
            let mut connection = GraphEdgeJson {
                src: Some(GraphLeafJson {
                    process: edge.from.node_id.clone(),
                    port: edge.from.port.clone(),
                    index: edge.from.index,
                }),
                tgt: Some(GraphLeafJson {
                    process: edge.to.node_id.clone(),
                    port: edge.to.port.clone(),
                    index: edge.to.index,
                }),
                metadata: None,
                data: None,
            };
            if let Some(metadata) = edge.metadata.clone() {
                if !metadata.is_empty() {
                    connection.metadata = Some(metadata);
                }
            }
            json.connections.push(connection);
        });

        let _ = self.initializers.iter().foreach(|initializer, _iter| {
            let mut iip = GraphEdgeJson {
                src: None,
                tgt: None,
                data: None,
                metadata: None,
            };
            if let Some(to) = initializer.to.clone() {
                iip.tgt = Some(GraphLeafJson {
                    process: to.node_id,
                    port: to.port,
                    index: to.index,
                });
            }

            if let Some(from) = initializer.from.clone() {
                iip.data = Some(from.data);
            }

            if let Some(metadata) = initializer.metadata.clone() {
                if !metadata.is_empty() {
                    iip.metadata = Some(metadata);
                }
            }

            json.connections.push(iip);
        });

        json
    }

    
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.to_json())
    }

    pub fn from_json(json: GraphJson, metadata: Option<Map<String, Value>>) -> Graph {
        let mut graph = Graph::new(
            json.properties
                .get("name")
                .or(Some(&Value::String("".to_string())))
                .unwrap()
                .as_str()
                .unwrap(),
            json.case_sensitive,
        );
      
        graph.start_transaction("load_json", metadata.clone());
       
        graph.set_properties(Map::from_iter(json.properties.clone().into_iter().filter(
            |v| {
                if v.0 != "name" {
                    return true;
                }
                return false;
            },
        )));

        json.processes.keys().foreach(|prop, _iter| {
            if let Some(def) = json.processes.clone().get(prop) {
                graph.add_node(prop.as_str(), &def.component, def.metadata.clone());
            }
        });

        json.connections.clone().into_iter().foreach(|conn, _| {
            if let Some(data) = conn.data {
                if let Some(tgt) = conn.tgt {
                    if tgt.index.is_some() {
                        graph.add_initial_index(
                            data,
                            &tgt.process,
                            &graph.get_port_name(&tgt.port),
                            tgt.index,
                            conn.metadata,
                        );
                    } else {
                        graph.add_initial(
                            data,
                            &tgt.process,
                            &graph.get_port_name(&tgt.port),
                            conn.metadata,
                        );
                    }
                    // iter.next();
                    return;
                }
            }
            if conn.src.clone().is_some() || conn.tgt.clone().is_some() {
                if conn.src.clone().unwrap().index.is_some()
                    || conn.tgt.clone().unwrap().index.is_some()
                {
                    graph.add_edge_index(
                        &conn.src.clone().unwrap().process,
                        &graph.get_port_name(&conn.src.clone().unwrap().port),
                        conn.src.unwrap().index,
                        &conn.tgt.clone().unwrap().process,
                        &graph.get_port_name(&conn.tgt.clone().unwrap().port),
                        conn.tgt.unwrap().index,
                        conn.metadata,
                    );
                    // iter.next();
                    return;
                }
                graph.add_edge(
                    &conn.src.clone().unwrap().process,
                    &graph.get_port_name(&conn.src.clone().unwrap().port),
                    &conn.tgt.clone().unwrap().process,
                    &graph.get_port_name(&conn.tgt.clone().unwrap().port),
                    conn.metadata,
                );
            }
        });

        json.inports.clone().keys().foreach(|inport, _iter| {
            if let Some(pri) = json.inports.clone().get(inport) {
                graph.add_inport(
                    inport,
                    &pri.clone().process,
                    &graph.get_port_name(&pri.port),
                    pri.metadata.clone(),
                );
            }
        });
        json.outports.clone().keys().foreach(|outport, _iter| {
            if let Some(pri) = json.outports.clone().get(outport) {
                graph.add_outport(
                    outport,
                    &pri.clone().process,
                    &graph.get_port_name(&pri.port),
                    pri.metadata.clone(),
                );
            }
        });

        for group in json.groups.clone() {
            graph.add_group(&group.name, group.nodes, group.metadata);
        }
     
        graph.end_transaction("load_json", metadata.clone());
       

        graph
    }

    pub fn from_json_string(
        source: &str,
        metadata: Option<Map<String, Value>>,
    ) -> Result<Graph, anyhow::Error> {
        let json = serde_json::from_str::<GraphJson>(source)?;
        let graph = Self::from_json(json, metadata);
        Ok(graph)
    }

    /// Convert FBP notation to ZFlow Graph
    pub fn from_fbp(
        source: &str,
        name: &str,
        case_sensitive: bool,
    ) -> Result<Graph, anyhow::Error> {
        let graph_json = fbp::FBP::new(source, name, case_sensitive).parse()?;
        let graph_json = GraphJson::deserialize(json!(graph_json))?;
        return Ok(Self::from_json(graph_json, None));
    }

    /// Save Graph to file
    pub fn save(&self, path: &str) -> Result<(), io::Error> {
        let mut file_res = File::create(path);
        if file_res.is_err() {
            return Err(file_res.err().unwrap());
        }
        if let Ok(file) = file_res.as_mut() {
            let json = self.to_json();
            let data = serde_json::to_string(&json)?;
            file.write_all(data.as_bytes())?;
            return Ok(());
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Can't save file",
        ))
    }
   
    pub fn load_file(path: &str, metadata: Option<Map<String, Value>>) -> Result<Graph, anyhow::Error> {
        if let Ok(file) = File::open(path).as_mut() {
            let mut json_str = String::from("");
            file.read_to_string(&mut json_str)?;
            return Graph::from_json_string(json_str.as_str(), metadata);
        }

        Err(anyhow::Error::msg(
            "Can't load file",
        ))
    }
    #[cfg(not(feature = "build_wasm"))]
    pub fn to_static(&self) -> &'static mut Graph {
        Box::leak(Box::new(self.clone()))
    }

    /// Send event
    #[cfg(feature = "build_wasm")]
    pub fn emit(&mut self, name: &str, data: JsValue) {

        // self.p
        // self.listeners
        //     .clone()
        //     .get_mut(name)
        //     .iter()
        //     .for_each(|actions| {
        //         (*actions).iter().enumerate().foreach(|actor, _| {
        //             if actor.1.once {
        //                 self.listeners.get_mut(name).unwrap().remove(actor.0);
        //             }
        //             if let Ok(mut callback) = actor.1.callback.lock() {
        //                 callback(self, data.clone());
        //             }
        //         })
        //     });
    }
    /// Remove listeners from event
    #[cfg(feature = "build_wasm")]
    pub fn disconnect(&mut self, name: &str) {
        // self.listeners.remove(name);
    }
    /// Check if we have events
    #[cfg(feature = "build_wasm")]
    pub fn has_event(&self, name: &str) -> bool {
        // self.listeners.contains_key(name)
        false
    }

     /// Attach listener to an event
     #[cfg(feature = "build_wasm")]
     fn on(
        &mut self,
        name: &'static str,
        rec: &web_sys::js_sys::Function,
        once: bool,
    ) {
        // if !self.listeners.contains_key(name) {
        //     self.listeners.insert(name, Vec::new());
        // }
        // if let Some(v) = self.listeners.get_mut(name) {
        //     v.push(EventActor {
        //         once,
        //         callback: Arc::new(Mutex::new(rec)),
        //     });
        // }
    }
    #[cfg(feature = "build_wasm")]
     fn once(
        &mut self,
        name: &'static str,
        rec: &web_sys::js_sys::Function,
    ) {
        // if !self.listeners.contains_key(name) {
        //     self.listeners.insert(name, Vec::new());
        // }
        // if let Some(v) = self.listeners.get_mut(name) {
        //     v.push(EventActor {
        //         once,
        //         callback: Arc::new(Mutex::new(rec)),
        //     });
        // }
    }
}
