use std::{
    collections::HashMap,
    iter::FromIterator,
    sync::{Arc, Mutex},
    thread, task::Poll, pin::Pin,
};

use crate::{
    ip::{IPType, IP},
    port::BasePort,
    sockets::SocketConnection, errors::async_transform,
};

use foreach::ForEach;
use fp_rust::publisher::Publisher;
use futures::{
    channel::mpsc::{self, Receiver, Sender},
    executor::block_on,
    SinkExt, Future, StreamExt, future::join_all
};



use serde_json::{json, Map, Value};
use zflow::{
    graph::{
        graph::Graph,
        types::{GraphEdge, GraphIIP, GraphNode},
    },
    internal::event_manager::{EventActor, EventListener},
};

use crate::{component::Component, sockets::InternalSocket};

#[derive(Clone)]
pub struct NetworkProcess {
    pub id: String,
    pub component_name: String,
    pub component: Option<Arc<Mutex<Component>>>,
}

#[derive(Clone)]
pub enum NetworkEvent {
    NodeActivate(String),
    NodeDeactivate(String, usize),
    NodeAttributeChange(String, String),
    ProcessReady,
}

#[derive(Clone)]
pub struct NetworkIIP {
    pub data: Value,
    pub socket: Arc<Mutex<InternalSocket>>,
}

pub trait BaseNetwork {
    fn load(
        &mut self,
        component: &str,
        metadata: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<Component>>, std::fmt::Error>;
    /// Add a process to the network. The node will also be registered
    /// with the current graph.
    fn add_node(
        &mut self,
        node: GraphNode,
        options: Option<Map<String, Value>>,
    ) -> Option<NetworkProcess>;
    /// Get node wit process id
    fn get_node(&self, id: &str) -> Option<&NetworkProcess>;
    fn get_node_mut(&mut self, id: &str) -> Option<&mut NetworkProcess>;
    /// Remove a process from the network. The node will also be removed
    /// from the current graph.
    fn remove_node(&mut self, node: GraphNode) -> Result<(), String>;
    fn rename_node(&mut self, old_id: &str, new_id: &str) -> Result<(), String>;
    /// Add a connection to the network. The edge will also be registered
    /// with the current graph.
    fn add_edge(
        &mut self,
        edge: GraphEdge,
        options: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String>;
    fn remove_edge(&mut self, edge: GraphEdge) -> Result<(), std::fmt::Error>;
    fn add_initial(
        &mut self,
        iip: GraphIIP,
        options: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String>;
    fn remove_initial(&mut self, iip: GraphIIP) -> Result<(), std::fmt::Error>;
    fn is_started(&self) -> bool;
    fn is_running(&self) -> bool;
    fn is_stopped(&self) -> bool;
    fn is_debug(&self) -> bool;
    fn set_debug(&mut self, active: bool);
    fn start(&mut self) -> Result<(), std::fmt::Error>;
    fn stop(&mut self) -> Result<(), std::fmt::Error>;
}

#[derive(Clone)]
pub struct NetworkOptions {
    pub base_dir: String,
    pub cache: bool,
    pub discover: bool,
    pub recursive: bool,
}
/// The Network instantiates all the processes connected together.
#[derive(Clone)]
pub struct Network {
    /// The Graph this network is instantiated with
    pub graph: Graph,
    pub options: NetworkOptions,
    /// Processes contains all the instantiated components for this network
    pub processes: HashMap<String, NetworkProcess>,
    /// Connections contains all the socket connections in the network
    pub connections: Vec<Arc<Mutex<InternalSocket>>>,
    /// Initials contains all Initial Information Packets (IIPs)
    pub initials: Vec<NetworkIIP>,
    pub next_initials: Vec<NetworkIIP>,
    /// Container to hold sockets that will be sending default data.
    pub defaults: Vec<Arc<Mutex<InternalSocket>>>,
    pub started: bool,
    pub stopped: bool,
    pub debug: bool,
    pub event_buffer: Vec<NetworkEvent>,
    pub (crate) bus: Arc<Mutex<Publisher<NetworkEvent>>>,
    debounce_ended: bool,
    abort_debounce: bool,
}

impl Network {
    /// Send event
    fn emit(&mut self, name: &'static str, data: Value) {
        self.listeners
            .clone()
            .get_mut(name)
            .iter()
            .foreach(|actions, iter| {
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
    fn disconnect(&mut self, name: &'static str) {
        self.listeners.remove(name);
    }
    /// Check if we have events
    fn has_event(&self, name: &'static str) -> bool {
        self.listeners.contains_key(name)
    }
    /// Attach listener to an event
    fn on(
        &mut self,
        name: &'static str,
        rec: impl FnMut(&mut Self, Value) -> () + 'static,
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

    fn subscribe_node(&mut self, process: NetworkProcess) {
        if let Some(instance) = process.component.clone() {
            let node_id = process.id.clone();
            instance.lock().unwrap().on(
                "activate",
                Box::new(move |_| unsafe {
                    let _ = NETWORK_PIPE
                        .0
                        .send(NetworkEvent::NodeActivate(node_id.clone()));
                }),
            );
            let node_id = process.id.clone();
            instance.lock().unwrap().on(
                "deactivate",
                Box::new(move |load| unsafe {
                    let _ = NETWORK_PIPE
                        .0
                        .send(NetworkEvent::NodeDeactivate(node_id.clone(), load));
                }),
            );
            let node_id = process.id.clone();
            instance.lock().unwrap().on(
                "icon",
                Box::new(move |_| unsafe {
                    let _ = NETWORK_PIPE.0.send(NetworkEvent::NodeAttributeChange(
                        node_id.clone(),
                        "icon".to_owned(),
                    ));
                }),
            );
        }
    }

    fn check_if_finished(&mut self) {}

    fn ensure_node(&mut self, node: String, direction: &str) -> Result<NetworkProcess, String> {
        let process = self.get_node(&node);
        return if let Some(process) = process {
            if process.component.is_none() {
                return Err(format!(
                    "No component defined for {} node {}",
                    direction, node
                ));
            }

            if let Some(component) = process.component.clone() {
                if !component.lock().unwrap().is_ready() {
                    component.lock().unwrap().once(
                        "ready",
                        Box::new(move |_| unsafe {
                            let _ = NETWORK_PIPE.0.send(NetworkEvent::ProcessReady);
                        }),
                    );

                    loop {
                        unsafe {
                            if let Ok(event) = NETWORK_PIPE.1.try_next() {
                                match event {
                                    Some(NetworkEvent::ProcessReady) => {
                                        return Ok(process.clone());
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            Ok(process.clone())
        } else {
            return Err(format!(
                "No process defined for {} node {}",
                direction, node
            ));
        };
    }
    fn subscribe_socket(
        &mut self,
        socket: Arc<Mutex<InternalSocket>>,
        source: Option<NetworkProcess>,
    ) {
    }

    fn connect_port(
        &mut self,
        socket: Arc<Mutex<InternalSocket>>,
        process: NetworkProcess,
        port: String,
        index: Option<usize>,
        inbound: bool,
    ) -> Result<Arc<Mutex<InternalSocket>>, String> {
        if inbound {
            socket.lock().unwrap().to = Some(SocketConnection {
                process: process.clone(),
                port: port.clone(),
                index,
            });
            if process.component.is_none()
                || process
                    .component
                    .clone()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .get_inports()
                    .ports
                    .get(&port.clone())
                    .is_none()
            {
                return Err(format!(
                    "No inport '{}' defined in process {} ({})",
                    port.clone(),
                    process.id,
                    socket.lock().unwrap().get_id()
                ));
            }
            if let Some(inport) = process
                .component
                .clone()
                .unwrap()
                .lock()
                .unwrap()
                .get_inports()
                .ports
                .get_mut(&port.clone())
            {
                (*inport).attach(socket, index);
                return Ok(socket);
            }
        }
        socket.lock().unwrap().from = Some(SocketConnection {
            process: process.clone(),
            port: port.clone(),
            index,
        });
        if process.component.is_none()
            || process
                .component
                .clone()
                .unwrap()
                .lock()
                .unwrap()
                .get_outports()
                .ports
                .get(&port.clone())
                .is_none()
        {
            return Err(format!(
                "No inport '{}' defined in process {} ({})",
                port.clone(),
                process.id,
                socket.lock().unwrap().get_id()
            ));
        }
        if let Some(outport) = process
            .component
            .clone()
            .unwrap()
            .lock()
            .unwrap()
            .get_outports()
            .ports
            .get_mut(&port.clone())
        {
            (*outport).attach(socket.clone(), index);
            return Ok(socket);
        }

        Err("".to_owned())
    }

    async fn send_initials(&mut self) {
        let mut futures:Vec<Pin<Box<dyn Future<Output = Result<(), String>>>>> = Vec::new();
        self.initials.iter().for_each(|init| {
            futures.push(Box::pin(async move {
                if let Ok(socket) = init.clone().socket.clone().try_lock().as_mut() {
                   return socket.post(Some(json!(IP::new(
                    IPType::Data(init.data.clone()),
                    Some(
                        json!({ "initial": json!(true) })
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                ))), true).await;
                }
                return Ok(());
            }));
        });
        join_all(futures).await;
    }
    async fn send_defaults(&mut self) {
        let mut futures:Vec<Pin<Box<dyn Future<Output = Result<(), String>>>>> = Vec::new();
        self.defaults.iter_mut().for_each(|socket| {
            let mut socket = socket.try_lock().expect("expected socket instace");
            if socket.to.as_ref()
                .expect("expect socket.to != None")
                .process
                .component.as_ref()
                .expect("expected component")
                .try_lock()
                .expect("expected component instance")
                .get_inports()
                .ports
                .len()
                != 1
            {
                return;
            }
            futures.push(Box::pin(async move {
                let _ = socket.connect().await?;
                let _ = socket.send(json!("null")).await?;
                let _ = socket.disconnect().await?;
                Ok(())
            }));
        });
        join_all(futures).await;
    }
    fn get_active_processes(&self) -> Vec<String> {
        Vec::new()
    }
    fn set_started(&mut self, started: bool) {}
}

impl BaseNetwork for Network {
    fn load(
        &mut self,
        component: &str,
        metadata: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<dyn Component>>, std::fmt::Error> {
        Err(std::fmt::Error)
    }
    fn add_node(
        &mut self,
        node: GraphNode,
        options: Option<Map<String, Value>>,
    ) -> Option<NetworkProcess> {
        if !self.processes.contains_key(&node.id.clone()) {
            let mut process = NetworkProcess {
                id: node.id.clone(),
                component_name: String::new(),
                component: None,
            };

            // Load the component for the process.
            if let Ok(instance) = self.load(&node.component, options) {
                let instance = instance.clone();
                instance.lock().unwrap().set_id(node.id.clone());
                process.component = Some(instance.clone());
                process.component_name = node.component;
                // Inform the ports of the node name
                for item in instance.lock().unwrap().get_inports().ports.iter_mut() {
                    (*item.1).node = node.id.clone();
                    (*item.1).node_instance = Some(instance.clone());
                }
                for item in instance.lock().unwrap().get_outports().ports.iter_mut() {
                    (*item.1).node = node.id.clone();
                    (*item.1).node_instance = Some(instance.clone());
                }
                if instance.lock().unwrap().is_subgraph() {
                    // self.subscribe_subgraph()
                    // Todo: add support for subgraph
                }
                self.subscribe_node(process.clone());

                // Store and return the process instance
                self.processes.insert(node.id.clone(), process.clone());
                return Some(process);
            }
        }
        self.processes.get(&node.id.clone()).cloned()
    }

    fn get_node(&self, id: &str) -> Option<&NetworkProcess> {
        self.processes.get(id)
    }
    fn get_node_mut(&mut self, id: &str) -> Option<&mut NetworkProcess> {
        self.processes.get_mut(id)
    }
    fn remove_node(&mut self, node: GraphNode) -> Result<(), String> {
        let process = self.get_node(&node.id);
        if process.is_none() {
            return Err(format!("Process {} not found", node.id));
        }

        if let Some(process) = process {
            if process.component.is_none() {
                self.processes.remove(&node.id);
                return Ok(());
            }

            if let Some(component) = process.component.clone() {
                if let Ok(()) = component.lock().unwrap().shutdown() {
                    self.processes.remove(&node.id);
                }
            }
        }

        Ok(())
    }

    fn rename_node(&mut self, old_id: &str, new_id: &str) -> Result<(), String> {
        let process = self.get_node_mut(old_id);
        if process.is_none() {
            return Err(format!("Process {} not found", old_id));
        }
        // Inform the process of its ID
        if let Some(process) = process.cloned().as_mut() {
            (*process).id = new_id.to_owned();
            if let Some(component) = (*process).component.clone() {
                // Inform the ports of the node name
                component
                    .lock()
                    .unwrap()
                    .get_inports_mut()
                    .ports
                    .keys()
                    .foreach(|key, _| {
                        if let Some(port) = component
                            .lock()
                            .unwrap()
                            .get_inports_mut()
                            .ports
                            .get_mut(key)
                        {
                            (*port).node = new_id.to_owned();
                        }
                    });
                component
                    .lock()
                    .unwrap()
                    .get_outports_mut()
                    .ports
                    .keys()
                    .foreach(|key, _| {
                        if let Some(port) = component
                            .lock()
                            .unwrap()
                            .get_outports_mut()
                            .ports
                            .get_mut(key)
                        {
                            (*port).node = new_id.to_owned();
                        }
                    });
            }
            self.processes.insert(new_id.to_owned(), process.clone());
        }
        Ok(())
    }

    fn add_edge(
        &mut self,
        edge: GraphEdge,
        options: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String> {
        let from = self.ensure_node(edge.from.node_id, "outbound")?;
        let socket = InternalSocket::create(edge.metadata);
        let to = self.ensure_node(edge.to.node_id, "inbound")?;
        // Subscribe to events from the socket
        self.subscribe_socket(socket.clone(), Some(from.clone()));
        self.connect_port(socket.clone(), to, edge.to.port, edge.to.index, true)?;
        self.connect_port(socket.clone(), from, edge.from.port, edge.from.index, false)?;
        self.connections.push(socket.clone());
        return Ok(socket);
    }

    fn remove_edge(&mut self, edge: GraphEdge) -> Result<(), std::fmt::Error> {
        self.connections
            .clone()
            .iter()
            .enumerate()
            .foreach(|(i, _connection), _| {
                if let Ok(connection) = _connection.clone().lock() {
                    if let Some(to) = connection.to.clone() {
                        if (edge.to.node_id != to.process.id) || (edge.to.port != to.port) {
                            return;
                        }

                        if let Some(component) = to.process.component.clone() {
                            component
                                .lock()
                                .unwrap()
                                .get_inports_mut()
                                .ports
                                .get_mut(&to.port)
                                .unwrap()
                                .detach(connection.index);
                        }
                    }

                    if !edge.from.node_id.is_empty() {
                        if let Some(from) = connection.from.clone() {
                            if (edge.from.node_id == from.process.id)
                                && (edge.from.port == from.port)
                            {
                                if let Some(component) = from.process.component.clone() {
                                    component
                                        .lock()
                                        .unwrap()
                                        .get_outports_mut()
                                        .ports
                                        .get_mut(&from.port)
                                        .unwrap()
                                        .detach(connection.index);
                                }
                            }
                        }
                    }
                    self.connections.remove(i);
                }
            });
        Ok(())
    }

    fn add_initial(
        &mut self,
        iip: GraphIIP,
        options: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String> {
        let iip_to = iip.to.expect("expect iip.to");
        let to = self.ensure_node(iip_to.node_id, "inbound")?;
        let socket = InternalSocket::create(iip.metadata);

        // Subscribe to events from the socket
        self.subscribe_socket(socket.clone(), None);

        let socket = self
            .connect_port(socket, to, iip_to.port, iip_to.index, true)?
            .clone();

        self.connections.push(socket.clone());

        let init = NetworkIIP {
            socket: socket.clone(),
            data: iip.from.expect("expect iip.from").data,
        };

        self.initials.push(init.clone());
        self.next_initials.push(init.clone());
        if self.is_running() {
            // Network is running now, send initials immediately
            block_on(self.send_initials());
        } else if !self.is_stopped() {
            // Network has finished but hasn't been stopped, set
            // started and set
            self.set_started(true);
            block_on(self.send_initials());
        }
        return Ok(socket);
    }

    fn remove_initial(&mut self, iip: GraphIIP) -> Result<(), std::fmt::Error> {
        self.connections
            .clone()
            .iter()
            .enumerate()
            .foreach(|(i, _connection), _| {
                if let Ok(connection) = _connection.clone().lock() {
                    if let Some(to) = connection.to.clone() {
                        if (iip.clone().to.unwrap().node_id != to.process.id)
                            || (iip.clone().to.unwrap().port != to.port)
                        {
                            return;
                        }

                        if let Some(component) = to.process.component.clone() {
                            component
                                .lock()
                                .unwrap()
                                .get_inports_mut()
                                .ports
                                .get_mut(&to.port)
                                .unwrap()
                                .detach(connection.index);
                        }
                    }
                    self.connections.remove(i);

                    for (i, init) in self.initials.clone().iter().enumerate() {
                        if init.socket.lock().unwrap().get_id() != connection.get_id() {
                            return;
                        }
                        self.initials.remove(i);
                    }
                    for (i, init) in self.next_initials.clone().iter().enumerate() {
                        if init.socket.lock().unwrap().get_id() != connection.get_id() {
                            return;
                        }
                        self.next_initials.remove(i);
                    }
                }
            });

        Ok(())
    }

    fn is_started(&self) -> bool {
        self.started
    }

    fn is_running(&self) -> bool {
        !self.get_active_processes().is_empty()
    }

    fn is_stopped(&self) -> bool {
        self.stopped
    }

    fn is_debug(&self) -> bool {
        self.debug
    }

    fn set_debug(&mut self, active: bool) {
        self.debug = active
    }

    fn start(&mut self) -> Result<(), std::fmt::Error> {
        todo!()
    }

    fn stop(&mut self) -> Result<(), std::fmt::Error> {
        todo!()
    }
}
