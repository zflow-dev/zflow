///    FBP Graph Journal
///    (c) 2022 Damilare Akinlaja
///    (c) 2016-2020 Flowhub UG
///    (c) 2014 Jon Nordby
///    (c) 2013 Flowhub UG
///    (c) 2011-2012 Henri Bergius, Nemein
///    FBP Graph may be freely distributed under the MIT license

use crate::internal::event_manager::EventManager;
use foreach::ForEach;
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{
    graph::Graph,
    types::{GraphEdge, GraphExportedPort, GraphGroup, GraphIIP, GraphLeaf},
};
use crate::graph::types::GraphNode;

#[derive(Clone, Serialize, Deserialize)]
pub struct TransactionEntry {
    pub cmd: Option<String>,
    pub args: Option<Value>,
    pub rev: Option<i32>,
    pub old: Option<Map<String, Value>>,
    pub new: Option<Map<String, Value>>,
}

pub trait JournalStore<'a>: EventManager<'a> {
    fn count_transactions(&self) -> usize;
    fn put_transaction(&mut self, rev_id: usize, entry: Vec<TransactionEntry>);
    fn fetch_transaction(&mut self, rev_id: usize) -> Option<&mut Vec<TransactionEntry>>;
}

impl<'a> JournalStore<'a> for Graph<'a> {
    fn count_transactions(&self) -> usize {
        self.transactions.len()
    }

    fn put_transaction(&mut self, rev_id: usize, entries: Vec<TransactionEntry>) {
        if rev_id > self.last_revision {
            self.last_revision = rev_id;
        }
        self.emit("transaction", &(rev_id, entries.clone()));
        self.transactions.insert(rev_id, entries);
    }

    fn fetch_transaction(&mut self, rev_id: usize) -> Option<&mut Vec<TransactionEntry>> {
        self.transactions.get_mut(rev_id)
    }
}

/// Journalling graph changes
///
/// The Journal can follow graph changes, store them
/// and allows to recall previous revisions of the graph.
///
/// Revisions stored in the journal follow the transactions of the graph.
/// It is not possible to operate on smaller changes than individual transactions.
/// Use start_transaction and end_transaction on Graph to structure the revisions logical changesets.
///

trait Journal<'a>: EventManager<'a> {
    fn init_journal(&mut self, metadata: Option<Map<String, Value>>) -> &mut Self;

    fn append_command(&mut self, cmd: &str, args: Value, rev: Option<i32>) -> &mut Self;
    fn start_journal(&mut self, id: &str, meta: Option<Map<String, Value>>) -> &mut Self;
    fn end_journal(&mut self, id: &str, meta: Option<Map<String, Value>>) -> &mut Self;
    fn execute_entry(&mut self, entry: TransactionEntry) -> &mut Self;
    fn execute_entry_inversed(&mut self, entry: TransactionEntry) -> &mut Self;
    fn move_to_revision(&mut self, rev_id: i32) -> &mut Self;
    /// Undo the last graph change
    fn undo(&mut self) -> &mut Self;
    /// Redo the last undo
    fn redo(&mut self) -> &mut Self;
    /// If there is something to redo
    fn can_redo(&self) -> bool;
    /// If there is something to undo
    fn can_undo(&self) -> bool;
}

impl<'a> Journal<'a> for Graph<'a> {
    fn init_journal(&mut self, metadata: Option<Map<String, Value>>) -> &mut Self {
        self.subscribed = true;

        self.entries = Vec::new();

        if self.count_transactions() == 0 {
            // Sync journal with current graph to start transaction history
            self.current_revision = -1;

            self.start_journal("initial", metadata.clone());
            self.nodes.clone().iter().foreach(|node, _| {
                self.append_command("add_node", json!(node), None);
            });
            self.edges.clone().iter().foreach(|edge, _| {
                self.append_command("add_edge", json!(edge), None);
            });
            self.initializers.clone().iter().foreach(|iip, _| {
                self.append_command("add_initial", json!(iip), None);
            });

            if self.properties.clone().keys().len() > 0 {
                self.append_command("change_properties", json!(self.properties), None);
            }

            self.inports.clone().keys().foreach(|name, _| {
                self.append_command(
                    "add_inport",
                    json!({
                        "name": name,
                        "port": self.inports.get(name).unwrap(),
                    }),
                    None,
                );
            });
            self.outports.clone().keys().foreach(|name, _| {
                self.append_command(
                    "add_outport",
                    json!({
                        "name": name,
                        "port": self.outports.get(name).unwrap(),
                    }),
                    None,
                );
            });

            self.groups.clone().iter().foreach(|group, _| {
                self.append_command("add_group", json!(group), None);
            });

            self.end_journal("initial", metadata.clone());
        } else {
            // Persistent store, start with its latest rev
            self.current_revision = self.last_revision as i32;
        }

        // Subscribe to graph changes
        self.connect(
            "add_node",
            |this, data| {
                this.append_command(
                    "add_node",
                    json!(data.downcast_ref::<GraphNode>().unwrap()),
                    None,
                );
            },
            false,
        );
        self.connect(
            "remove_node",
            |this, data| {
                this.append_command(
                    "remove_node",
                    json!(data.downcast_ref::<GraphNode>().unwrap()),
                    None,
                );
            },
            false,
        );

        self.connect(
            "rename_node",
            |this, data| {
                let (old_name, new_name) = data.downcast_ref::<(String, String)>().unwrap();
                this.append_command(
                    "rename_node",
                    json!({
                        "old_id": *old_name,
                        "new_id": *new_name
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "change_node",
            |this, data| {
                let (node, old, new) = data
                    .downcast_ref::<(GraphNode, Option<Map<String, Value>>, Map<String, Value>)>()
                    .unwrap();
                this.append_command(
                    "change_node",
                    json!({
                        "id": node.id,
                        "new": *new,
                        "old": *old
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "add_edge",
            |this, data| {
                this.append_command(
                    "add_edge",
                    json!(data.downcast_ref::<GraphEdge>().unwrap()),
                    None,
                );
            },
            false,
        );
        self.connect(
            "remove_edge",
            |this, data| {
                this.append_command(
                    "remove_edge",
                    json!(data.downcast_ref::<GraphEdge>().unwrap()),
                    None,
                );
            },
            false,
        );
        self.connect(
            "change_edge",
            |this, data| {
                let (edge, old_meta, _) = data
                    .downcast_ref::<(GraphEdge, Option<Map<String, Value>>, Map<String, Value>)>()
                    .unwrap();
                let old = if old_meta.is_none() {
                    json!(null)
                } else {
                    json!(old_meta.clone().unwrap())
                };
                let new = if edge.metadata.is_none() {
                    json!(null)
                } else {
                    json!(edge.metadata.clone().unwrap())
                };
                this.append_command(
                    "change_edge",
                    json!({
                        "from": edge.from,
                        "to": edge.to,
                        "new": new,
                        "old": old
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "add_initial",
            |this, data| {
                this.append_command(
                    "add_initial",
                    json!(data.downcast_ref::<GraphIIP>().unwrap()),
                    None,
                );
            },
            false,
        );
        self.connect(
            "remove_initial",
            |this, data| {
                this.append_command(
                    "remove_initial",
                    json!(data.downcast_ref::<GraphIIP>().unwrap()),
                    None,
                );
            },
            false,
        );

        self.connect(
            "change_properties",
            |this, data| {
                let (new_props, old_props) = data
                    .downcast_ref::<(Map<String, Value>, Map<String, Value>)>()
                    .unwrap();
                this.append_command(
                    "change_properties",
                    json!({
                        "old": *old_props,
                        "new": *new_props
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "add_group",
            |this, data| {
                this.append_command(
                    "add_group",
                    json!(data.downcast_ref::<GraphGroup>().unwrap()),
                    None,
                );
            },
            false,
        );

        self.connect(
            "rename_group",
            |this, data| {
                let (old_name, new_name) = data.downcast_ref::<(String, String)>().unwrap();
                this.append_command(
                    "rename_group",
                    json!({
                        "old_name": *old_name,
                        "new_name": *new_name
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "remove_group",
            |this, data| {
                this.append_command(
                    "remove_group",
                    json!(data.downcast_ref::<GraphGroup>().unwrap()),
                    None,
                );
            },
            false,
        );

        self.connect(
            "change_group",
            |this, data| {
                let (group, old, _) = data
                    .downcast_ref::<(GraphGroup, Option<Map<String, Value>>, Map<String, Value>)>()
                    .unwrap();
                let old = if old.is_none() {
                    json!(null)
                } else {
                    json!(old.clone().unwrap())
                };
                let new = if group.metadata.is_none() {
                    json!(null)
                } else {
                    json!(group.metadata.clone().unwrap())
                };
                this.append_command(
                    "change_group",
                    json!({
                        "name": group.name,
                        "new": new,
                        "old": old
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "add_inport",
            |this, data| {
                let (name, port) = data.downcast_ref::<(String, GraphExportedPort)>().unwrap();
                this.append_command(
                    "add_inport",
                    json!({
                        "name": name,
                        "port": *port
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "remove_inport",
            |this, data| {
                let (name, port) = data
                    .downcast_ref::<(String, Option<GraphExportedPort>)>()
                    .unwrap();
                this.append_command(
                    "remove_inport",
                    json!({
                        "name": name,
                        "port": *port
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "rename_inport",
            |this, data| {
                let (old_name, new_name) = data.downcast_ref::<(String, String)>().unwrap();
                this.append_command(
                    "rename_inport",
                    json!({
                        "old_id": *old_name,
                        "new_id": *new_name
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "change_inport",
            |this, data| {
                let (name, port, old, _) = data
                    .downcast_ref::<(
                        String,
                        GraphExportedPort,
                        Map<String, Value>,
                        Map<String, Value>,
                    )>()
                    .unwrap();
                this.append_command(
                    "change_inport",
                    json!({
                        "name": name,
                        "new": port.metadata,
                        "old": *old
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "add_outport",
            |this, data| {
                let (name, port) = data.downcast_ref::<(String, GraphExportedPort)>().unwrap();
                this.append_command(
                    "add_outport",
                    json!({
                        "name": name,
                        "port": *port
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "remove_outport",
            |this, data| {
                let (name, port) = data
                    .downcast_ref::<(String, Option<GraphExportedPort>)>()
                    .unwrap();
                this.append_command(
                    "remove_outport",
                    json!({
                        "name": name,
                        "port": *port
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "rename_outport",
            |this, data| {
                let (old_name, new_name) = data.downcast_ref::<(String, String)>().unwrap();
                this.append_command(
                    "rename_outport",
                    json!({
                        "old_id": *old_name,
                        "new_id": *new_name
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "change_outport",
            |this, data| {
                let (name, port, old, _) = data
                    .downcast_ref::<(
                        String,
                        GraphExportedPort,
                        Map<String, Value>,
                        Map<String, Value>,
                    )>()
                    .unwrap();
                this.append_command(
                    "change_outport",
                    json!({
                        "name": name,
                        "new": port.metadata,
                        "old": *old
                    }),
                    None,
                );
            },
            false,
        );

        self.connect(
            "start_transaction",
            |this, data| {
                let (id, meta) = data
                    .downcast_ref::<(String, Option<Map<String, Value>>)>()
                    .unwrap();
                this.start_journal(id, meta.clone());
            },
            false,
        );

        self.connect(
            "end_transaction",
            |this, data| {
                let (id, meta) = data
                    .downcast_ref::<(String, Option<Map<String, Value>>)>()
                    .unwrap();
                this.end_journal(id, meta.clone());
            },
            false,
        );

        self
    }

    fn append_command(&mut self, cmd: &str, args: Value, rev: Option<i32>) -> &mut Self {
        if !self.subscribed {
            return self;
        }

        let entry = TransactionEntry {
            cmd: Some(cmd.to_owned()),
            args: Some(args),
            rev,
            old: None,
            new: None,
        };

        self.entries.push(entry);

        self
    }

    fn start_journal(&mut self, id: &str, meta: Option<Map<String, Value>>) -> &mut Self {
        if !self.subscribed {
            return self;
        }
        if !self.entries.is_empty() {
            error!("Inconsistent @entries");
            return self;
        }
        self.current_revision += 1;
        self.append_command(
            "start_transaction",
            json!({
              "id": id,
              "metadata": meta,
            }),
            Some(self.current_revision),
        );
        self
    }

    fn end_journal(&mut self, id: &str, meta: Option<Map<String, Value>>) -> &mut Self {
        if !self.subscribed {
            return self;
        }
        self.append_command(
            "end_transaction",
            json!({
              "id": id,
              "metadata": meta,
            }),
            Some(self.current_revision),
        );

        self.put_transaction(
            self.current_revision.try_into().unwrap(),
            self.entries.clone(),
        );
        self.entries = Vec::new();
        self
    }

    fn execute_entry(&mut self, entry: TransactionEntry) -> &mut Self {
        let a = entry.args.clone();
        if let Some(a) = a {
            if let Some(cmd) = entry.cmd.as_ref() {
                match cmd.as_str() {
                    "add_node" => {
                        let a = a.as_object().unwrap();
                        self.add_node(
                            a.get("id").unwrap().as_str().unwrap(),
                            a.get("component").unwrap().as_str().unwrap(),
                            None,
                        );
                    }
                    "remove_node" => {
                        let a = a.as_object().unwrap();
                        self.remove_node(a.get("id").unwrap().as_str().unwrap());
                    }
                    "rename_node" => {
                        let a = a.as_object().unwrap();
                        self.rename_node(
                            a.get("old_id").unwrap().as_str().unwrap(),
                            a.get("new_id").unwrap().as_str().unwrap(),
                        );
                    }
                    "change_node" => {
                        let a = a.as_object().unwrap();
                        let id = a.get("id").unwrap().as_str().unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap();
                        if let Some(old) = a.get("old").unwrap().as_object() {
                            self.set_node_metadata(id, calculate_meta(old.clone(), new.clone()));
                        } else {
                            self.set_node_metadata(id, calculate_meta(Map::new(), new.clone()));
                        }
                    }
                    "add_edge" => {
                        let edge = GraphEdge::deserialize(&a);
                        if let Ok(edge) = edge {
                            self.add_edge(
                                &edge.from.node_id,
                                &edge.from.port,
                                &edge.to.node_id,
                                &edge.to.port,
                                None,
                            );
                        }
                    }
                    "remove_edge" => {
                        let edge = GraphEdge::deserialize(&a);
                        if let Ok(edge) = edge {
                            self.remove_edge(
                                &edge.from.node_id,
                                &edge.from.port,
                                Some(&edge.to.node_id),
                                Some(&edge.to.port),
                            );
                        }
                    }
                    "change_edge" => {
                        let from = GraphLeaf::deserialize(a.get("from").unwrap()).unwrap();
                        let to = GraphLeaf::deserialize(a.get("to").unwrap()).unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap().clone();
                        let old = a.get("old").unwrap().as_object().unwrap().clone();
                        self.set_edge_metadata(
                            &from.node_id,
                            &from.port,
                            &to.node_id,
                            &to.port,
                            calculate_meta(old, new),
                        );
                    }
                    "add_initial" => {
                        let iip = GraphIIP::deserialize(&a);
                        if let Ok(iip) = iip {
                            if iip.to.is_some() && iip.from.is_some() {
                                let to = iip.to.unwrap();
                                if to.index.is_some() {
                                    self.add_initial_index(
                                        iip.from.unwrap().data,
                                        &to.node_id,
                                        &to.port,
                                        to.index,
                                        iip.metadata,
                                    );
                                } else {
                                    self.add_initial(
                                        iip.from.unwrap().data,
                                        &to.node_id,
                                        &to.port,
                                        iip.metadata,
                                    );
                                }
                            }
                        }
                    }
                    "remove_initial" => {
                        let iip = GraphIIP::deserialize(&a).unwrap();
                        let to = iip.to.unwrap();
                        self.remove_initial(&to.node_id, &to.port);
                    }
                    "start_transaction" => {}
                    "end_transaction" => {}
                    "change_properties" => {
                        let a = a.as_object().unwrap();
                        self.set_properties(a.get("new").unwrap().as_object().unwrap().clone());
                    }
                    "add_group" => {
                        if let Ok(group) = GraphGroup::deserialize(&a) {
                            self.add_group(&group.name, group.nodes, group.metadata);
                        }
                    }
                    "rename_group" => {
                        let a = a.as_object().unwrap();
                        self.rename_node(
                            a.get("old_name").unwrap().as_str().unwrap(),
                            a.get("new_name").unwrap().as_str().unwrap(),
                        );
                    }
                    "remove_group" => {
                        if let Ok(group) = GraphGroup::deserialize(&a) {
                            self.remove_group(&group.name);
                        }
                    }
                    "change_group" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap();
                        let old = a.get("old").unwrap().as_object().unwrap();
                        self.set_group_metadata(name, calculate_meta(old.clone(), new.clone()));
                    }
                    "add_inport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let port = a.get("port").unwrap();
                        if let Ok(inport) = GraphExportedPort::deserialize(port) {
                            self.add_inport(name, &inport.process, &inport.port, inport.metadata);
                        }
                    }
                    "remove_inport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        self.remove_inport(name);
                    }
                    "rename_inport" => {
                        let a = a.as_object().unwrap();
                        self.rename_inport(
                            a.get("old_id").unwrap().as_str().unwrap(),
                            a.get("new_id").unwrap().as_str().unwrap(),
                        );
                    }
                    "change_inport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap();
                        let old = a.get("old").unwrap().as_object().unwrap();
                        self.set_inports_metadata(name, calculate_meta(old.clone(), new.clone()));
                    }
                    "add_outport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let port = a.get("port").unwrap();
                        if let Ok(inport) = GraphExportedPort::deserialize(port) {
                            self.add_outport(name, &inport.process, &inport.port, inport.metadata);
                        }
                    }
                    "remove_outport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        self.remove_outport(name);
                    }
                    "rename_outport" => {
                        let a = a.as_object().unwrap();
                        self.rename_outport(
                            a.get("old_id").unwrap().as_str().unwrap(),
                            a.get("new_id").unwrap().as_str().unwrap(),
                        );
                    }
                    "change_outport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap();
                        let old = a.get("old").unwrap().as_object().unwrap();
                        self.set_outports_metadata(name, calculate_meta(old.clone(), new.clone()));
                    }
                    &_ => {
                        log::error!("`Unknown journal entry: {}", cmd);
                        panic!();
                    }
                }
            }
        }
        self
    }

    fn execute_entry_inversed(&mut self, entry: TransactionEntry) -> &mut Self {
        let a = entry.args.clone();
        if let Some(a) = a {
            if let Some(cmd) = entry.cmd.as_ref() {
                match cmd.as_str() {
                    "add_node" => {
                        let a = a.as_object().unwrap();
                        self.remove_node(a.get("id").unwrap().as_str().unwrap());
                    }
                    "remove_node" => {
                        let a = a.as_object().unwrap();
                        self.add_node(
                            a.get("id").unwrap().as_str().unwrap(),
                            a.get("component").unwrap().as_str().unwrap(),
                            None,
                        );
                    }
                    "rename_node" => {
                        let a = a.as_object().unwrap();
                        self.rename_node(
                            a.get("new_id").unwrap().as_str().unwrap(),
                            a.get("old_id").unwrap().as_str().unwrap(),
                        );
                    }
                    "change_node" => {
                        let a = a.as_object().unwrap();
                        let id = a.get("id").unwrap().as_str().unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap();

                        if let Some(old) = a.get("old").unwrap().as_object() {
                            self.set_node_metadata(id, calculate_meta(new.clone(), old.clone()));
                        } else {
                            let meta = calculate_meta(new.clone(), Map::new());
                            self.set_node_metadata(id, meta);
                        }
                    }
                    "add_edge" => {
                        if let Ok(edge) = GraphEdge::deserialize(&a) {
                            self.remove_edge(
                                &edge.from.node_id,
                                &edge.from.port,
                                Some(&edge.to.node_id),
                                Some(&edge.to.port),
                            );
                        }
                    }
                    "remove_edge" => {
                        let edge = GraphEdge::deserialize(&a);
                        if let Ok(edge) = edge {
                            self.add_edge(
                                &edge.from.node_id,
                                &edge.from.port,
                                &edge.to.node_id,
                                &edge.to.port,
                                None,
                            );
                        }
                    }
                    "change_edge" => {
                        let from = GraphLeaf::deserialize(a.get("from").unwrap()).unwrap();
                        let to = GraphLeaf::deserialize(a.get("to").unwrap()).unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap().clone();
                        let old = a.get("old").unwrap().as_object().unwrap().clone();

                        self.set_edge_metadata(
                            &from.node_id,
                            &from.port,
                            &to.node_id,
                            &to.port,
                            calculate_meta(new, old),
                        );
                    }
                    "add_initial" => {
                        let iip = GraphIIP::deserialize(&a).unwrap();
                        let to = iip.to.unwrap();
                        self.remove_initial(&to.node_id, &to.port);
                    }
                    "remove_initial" => {
                        let iip = GraphIIP::deserialize(&a);
                        if let Ok(iip) = iip {
                            if iip.to.is_some() && iip.from.is_some() {
                                let to = iip.to.unwrap();
                                if to.index.is_some() {
                                    self.add_initial_index(
                                        iip.from.unwrap().data,
                                        &to.node_id,
                                        &to.port,
                                        to.index,
                                        iip.metadata,
                                    );
                                } else {
                                    self.add_initial(
                                        iip.from.unwrap().data,
                                        &to.node_id,
                                        &to.port,
                                        iip.metadata,
                                    );
                                }
                            }
                        }
                    }
                    "start_transaction" => {}
                    "end_transaction" => {}
                    "change_properties" => {
                        let a = a.as_object().unwrap();
                        self.set_properties(a.get("old").unwrap().as_object().unwrap().clone());
                    }
                    "add_group" => {
                        if let Ok(group) = GraphGroup::deserialize(&a) {
                            self.remove_group(&group.name);
                        }
                    }
                    "rename_group" => {
                        let a = a.as_object().unwrap();
                        self.rename_node(
                            a.get("new_name").unwrap().as_str().unwrap(),
                            a.get("old_name").unwrap().as_str().unwrap(),
                        );
                    }
                    "remove_group" => {
                        if let Ok(group) = GraphGroup::deserialize(&a) {
                            self.add_group(&group.name, group.nodes, group.metadata);
                        }
                    }
                    "change_group" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap();
                        let old = a.get("old").unwrap().as_object().unwrap();
                        self.set_group_metadata(name, calculate_meta(new.clone(), old.clone()));
                    }
                    "add_inport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        self.remove_inport(name);
                    }
                    "remove_inport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let port = a.get("port").unwrap();
                        if let Ok(inport) = GraphExportedPort::deserialize(port) {
                            self.add_inport(name, &inport.process, &inport.port, inport.metadata);
                        }
                    }
                    "rename_inport" => {
                        let a = a.as_object().unwrap();
                        self.rename_inport(
                            a.get("new_id").unwrap().as_str().unwrap(),
                            a.get("old_id").unwrap().as_str().unwrap(),
                        );
                    }
                    "change_inport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap();
                        let old = a.get("old").unwrap().as_object().unwrap();
                        self.set_inports_metadata(name, calculate_meta(new.clone(), old.clone()));
                    }
                    "add_outport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        self.remove_outport(name);
                    }
                    "remove_outport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let port = a.get("port").unwrap();
                        if let Ok(outport) = GraphExportedPort::deserialize(port) {
                            self.add_outport(
                                name,
                                &outport.process,
                                &outport.port,
                                outport.metadata,
                            );
                        }
                    }
                    "rename_outport" => {
                        let a = a.as_object().unwrap();
                        self.rename_inport(
                            a.get("new_id").unwrap().as_str().unwrap(),
                            a.get("old_id").unwrap().as_str().unwrap(),
                        );
                    }
                    "change_outport" => {
                        let a = a.as_object().unwrap();
                        let name = a.get("name").unwrap().as_str().unwrap();
                        let new = a.get("new").unwrap().as_object().unwrap();
                        let old = a.get("old").unwrap().as_object().unwrap();
                        self.set_inports_metadata(name, calculate_meta(new.clone(), old.clone()));
                    }
                    &_ => {
                        log::error!("`Unknown journal entry: {}", cmd);
                        panic!();
                    }
                }
            }
        }
        self
    }
    fn move_to_revision(&mut self, rev_id: i32) -> &mut Self {
        if rev_id == self.current_revision {
            return self;
        }

        self.subscribed = false;
        if rev_id > self.current_revision {
            // Forward replay journal to revId
            let (mut r, end, asc) = {
                let start = self.current_revision + 1;
                let r = start;
                let end = rev_id;
                let asc = start <= end;
                (r, end, asc)
            };
            while if asc { r <= end } else { r >= end } {
                if let Some(transaction) = self.fetch_transaction(rev_id as usize) {
                    transaction.clone().iter().foreach(|entry, _| {
                        self.execute_entry(entry.clone());
                    });
                }
                if asc {
                    r += 1;
                } else {
                    r -= 1;
                }
            }
        } else {
            // Move backwards, and apply inverse changes
            let (mut r, end) = {
                let r = self.current_revision;
                let end = rev_id + 1;
                (r, end)
            };
            while r >= end {
                // Apply entries in reverse order
                if let Some(_entries) = self.fetch_transaction(r as usize) {
                    let mut entries = _entries.clone();
                    entries.reverse();
                    entries.iter().foreach(|entry, _| {
                        self.execute_entry_inversed(entry.clone());
                    });
                }
                // end = rev_id + 1;
                r -= 1;
            }
        }
        self.current_revision = rev_id;
        self.subscribed = true;
        self
    }

    fn undo(&mut self) -> &mut Self {
        if !self.can_undo() {
            return self;
        }
        self.move_to_revision(self.current_revision - 1);
        self
    }

    fn redo(&mut self) -> &mut Self {
        if !self.can_undo() {
            return self;
        }
        self.move_to_revision(self.current_revision + 1);
        self
    }

    fn can_redo(&self) -> bool {
        self.current_revision < self.last_revision as i32
    }

    fn can_undo(&self) -> bool {
        self.current_revision > 0
    }
}

/// To set, not just update (append) metadata
fn calculate_meta(old: Map<String, Value>, new: Map<String, Value>) -> Map<String, Value> {
    let mut set_meta = Map::new();
    old.keys().foreach(|key, _| {
        //set_meta.insert(key.clone(), Value::Null);
        set_meta.remove(key);
    });
    new.keys().foreach(|key, _| {
        set_meta.insert(key.clone(), new.get(key).unwrap().clone());
    });
    set_meta
}

#[cfg(test)]
mod tests {
    use crate::graph::graph::Graph;
    use crate::graph::journal::Journal;
    use assert_json_diff::assert_json_eq;
    use beady::scenario;
    use serde_json::json;

    #[scenario]
    #[test]
    fn fbp_graph_journal() {
        'given_a_journaled_graph: {
            'when_connected_to_initialized_graph: {
                let mut g = Graph::new("", false);
                g.add_node("Foo", "Bar", None)
                    .add_node("Baz", "Foo", None)
                    .add_edge("Foo", "out", "Baz", "in", None)
                    .init_journal(None);
                'then_it_should_have_just_the_initial_transaction: {
                    assert_eq!(g.last_revision, 0);
                }
            }
            'when_following_basic_graph_changes: {
                let mut g = Graph::new("", false);
                g.init_journal(None);
                'then_it_should_create_one_transaction_per_change: {
                    g.add_node("Foo", "Bar", None)
                        .add_node("Baz", "Foo", None)
                        .add_edge("Foo", "out", "Baz", "in", None);

                    assert_eq!(g.last_revision, 3);
                    g.remove_node("Baz");
                    assert_eq!(g.last_revision, 4);
                }
            }
            'when_printing_to_pretty_string: {}
            'when_jumping_to_revision: {
                let mut g = Graph::new("", false);
                g.init_journal(None)
                    .add_node("Foo", "Bar", None)
                    .add_node("Baz", "Foo", None)
                    .add_edge("Foo", "out", "Baz", "in", None)
                    .add_initial(json!(42), "Foo", "in", None)
                    .remove_node("Foo");
                'then_it_should_change_the_graph: {
                    // g.move_to_revision(0);
                    // assert_eq!(g.nodes.len(), 0);
                    g.move_to_revision(2);
                    assert_eq!(g.nodes.len(), 2);
                    g.move_to_revision(5);
                    assert_eq!(g.nodes.len(), 1);
                }
            }
            'when_linear_undo_or_redo: {
                let mut g = Graph::new("", false);
                g.init_journal(None)
                    .add_node("Foo", "Bar", None)
                    .add_node("Baz", "Foo", None)
                    .add_edge("Foo", "out", "Baz", "in", None)
                    .add_initial(json!(42), "Foo", "in", None);

                let graph_before_error = futures::executor::block_on(g.to_json());
                'then_undo_should_restore_previous_revision: {
                    assert_eq!(g.nodes.len(), 2);
                    g.remove_node("Foo");
                    assert_eq!(g.nodes.len(), 1);
                    g.undo();
                    assert_eq!(g.nodes.len(), 2);
                    assert_json_eq!(futures::executor::block_on(g.to_json()), graph_before_error);

                    'and_then_it_redo_should_apply_the_same_change_again: {
                        g.redo();
                        assert_eq!(g.nodes.len(), 1);

                        'and_then_undo_should_also_work_multiple_revisions_back: {
                            g.remove_node("Baz");
                            g.undo();
                            g.undo();
                            assert_eq!(g.nodes.len(), 2);
                            assert_json_eq!(
                                futures::executor::block_on(g.to_json()),
                                graph_before_error
                            );
                        }
                    }
                }
            }
            'when_undo_or_redo_of_metadata_changes: {
                let mut g = Graph::new("", false);
                g.init_journal(None)
                    .add_node("Foo", "Bar", None)
                    .add_node("Baz", "Foo", None)
                    .add_edge("Foo", "out", "Baz", "in", None);

                'then_when_adding_group: {
                    g.add_group(
                        "all",
                        ["Foo".to_owned(), "Bax".to_owned()].to_vec(),
                        Some(json!({"label": "all nodes"}).as_object().unwrap().clone()),
                    );
                    assert_eq!(g.groups.len(), 1);
                    assert_eq!(g.groups[0].name, "all");

                    'and_then_when_undoing_group_add: {
                        g.undo();
                        assert_eq!(g.groups.len(), 0);

                        'and_then_when_redoing_group_add: {
                            g.redo();
                            assert_eq!(g.groups.len(), 1);

                            'and_then_when_changing_group_metadata_adds_revision: {
                                let r = g.last_revision.clone();
                                g.set_group_metadata(
                                    "all",
                                    json!({"label": "ALL NODES!"}).as_object().unwrap().clone(),
                                );
                                assert_eq!(g.last_revision, r + 1);

                                'and_then_when_undoing_group_metadata_change: {
                                    g.undo();
                                    assert_eq!(
                                        g.groups[0].metadata.as_ref().unwrap().get("label"),
                                        Some(&json!("all nodes"))
                                    );

                                    'and_then_when_redoing_group_metadata_change: {
                                        g.redo();
                                        assert_eq!(
                                            g.groups[0].metadata.as_ref().unwrap().get("label"),
                                            Some(&json!("ALL NODES!"))
                                        );

                                        'and_then_when_setting_node_metadata: {
                                            g.set_node_metadata(
                                                "Foo",
                                                json!({"oneone": json!(11), "2": "two"})
                                                    .as_object()
                                                    .unwrap()
                                                    .clone(),
                                            );

                                            assert_eq!(
                                                g.get_node("Foo")
                                                    .unwrap()
                                                    .metadata
                                                    .as_ref()
                                                    .unwrap()
                                                    .keys()
                                                    .len(),
                                                2
                                            );

                                            'and_then_when_undoing_node_metadata_change: {
                                                g.undo();

                                                assert_eq!(
                                                    g.get_node("Foo")
                                                        .unwrap()
                                                        .metadata
                                                        .as_ref()
                                                        .unwrap()
                                                        .keys()
                                                        .len(),
                                                    0
                                                );

                                                'and_then_when_redoing_node_metadata_change:{
                                                    g.redo();
                                                    let node = g.get_node("Foo").unwrap();
                                                    assert_eq!(node.metadata.as_ref().unwrap().get("oneone"), Some(&json!(11)));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
