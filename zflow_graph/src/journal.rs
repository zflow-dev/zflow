///    FBP Graph Journal
///    (c) 2022 Damilare Akinlaja
///    (c) 2016-2020 Flowhub UG
///    (c) 2014 Jon Nordby
///    (c) 2013 Flowhub UG
///    (c) 2011-2012 Henri Bergius, Nemein
///    FBP Graph may be freely distributed under the MIT license
use crate::internal::event_manager::EventListener;
use foreach::ForEach;

use log::error;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{
    graph::Graph,
    types::{GraphEdge, GraphEvents, GraphExportedPort, GraphGroup, GraphIIP},
};
use crate::types::GraphNode;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TransactionEntry {
    pub cmd: Option<GraphEvents>,
    pub rev: Option<i32>,
    pub old: Option<Map<String, Value>>,
    pub new: Option<Map<String, Value>>,
}

pub trait JournalStore {
    fn count_transactions(&self) -> usize;
    fn put_transaction(&mut self, rev_id: i32, entry: Vec<TransactionEntry>);
    fn fetch_transaction(&mut self, rev_id: i32) -> Vec<TransactionEntry>;
    fn set_subscribed(&mut self, sub: bool);
    fn set_current_rev(&mut self, rev: i32);
    fn get_current_rev(&self) -> i32;
    fn get_last_rev(&self) -> i32;
    fn set_last_rev(&mut self, rev: i32);
    fn append_command(&mut self, cmd: GraphEvents, rev: Option<i32>);
    fn start_journal_transaction(&mut self, id: &str, meta: Option<Map<String, Value>>);
    fn end_journal_transaction(&mut self, id: &str, meta: Option<Map<String, Value>>);
}

impl JournalStore for Graph {
    fn count_transactions(&self) -> usize {
        self.transactions.len()
    }

    fn put_transaction(&mut self, rev_id: i32, entries: Vec<TransactionEntry>) {
        if rev_id > self.last_revision {
            self.last_revision = rev_id;
        }

        self.transactions.insert(rev_id as usize, entries);
    }

    fn fetch_transaction(&mut self, rev_id: i32) -> Vec<TransactionEntry> {
        if let Some(tx) = self
            .transactions
            .iter()
            .find(|entry| entry.iter().find(|tx| tx.rev == Some(rev_id)).is_some())
        {
            return tx.to_vec();
        }
        Vec::new()
    }

    fn set_subscribed(&mut self, sub: bool) {
        self.subscribed = sub;
    }
    fn set_current_rev(&mut self, rev: i32) {
        self.current_revision = rev;
    }

    fn append_command(&mut self, cmd: GraphEvents, rev: Option<i32>) {
        if !self.subscribed {
            return;
        }

        let entry = TransactionEntry {
            cmd: Some(cmd),
            rev,
            old: None,
            new: None,
        };

        self.entries.push(entry);
    }

    fn start_journal_transaction(&mut self, id: &str, meta: Option<Map<String, Value>>) {
        if !self.subscribed {
            return;
        }
        if !self.entries.is_empty() {
            error!("Inconsistent @entries");
            return;
        }
        self.current_revision += 1;
        self.append_command(
            GraphEvents::StartTransaction(json!({
              "id": id,
              "metadata": meta,
            })),
            Some(self.current_revision),
        );
    }

    fn end_journal_transaction(&mut self, id: &str, meta: Option<Map<String, Value>>) {
        if !self.subscribed {
            return;
        }
        self.append_command(
            GraphEvents::EndTransaction(json!({
              "id": id,
              "metadata": meta,
            })),
            Some(self.get_current_rev()),
        );
        let cur = self.get_current_rev();
        self.put_transaction(cur, self.entries.clone());
        self.entries.clear();
    }

    fn get_last_rev(&self) -> i32 {
        self.last_revision
    }
    fn set_last_rev(&mut self, rev: i32) {
        self.last_revision = rev;
    }

    fn get_current_rev(&self) -> i32 {
        self.current_revision
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
pub trait Journal {
    fn execute_entry(&mut self, entry: TransactionEntry);
    fn execute_entry_inversed(&mut self, entry: TransactionEntry);
    fn move_to_revision(&mut self, rev_id: i32);
    /// Undo the last graph change
    fn undo(&mut self);
    /// Redo the last undo
    fn redo(&mut self);
    /// If there is something to redo
    fn can_redo(&self) -> bool;
    /// If there is something to undo
    fn can_undo(&self) -> bool;
    fn start_journal(&mut self, metadata: Option<Map<String, Value>>);
}

impl Journal for Graph {
    fn execute_entry(&mut self, entry: TransactionEntry) {
        if let Some(event) = entry.cmd {
            match event {
                GraphEvents::AddNode(data) => {
                    if let Ok(node) = GraphNode::deserialize(data) {
                        self.add_node(&node.id, &node.component, None);
                    }
                }
                GraphEvents::RemoveNode(data) => {
                    if let Ok(node) = GraphNode::deserialize(data) {
                        self.remove_node(&node.id);
                    }
                }
                GraphEvents::RenameNode(data) => {
                    if let Some(data) = data.as_object() {
                        self.rename_node(
                            data.get("old").unwrap().as_str().unwrap(),
                            data.get("new").unwrap().as_str().unwrap(),
                        );
                    }
                }
                GraphEvents::ChangeNode(data) => {
                    if let Some(data) = data.as_object() {
                        let node = GraphNode::deserialize(data.get("node").unwrap()).unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        if let Some(old) = data.get("old_metadata").unwrap().as_object() {
                            self.set_node_metadata(
                                &node.id,
                                calculate_meta(old.clone(), new.clone()),
                            );
                        } else {
                            self.set_node_metadata(
                                &node.id,
                                calculate_meta(Map::new(), new.clone()),
                            );
                        }
                    }
                }
                GraphEvents::AddEdge(data) => {
                    if let Ok(edge) = GraphEdge::deserialize(data) {
                        self.add_edge(
                            &edge.from.node_id,
                            &edge.from.port,
                            &edge.to.node_id,
                            &edge.to.port,
                            None,
                        );
                    }
                }
                GraphEvents::RemoveEdge(data) => {
                    if let Ok(edge) = GraphEdge::deserialize(data) {
                        self.remove_edge(
                            &edge.from.node_id,
                            &edge.from.port,
                            Some(&edge.to.node_id),
                            Some(&edge.to.port),
                        );
                    }
                }
                GraphEvents::ChangeEdge(data) => {
                    if let Some(data) = data.as_object() {
                        let edge = GraphEdge::deserialize(data.get("edge").unwrap()).unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        let old = data
                            .get("old_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();

                        self.set_edge_metadata(
                            &edge.from.node_id,
                            &edge.from.port,
                            &edge.to.node_id,
                            &edge.to.port,
                            calculate_meta(old.clone(), new.clone()),
                        );
                    }
                }
                GraphEvents::AddInitial(data) => {
                    if let Ok(iip) = GraphIIP::deserialize(data) {
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
                GraphEvents::RemoveInitial(data) => {
                    if let Ok(iip) = GraphIIP::deserialize(data) {
                        let to = iip.to.unwrap();
                        self.remove_initial(&to.node_id, &to.port);
                    }
                }
                GraphEvents::ChangeProperties(data) => {
                    let data = data.as_object().unwrap();
                    self.set_properties(data.get("new").unwrap().as_object().unwrap().clone());
                }
                GraphEvents::AddGroup(data) => {
                    if let Ok(group) = GraphGroup::deserialize(data) {
                        self.add_group(&group.name, group.nodes, group.metadata);
                    }
                }
                GraphEvents::RemoveGroup(data) => {
                    if let Ok(group) = GraphGroup::deserialize(data) {
                        self.remove_group(&group.name);
                    }
                }
                GraphEvents::RenameGroup(data) => {
                    if let Some(a) = data.as_object() {
                        self.rename_group(
                            a.get("old").unwrap().as_str().unwrap(),
                            a.get("new").unwrap().as_str().unwrap(),
                        );
                    }
                }
                GraphEvents::ChangeGroup(data) => {
                    if let Some(data) = data.as_object() {
                        let group = GraphGroup::deserialize(data.get("group").unwrap()).unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        let old = data
                            .get("old_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();

                        self.set_group_metadata(
                            &group.name,
                            calculate_meta(old.clone(), new.clone()),
                        );
                    }
                }
                GraphEvents::AddInport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        let port = data.get("port").unwrap();
                        if let Ok(inport) = GraphExportedPort::deserialize(port) {
                            self.add_inport(name, &inport.process, &inport.port, inport.metadata);
                        }
                    }
                }
                GraphEvents::RemoveInport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        self.remove_inport(name);
                    }
                }
                GraphEvents::RenameInport(data) => {
                    if let Some(data) = data.as_object() {
                        self.rename_inport(
                            data.get("old").unwrap().as_str().unwrap(),
                            data.get("new").unwrap().as_str().unwrap(),
                        );
                    }
                }
                GraphEvents::ChangeInport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        let old = data
                            .get("old_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();

                        self.set_inports_metadata(name, calculate_meta(old.clone(), new.clone()));
                    }
                }
                GraphEvents::AddOutport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        let port = data.get("port").unwrap();
                        if let Ok(inport) = GraphExportedPort::deserialize(port) {
                            self.add_outport(name, &inport.process, &inport.port, inport.metadata);
                        }
                    }
                }
                GraphEvents::RemoveOutport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        self.remove_outport(name);
                    }
                }
                GraphEvents::RenameOutport(data) => {
                    if let Some(data) = data.as_object() {
                        self.rename_outport(
                            data.get("old").unwrap().as_str().unwrap(),
                            data.get("new").unwrap().as_str().unwrap(),
                        );
                    }
                }
                GraphEvents::ChangeOutport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        let old = data
                            .get("old_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();

                        self.set_outports_metadata(name, calculate_meta(old.clone(), new.clone()));
                    }
                }
                GraphEvents::StartTransaction(_) => {}
                GraphEvents::EndTransaction(_) => {}
                _ => {}
            }
        }
    }

    fn execute_entry_inversed(&mut self, entry: TransactionEntry) {
        if let Some(event) = entry.cmd {
            match event {
                GraphEvents::AddNode(data) => {
                    if let Ok(node) = GraphNode::deserialize(data) {
                        self.remove_node(&node.id);
                    }
                }
                GraphEvents::RemoveNode(data) => {
                    if let Ok(node) = GraphNode::deserialize(data) {
                        self.add_node(&node.id, &node.component, node.metadata);
                    }
                }
                GraphEvents::RenameNode(data) => {
                    if let Some(data) = data.as_object() {
                        self.rename_node(
                            data.get("new").unwrap().as_str().unwrap(),
                            data.get("old").unwrap().as_str().unwrap(),
                        );
                    }
                }
                GraphEvents::ChangeNode(data) => {
                    if let Some(data) = data.as_object() {
                        let node = GraphNode::deserialize(data.get("node").unwrap()).unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        if let Some(old) = data.get("old_metadata").unwrap().as_object() {
                            self.set_node_metadata(
                                &node.id,
                                calculate_meta(new.clone(), old.clone()),
                            );
                        } else {
                            self.set_node_metadata(
                                &node.id,
                                calculate_meta(new.clone(), Map::new()),
                            );
                        }
                    }
                }
                GraphEvents::AddEdge(data) => {
                    if let Ok(edge) = GraphEdge::deserialize(data) {
                        self.remove_edge(
                            &edge.from.node_id,
                            &edge.from.port,
                            Some(&edge.to.node_id),
                            Some(&edge.to.port),
                        );
                    }
                }
                GraphEvents::RemoveEdge(data) => {
                    if let Ok(edge) = GraphEdge::deserialize(data) {
                        self.add_edge(
                            &edge.from.node_id,
                            &edge.from.port,
                            &edge.to.node_id,
                            &edge.to.port,
                            None,
                        );
                    }
                }
                GraphEvents::ChangeEdge(data) => {
                    if let Some(data) = data.as_object() {
                        let edge = GraphEdge::deserialize(data.get("edge").unwrap()).unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        let old = data
                            .get("old_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();

                        self.set_edge_metadata(
                            &edge.from.node_id,
                            &edge.from.port,
                            &edge.to.node_id,
                            &edge.to.port,
                            calculate_meta(new.clone(), old.clone()),
                        );
                    }
                }
                GraphEvents::AddInitial(data) => {
                    if let Ok(iip) = GraphIIP::deserialize(data) {
                        let to = iip.to.unwrap();
                        self.remove_initial(&to.node_id, &to.port);
                    }
                }
                GraphEvents::RemoveInitial(data) => {
                    if let Ok(iip) = GraphIIP::deserialize(data) {
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
                GraphEvents::ChangeProperties(data) => {
                    let data = data.as_object().unwrap();
                    self.set_properties(data.get("old").unwrap().as_object().unwrap().clone());
                }
                GraphEvents::AddGroup(data) => {
                    if let Ok(group) = GraphGroup::deserialize(data) {
                        self.remove_group(&group.name);
                    }
                }
                GraphEvents::RemoveGroup(data) => {
                    if let Ok(group) = GraphGroup::deserialize(data) {
                        self.add_group(&group.name, group.nodes, group.metadata);
                    }
                }
                GraphEvents::RenameGroup(data) => {
                    if let Some(a) = data.as_object() {
                        self.rename_group(
                            a.get("new").unwrap().as_str().unwrap(),
                            a.get("old").unwrap().as_str().unwrap(),
                        );
                    }
                }
                GraphEvents::ChangeGroup(data) => {
                    if let Some(data) = data.as_object() {
                        let group = GraphGroup::deserialize(data.get("group").unwrap()).unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        let old = data
                            .get("old_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();

                        self.set_group_metadata(&group.name, calculate_meta(new, old));
                    }
                }
                GraphEvents::AddInport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        self.remove_inport(name);
                    }
                }
                GraphEvents::RemoveInport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        let port = data.get("port").unwrap();
                        if let Ok(inport) = GraphExportedPort::deserialize(port) {
                            self.add_inport(name, &inport.process, &inport.port, inport.metadata);
                        }
                    }
                }
                GraphEvents::RenameInport(data) => {
                    if let Some(data) = data.as_object() {
                        self.rename_inport(
                            data.get("new").unwrap().as_str().unwrap(),
                            data.get("old").unwrap().as_str().unwrap(),
                        );
                    }
                }
                GraphEvents::ChangeInport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        let old = data
                            .get("old_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();

                        self.set_inports_metadata(name, calculate_meta(new, old));
                    }
                }
                GraphEvents::AddOutport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        self.remove_outport(name);
                    }
                }
                GraphEvents::RemoveOutport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        let port = data.get("port").unwrap();
                        if let Ok(inport) = GraphExportedPort::deserialize(port) {
                            self.add_outport(name, &inport.process, &inport.port, inport.metadata);
                        }
                    }
                }
                GraphEvents::RenameOutport(data) => {
                    if let Some(data) = data.as_object() {
                        self.rename_outport(
                            data.get("new").unwrap().as_str().unwrap(),
                            data.get("old").unwrap().as_str().unwrap(),
                        );
                    }
                }
                GraphEvents::ChangeOutport(data) => {
                    if let Some(data) = data.as_object() {
                        let name = data.get("name").unwrap().as_str().unwrap();
                        let new = data
                            .get("new_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();
                        let old = data
                            .get("old_metadata")
                            .unwrap()
                            .as_object()
                            .unwrap()
                            .clone();

                        self.set_outports_metadata(name, calculate_meta(new, old));
                    }
                }
                GraphEvents::StartTransaction(_) => {}
                GraphEvents::EndTransaction(_) => {}
                _ => {}
            }
        }
    }
    fn move_to_revision(&mut self, rev_id: i32) {
        let cur = self.get_current_rev();
        if rev_id == cur {
            return;
        }

        self.set_subscribed(false);
        if rev_id > self.get_current_rev() {
            // Forward replay journal to revId
            let (mut r, _start, end, asc) = {
                let start = self.get_current_rev() + 1;
                let r = start;
                let end = rev_id;
                let asc = start <= end;
                (r, start, end, asc)
            };

            while if asc { r <= end } else { r >= end } {
                self.fetch_transaction(r)
                    .clone()
                    .iter()
                    .foreach(|entry, _| {
                        self.execute_entry(entry.clone());
                    });

                if asc {
                    r += 1;
                } else {
                    r -= 1;
                }
            }
        } else {
            // Move backwards, and apply inverse changes
            let mut r = self.get_current_rev();
            let end = rev_id + 1;
            while r >= end {
                // Apply entries in reverse order
                let mut tx = self.fetch_transaction(r).clone();
                tx.reverse();
                tx.iter().foreach(|entry, _| {
                    self.execute_entry_inversed(entry.clone());
                });
                r -= 1;
            }
        }

        self.current_revision = rev_id;
        self.set_subscribed(true);
    }

    fn undo(&mut self) {
        if !self.can_undo() {
            return;
        }
        // println!("Undo");

        let cur = self.current_revision;
        self.move_to_revision(cur - 1);
    }

    fn redo(&mut self) {
        if !self.can_redo() {
            return;
        }
        // println!("Redo");

        let cur = self.current_revision;
        self.move_to_revision(cur + 1);
    }

    fn can_redo(&self) -> bool {
        let cur = self.current_revision;
        let last = self.last_revision;

        return cur < last;
    }

    fn can_undo(&self) -> bool {
        let cur = self.current_revision;
        cur > 0
    }

    fn start_journal(&mut self, metadata: Option<Map<String, Value>>) {
        self.set_subscribed(true);
        self.entries.clear();

        if self.count_transactions() == 0 {
            // Sync journal with current graph to start transaction history
            self.set_current_rev(-1);

            self.start_journal_transaction("initial", metadata.clone());

            self.nodes.clone().iter().foreach(|node, _| {
                self.append_command(GraphEvents::AddNode(json!(node)), None);
            });
            self.edges.clone().iter().foreach(|edge, _| {
                self.append_command(GraphEvents::AddEdge(json!(edge)), None);
            });
            self.initializers.clone().iter().foreach(|iip, _| {
                self.append_command(GraphEvents::AddInitial(json!(iip)), None);
            });

            if self.properties.clone().keys().len() > 0 {
                self.append_command(GraphEvents::ChangeProperties(json!(self.properties)), None);
            }

            self.inports.clone().keys().foreach(|name, _| {
                self.append_command(
                    GraphEvents::AddInport(json!({
                        "name": name,
                        "port": self.inports.get(name).unwrap(),
                    })),
                    None,
                );
            });
            self.outports.clone().keys().foreach(|name, _| {
                self.append_command(
                    GraphEvents::AddOutport(json!({
                        "name": name,
                        "port": self.outports.get(name).unwrap(),
                    })),
                    None,
                );
            });

            self.groups.clone().iter().foreach(|group, _| {
                self.append_command(GraphEvents::AddGroup(json!(group)), None);
            });

            self.end_journal_transaction("initial", metadata.clone());
        } else {
            // Persistent store, start with its latest rev
            let last = self.get_last_rev();
            self.set_current_rev(last);
        }

        // Capture graph changes in the background and cache them

        self.connect(
            "add_node",
            |this, data| {
                this.append_command(GraphEvents::new("add_node", data), None);
            },
            false,
        );

        self.connect(
            "remove_node",
            |this, data| {
                this.append_command(GraphEvents::new("remove_node", data), None);
            },
            false,
        );

        self.connect(
            "rename_node",
            |this, data| {
                this.append_command(GraphEvents::new("rename_node", data), None);
            },
            false,
        );

        self.connect(
            "change_node",
            |this, data| {
                this.append_command(GraphEvents::new("change_node", data), None);
            },
            false,
        );

        self.connect(
            "add_edge",
            |this, data| {
                this.append_command(GraphEvents::new("add_edge", data), None);
            },
            false,
        );

        self.connect(
            "remove_edge",
            |this, data| {
                this.append_command(GraphEvents::new("remove_edge", data), None);
            },
            false,
        );

        self.connect(
            "change_edge",
            |this, data| {
                this.append_command(GraphEvents::new("change_edge", data), None);
            },
            false,
        );

        self.connect(
            "add_initial",
            |this, data| {
                this.append_command(GraphEvents::new("add_initial", data), None);
            },
            false,
        );

        self.connect(
            "remove_initial",
            |this, data| {
                this.append_command(GraphEvents::new("remove_initial", data), None);
            },
            false,
        );

        self.connect(
            "change_properties",
            |this, data| {
                this.append_command(GraphEvents::new("change_properties", data), None);
            },
            false,
        );

        self.connect(
            "add_group",
            |this, data| {
                this.append_command(GraphEvents::new("add_group", data), None);
            },
            false,
        );

        self.connect(
            "remove_group",
            |this, data| {
                this.append_command(GraphEvents::new("remove_group", data), None);
            },
            false,
        );

        self.connect(
            "rename_group",
            |this, data| {
                this.append_command(GraphEvents::new("rename_group", data), None);
            },
            false,
        );

        self.connect(
            "change_group",
            |this, data| {
                this.append_command(GraphEvents::new("change_group", data), None);
            },
            false,
        );

        self.connect(
            "add_inport",
            |this, data| {
                this.append_command(GraphEvents::new("add_inport", data), None);
            },
            false,
        );

        self.connect(
            "remove_inport",
            |this, data| {
                this.append_command(GraphEvents::new("remove_inport", data), None);
            },
            false,
        );

        self.connect(
            "rename_inport",
            |this, data| {
                this.append_command(GraphEvents::new("rename_inport", data), None);
            },
            false,
        );

        self.connect(
            "change_inport",
            |this, data| {
                this.append_command(GraphEvents::new("change_inport", data), None);
            },
            false,
        );

        self.connect(
            "add_outport",
            |this, data| {
                this.append_command(GraphEvents::new("add_outport", data), None);
            },
            false,
        );

        self.connect(
            "remove_outport",
            |this, data| {
                this.append_command(GraphEvents::new("remove_outport", data), None);
            },
            false,
        );

        self.connect(
            "rename_outport",
            |this, data| {
                this.append_command(GraphEvents::new("rename_outport", data), None);
            },
            false,
        );

        self.connect(
            "change_outport",
            |this, data| {
                this.append_command(GraphEvents::new("change_outport", data), None);
            },
            false,
        );

        self.connect(
            "start_transaction",
            |this, data| {
                let data = data.as_object().expect("Expected an object");
                this.start_journal_transaction(
                    data["id"].as_str().unwrap(),
                    data["metadata"].as_object().map(|m| m.clone()),
                );
            },
            false,
        );

        self.connect(
            "end_transaction",
            |this, data| {
                let data = data.as_object().expect("Expected an object");
                this.end_journal_transaction(
                    data["id"].as_str().unwrap(),
                    data["metadata"].as_object().map(|m| m.clone()),
                );
            },
            false,
        );
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
    use crate::graph::Graph;
    use crate::journal::Journal;
    use assert_json_diff::assert_json_eq;
    use beady::scenario;
    use serde_json::json;

    #[scenario]
    #[test]
    fn fbp_graph_journal() {
        'given_a_journaled_graph: {
            'when_connected_to_initialized_graph: {
                let mut graph = Graph::new("", false);
                graph.add_node("Foo", "Bar", None);
                graph.add_node("Baz", "Foo", None);
                graph.add_edge("Foo", "out", "Baz", "in", None);
                graph.start_journal(None);
                'then_it_should_have_just_the_initial_transaction: {
                    assert_eq!(graph.last_revision, 0);
                }
            }
            'when_following_basic_graph_changes: {
                let mut graph = Graph::new("", false);
                graph.start_journal(None);
                'then_it_should_create_one_transaction_per_change: {
                    graph.add_node("Foo", "Bar", None);
                    graph.add_node("Baz", "Foo", None);
                    graph.add_edge("Foo", "out", "Baz", "in", None);

                    assert_eq!(graph.last_revision, 3);
                    graph.remove_node("Baz");
                    assert_eq!(graph.last_revision, 4);
                }
            }
            // 'when_printing_to_pretty_string: {}
            'when_jumping_to_revision: {
                let mut graph = Graph::new("", false);
                graph.start_journal(None);
                graph.add_node("Foo", "Bar", None);
                graph.add_node("Baz", "Foo", None);
                graph.add_edge("Foo", "out", "Baz", "in", None);
                graph.add_initial(json!(42), "Foo", "in", None);
                graph.remove_node("Foo");
                'then_it_should_change_the_graph: {
                    graph.move_to_revision(0);
                    assert_eq!(graph.nodes.len(), 0);
                    graph.move_to_revision(2);
                    assert_eq!(graph.nodes.len(), 2);
                    graph.move_to_revision(5);
                    assert_eq!(graph.nodes.len(), 1);
                }
            }
            'when_linear_undo_or_redo: {
                let mut graph = Graph::new("", false);
                graph.start_journal(None);
                graph
                    .add_node("Foo", "Bar", None)
                    .add_node("Baz", "Foo", None)
                    .add_edge("Foo", "out", "Baz", "in", None)
                    .add_initial(json!(42), "Foo", "in", None);
                let graph_before_error = graph.to_json();

                'then_undo_should_restore_previous_revision: {
                    assert_eq!(graph.nodes.len(), 2);
                    graph.remove_node("Foo");
                    assert_eq!(graph.nodes.len(), 1);
                    graph.undo();
                    assert_eq!(graph.nodes.len(), 2);
                    assert_json_eq!(graph.to_json(), graph_before_error);
                    'and_then_redo_should_apply_the_same_change_again: {
                        graph.redo();
                        assert_eq!(graph.nodes.len(), 1);
                        'and_then_undo_should_also_work_multiple_revisions_back: {
                            graph.remove_node("Baz");
                            graph.undo();
                            graph.undo();
                            assert_eq!(graph.nodes.len(), 2);
                            assert_json_eq!(graph.to_json(), graph_before_error);
                        }
                    }
                }
            }
            'when_undo_or_redo_of_metadata_changes: {
                let mut graph = Graph::new("", false);

                graph.start_journal(None);
                graph
                    .add_node("Foo", "Bar", None)
                    .add_node("Baz", "Foo", None)
                    .add_edge("Foo", "out", "Baz", "in", None);

                'then_when_adding_group: {
                    graph.add_group(
                        "all",
                        ["Foo".to_owned(), "Bax".to_owned()].to_vec(),
                        Some(json!({"label": "all nodes"}).as_object().unwrap().clone()),
                    );

                    assert_eq!(graph.groups.len(), 1);
                    assert_eq!(graph.groups[0].name, "all");

                    'and_then_when_undoing_group_add: {
                        graph.undo();

                        assert_eq!(graph.groups.len(), 0);

                        'and_then_when_redoing_group_add: {
                            graph.redo();
                            assert_eq!(graph.groups.len(), 1);

                            'and_then_when_changing_group_metadata_adds_revision: {
                                let r = graph.last_revision;
                                graph.set_group_metadata(
                                    "all",
                                    json!({"label": "ALL NODES!"})
                                        .as_object()
                                        .unwrap()
                                        .to_owned(),
                                );

                                assert_eq!(graph.last_revision, r + 1);

                                'and_then_when_undoing_group_metadata_change: {
                                    graph.undo();
                                    assert_eq!(
                                        graph.groups[0].metadata.as_ref().unwrap().get("label"),
                                        Some(&json!("all nodes"))
                                    );

                                    'and_then_when_redoing_group_metadata_change: {
                                        graph.redo();
                                        assert_eq!(
                                            graph.groups[0].metadata.as_ref().unwrap().get("label"),
                                            Some(&json!("ALL NODES!"))
                                        );

                                        'and_then_when_setting_node_metadata: {
                                            graph.set_node_metadata(
                                                "Foo",
                                                json!({"oneone": json!(11), "2": "two"})
                                                    .as_object()
                                                    .unwrap()
                                                    .clone(),
                                            );

                                            assert_eq!(
                                                graph
                                                    .get_node("Foo")
                                                    .unwrap()
                                                    .metadata
                                                    .as_ref()
                                                    .unwrap()
                                                    .keys()
                                                    .len(),
                                                2
                                            );

                                            'and_then_when_undoing_node_metadata_change: {
                                                graph.undo();

                                                assert_eq!(
                                                    graph
                                                        .get_node("Foo")
                                                        .unwrap()
                                                        .metadata
                                                        .as_ref()
                                                        .unwrap()
                                                        .keys()
                                                        .len(),
                                                    0
                                                );

                                                'and_then_when_redoing_node_metadata_change: {
                                                    graph.redo();
                                                    assert_eq!(
                                                        graph
                                                            .get_node("Foo")
                                                            .unwrap()
                                                            .metadata
                                                            .as_ref()
                                                            .unwrap()
                                                            .get("oneone"),
                                                        Some(&json!(11))
                                                    );
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
