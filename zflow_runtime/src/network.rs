use core::panic;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, SystemTime},
};

use array_tool::vec::Shift;
use fp_rust::{handler::Handler, publisher::Publisher};
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use zflow::{
    graph::{
        graph::Graph,
        types::{GraphEdge, GraphIIP, GraphNode},
    },
};

use crate::{
    component::{Component, ComponentEvent, GraphDefinition},
    ip::{IPOptions, IPType, IP},
    loader::ComponentLoader,
    port::BasePort,
    sockets::{InternalSocket, SocketConnection},
};

#[derive(Default, Clone, Debug)]
pub struct NetworkProcess{
    pub id: String,
    pub component_name: String,
    pub component: Option<Arc<Mutex<Component>>>,
}

#[derive(Clone, Debug)]
pub struct NetworkIIP {
    pub socket: Arc<Mutex<InternalSocket>>,
    pub data: Value,
}

#[derive(Clone)]
pub enum NetworkEvent {
    OpenBracket(Value),
    CloseBracket(Value),
    Start(Value),
    End(Value),
    IP(Value),
    Error(Value),
}

pub struct NetworkOptions {
    pub subscribe_graph: bool,
    pub debug: bool,
    pub delay: bool,
}

/// ## The ZFlow Network
///
/// ZFlow networks consist of processes connected to each other
/// via sockets attached from outports to inports.
///
/// The role of the network coordinator is to take a graph and
/// instantiate all the necessary processes from the designated
/// components, attach sockets between them, and handle the sending
/// of Initial Information Packets.
pub trait BaseNetwork {
    fn get_publisher(&self) -> Arc<Mutex<Publisher<NetworkEvent>>>;
    fn is_started(&self) -> bool;
    fn is_stopped(&self) -> bool;
    fn is_running(&self) -> bool {
        self.get_active_processes().len() > 0
    }
    fn set_started(&mut self, started: bool);

    fn get_loader(&mut self) -> &mut Option<ComponentLoader>;

    fn get_connections(&self) -> Vec<Arc<Mutex<InternalSocket>>>;
    fn get_connections_mut(&mut self) -> &mut Vec<Arc<Mutex<InternalSocket>>>;
    fn get_defaults(&self) -> Vec<Arc<Mutex<InternalSocket>>>;
    fn get_defaults_mut(&mut self) -> &mut Vec<Arc<Mutex<InternalSocket>>>;

    fn get_initials(&self) -> Vec<NetworkIIP>;
    fn get_initials_mut(&mut self) -> &mut Vec<NetworkIIP>;

    fn get_next_initials(&self) -> Vec<NetworkIIP>;
    fn get_next_initials_mut(&mut self) -> &mut Vec<NetworkIIP>;

    /// Processes contains all the instantiated components for this network
    fn get_processes(&self) -> HashMap<String, NetworkProcess>;
    fn get_processes_mut(&mut self) -> &mut HashMap<String, NetworkProcess>;
    fn get_active_processes(&self) -> Vec<String> {
        let mut active = vec![];
        if !self.is_started() {
            return active;
        }

        self.get_processes().iter().for_each(|(name, process)| {
            if let Some(component) = process.component.clone() {
                if let Ok(component) = &component.try_lock() {
                    if component.get_load() > 0
                        || component
                            .get_handler_thread()
                            .try_lock()
                            .unwrap()
                            .is_alive()
                    {
                        active.push(name.to_string());
                    }
                }
            }
        });

        active
    }
    fn get_startup_time(&self) -> Option<SystemTime>;
    /// The uptime of the network is the current time minus the start-up
    /// time, in seconds.
    fn uptime(&self) -> u128 {
        if let Some(earlier) = self.get_startup_time() {
            return SystemTime::now()
                .duration_since(earlier)
                .unwrap()
                .as_millis();
        }
        0
    }

    /// ## Loading components
    fn load(
        &mut self,
        component: &str,
        metadata: Value,
    ) -> Result<Arc<Mutex<Component>>, String>;

    // fn add_defaults(&mut self, node: GraphNode) -> Result<(), String>;

    /// Remove node from network
    fn remove_node(&mut self, node: GraphNode) -> Result<(), String>;
    /// Rename a node in the  network
    fn rename_node(&mut self, old_id: &str, new_id: &str) -> Result<(), String>;

    /// Get process by its ID.
    fn get_node(&self, id: &str) -> Option<NetworkProcess>;

    fn ensure_node(&self, node: &str, direction: &str) -> Result<NetworkProcess, String>;

    // fn connect(&mut self) -> Result<(), String>;

    /// Remove edge
    fn remove_edge(&mut self, node: GraphEdge) -> Result<(), String>;
    /// Remove initial packets
    fn remove_initial(&mut self, initializer: GraphIIP) -> Result<(), String>;
    fn send_initials(&mut self) -> Result<(), String>;

    fn send_defaults(&mut self) -> Result<(), String>;

    /// Start the network
    fn start(&mut self) -> Result<(), String>;
    /// Stop the network
    fn stop(&mut self) -> Result<(), String>;

    fn get_debug(&self) -> bool;
    fn set_debug(&mut self, active: bool);
    fn get_graph(&self) -> Arc<Mutex<Graph>>;
    fn cancel_debounce(&mut self, abort: bool);
    fn is_abort_debounce(&self) -> bool;
    fn set_debounce_ended(&mut self, end: bool);
    fn get_debounce_ended(&self) -> bool;
    fn buffered_emit(&mut self, event: NetworkEvent);
}

/// ## The ZFlow Network
///
/// ZFlow networks consist of processes connected to each other
/// via sockets attached from outports to inports.
///
/// The role of the network coordinator is to take a graph and
/// instantiate all the necessary processes from the designated
/// components, attach sockets between them, and handle the sending
/// of Initial Information Packets.
pub struct Network {
    pub options: NetworkOptions,
    /// Processes contains all the instantiated components for this network
    pub processes: HashMap<String, NetworkProcess>,
    /// Initials contains all Initial Information Packets (IIPs)
    pub initials: Vec<NetworkIIP>,
    pub next_initials: Vec<NetworkIIP>,
    /// Connections contains all the socket connections in the network
    pub connections: Vec<Arc<Mutex<InternalSocket>>>,
    /// Container to hold sockets that will be sending default data.
    pub defaults: Vec<Arc<Mutex<InternalSocket>>>,
    /// The Graph this network is instantiated with
    pub graph: Arc<Mutex<Graph>>,
    pub started: bool,
    pub stopped: bool,
    pub debug: bool,
    pub event_buffer: Vec<NetworkEvent>,
    pub loader: Option<ComponentLoader>,
    pub publisher: Arc<Mutex<Publisher<NetworkEvent>>>,
    debounce_end: bool,
    abort_debounce: bool,
    startup_time: Option<SystemTime>,
}

unsafe impl Send for Network {}
unsafe impl Sync for Network {}

impl Network {
    fn new(graph: Graph, options: NetworkOptions) -> Self {
        Self {
            options,
            processes: HashMap::new(),
            initials: Vec::new(),
            next_initials: Vec::new(),
            connections: Vec::new(),
            defaults: Vec::new(),
            graph: Arc::new(Mutex::new(graph)),
            started: false,
            stopped: false,
            debug: false,
            event_buffer: Vec::new(),
            loader: None,
            publisher: Arc::new(Mutex::new(Publisher::new())),
            startup_time: None,
            debounce_end: false,
            abort_debounce: false,
        }
    }

    fn check_if_finished<Net>(network: Arc<Mutex<Net>>)
    where
        Net: BaseNetwork + Send + Sync + 'static,
    {
        if let Ok(this) = network.clone().try_lock().as_mut() {
            if this.is_running() {
                return;
            }
            this.cancel_debounce(false);
        }

        thread::spawn(move || {
            if let Ok(this) = network.clone().try_lock().as_mut() {
                if !this.get_debounce_ended() {
                    thread::sleep(Duration::from_millis(50));
                    loop {
                        if this.is_abort_debounce() {
                            break;
                        }
                        if this.is_running() {
                            break;
                        }
                        this.set_started(false);
                    }
                    this.set_debounce_ended(true);
                }
            }
        });
    }

    fn subscribe_subgraph<Net>(
        network: Arc<Mutex<Net>>,
        node: NetworkProcess,
    ) -> Result<(), String>
    where
        Net: BaseNetwork + Send + Sync + 'static,
    {
        if node.component.is_none() {
            return Ok(());
        }
        if let Ok(component) = node.clone().component.unwrap().clone().try_lock().as_mut() {
            if !component.is_ready() {
                component.on(move |event| match event.as_ref() {
                    ComponentEvent::Ready => {
                        Network::subscribe_subgraph(network.clone(), node.clone())
                            .expect("expected to subscribe to subgraph");
                    }
                    _ => {}
                });
                return Ok(());
            }

            if component.get_network().is_none() {
                return Ok(());
            }

            if let Ok(this) = network.clone().try_lock().as_mut() {
                if let Ok(_network) = component.get_network().unwrap().clone().try_lock().as_mut() {
                    _network.set_debug(this.get_debug());
                    // Todo: async_delivery?
                    // Todo: enable flow_trace?
                }
            }

            if let Ok(bus) = network
                .clone()
                .try_lock()
                .as_mut()
                .unwrap()
                .get_publisher()
                .try_lock()
                .as_mut()
            {
                bus.subscribe_fn(move |event| {
                    if let Ok(this) = network.clone().try_lock().as_mut() {
                        match event.as_ref() {
                            NetworkEvent::IP(data) => {
                                if let Some(data) = data.as_object() {
                                    let mut _data = data.clone();
                                    if _data.contains_key("subgraph") {
                                        if let Some(__data) = _data.get_mut("subgraph") {
                                            if let Some(_data) = __data.as_array_mut() {
                                                if _data.is_empty() {
                                                    _data.unshift(json!(node.clone().id));
                                                }
                                            } else {
                                                _data.insert(
                                                    "subgraph".to_owned(),
                                                    json!([json!(node.clone().id)]),
                                                );
                                            }
                                        }
                                    }
                                    this.buffered_emit(NetworkEvent::IP(json!(_data)));
                                }
                            }
                            NetworkEvent::Error(err) => {
                                this.buffered_emit(NetworkEvent::Error(err.clone()));
                            },
                            _ => {}
                        }
                    }
                });
            }
        }
        Ok(())
    }

    fn subscribe_node<Net>(network: Arc<Mutex<Net>>, node: NetworkProcess) -> Result<(), String>
    where
        Net: BaseNetwork + Send + Sync + 'static,
    {
        if node.component.is_none() {
            return Ok(());
        }
        if let Ok(component) = node.component.unwrap().clone().try_lock().as_mut() {
            if !component.is_ready() {
                component.on(move |event| {
                    let binding = network.clone();
                    let mut binding = binding.try_lock();
                    let _network = binding.as_mut().unwrap();

                    match event.as_ref() {
                        ComponentEvent::Activate(_) => {
                            if _network.get_debounce_ended() {
                                _network.cancel_debounce(true);
                            }
                        }
                        ComponentEvent::Deactivate(load) => {
                            if *load > 0 {
                                return;
                            }
                            Network::check_if_finished(network.clone());
                        }
                        // Todo: detect changes to other component properties like icon change
                        _ => {}
                    }
                });
            }
        }

        Ok(())
    }
    /// Subscribe to events from all connected sockets and re-emit them
    fn subscribe_socket<Net>(
        network: Arc<Mutex<Net>>,
        socket: Arc<Mutex<InternalSocket>>,
        _: Option<NetworkProcess>,
    ) -> Result<(), String>
    where
        Net: BaseNetwork + Send + Sync + 'static,
    {
        if let Ok(_socket) = socket.clone().try_lock().as_mut() {
            let id = _socket.get_id();
            let metadata = _socket.metadata.clone();
            _socket.on(move |event| {
                let binding = network.clone();
                let mut binding = binding.try_lock();
                let _network = binding.as_mut().unwrap();
                match event.as_ref() {
                    crate::sockets::SocketEvent::IP(ip, _) => {
                        _network.buffered_emit(NetworkEvent::IP(json!({
                        "id": id,
                        "data": ip.datatype,
                        "metadata": metadata
                        })));
                    }
                    crate::sockets::SocketEvent::Error(err, _) => {
                        // panic!("{}", err);
                        log::error!("InternalSocket::Error : {}", err);
                    }
                    _ => {}
                }
            });
        }
        Ok(())
    }

    pub fn start_components(&mut self) -> Result<(), String> {
        if self.processes.is_empty() {
            return Ok(());
        }
        // Perform any startup routines necessary for every component.
        self.processes
            .clone()
            .par_iter_mut()
            .for_each(|(_, process)| {
                if let Some(component) = process.component.clone() {
                    Component::start(component.clone()).expect("expected component to start");
                }
            });

        Ok(())
    }

    pub fn create(
        graph: Graph,
        options: NetworkOptions,
    ) -> Arc<Mutex<dyn BaseNetwork + Send + Sync + 'static>> {
        Arc::new(Mutex::new(Network::new(graph, options)))
    }

    pub fn connect<Net>(network: Arc<Mutex<Net>>) -> Result<(), String>
    where
        Net: BaseNetwork + Send + Sync + 'static,
    {
        let binding = network.clone();
        let _network = binding.try_lock().unwrap();
        if let Ok(graph) = _network.get_graph().try_lock() {
            graph.nodes.clone().iter().for_each(|node| {
                Network::add_node::<Net>(
                    network.clone(),
                    node.clone(),
                    Some(HashMap::from_iter([(String::from("initial"), json!(true))])),
                )
                .expect(format!("Could not add node {} to network", node.id).as_str());
            });
            graph.edges.clone().iter().for_each(|edge| {
                Network::add_edge::<Net>(
                    network.clone(),
                    edge.clone(),
                    Some(HashMap::from_iter([(String::from("initial"), json!(true))])),
                )
                .expect(format!("Could not add edge {:?} to network", edge).as_str());
            });
            graph.initializers.clone().iter().for_each(|iip| {
                Network::add_initial::<Net>(
                    network.clone(),
                    iip.clone(),
                    Some(HashMap::from_iter([(String::from("initial"), json!(true))])),
                )
                .expect(format!("Could not add IIP {:?} to network", iip).as_str());
            });
            graph.nodes.clone().iter().for_each(|node| {
                Network::add_defaults::<Net>(network.clone(), node.clone())
                    .expect(format!("Could not add node {} to network", node.id).as_str());
            });
        }

        Ok(())
    }

    /// ## Add a process to the network
    ///
    /// Processes can be added to a network at either start-up time
    /// or later. The processes are added with a node definition object
    /// that includes the following properties:
    ///
    /// * `id`: Identifier of the process in the network. Typically a string
    /// * `component`: A ZFlow component instance object
    pub fn add_node<Net>(
        network: Arc<Mutex<Net>>,
        node: GraphNode,
        options: Option<HashMap<String, Value>>,
    ) -> Result<NetworkProcess, String>
    where
        Net: BaseNetwork + Send + Sync + 'static,
    {
        let binding = network.clone();
        let mut binding = binding.try_lock();
        let _network = binding.as_mut().unwrap();
        // Processes are treated as singletons by their identifier. If
        // we already have a process with the given ID, return that.
        if _network.get_processes().contains_key(&node.id) {
            return Ok(_network
                .get_processes()
                .get(&node.id.clone())
                .unwrap()
                .clone());
        }
        let mut process = NetworkProcess {
            id: node.id.clone(),
            ..NetworkProcess::default()
        };
        if node.component.is_empty() {
            // No component defined, just register the process but don't start.
            _network.get_processes().insert(node.id.clone(), process);
            return Ok(_network
                .get_processes()
                .get(&node.id.clone())
                .unwrap()
                .clone());
        }
        // Load the component for the process.
        let _instance = _network.load(&node.component, json!(options.clone()))?;
        if let Ok(instance) = _instance.clone().try_lock().as_mut() {
            instance.set_node_id(node.id.clone());
            process.component = Some(_instance.clone());
            process.component_name = node.clone().component;

            // Inform the ports of the node name
            instance
                .get_inports_mut()
                .ports
                .iter_mut()
                .for_each(|(name, port)| {
                    port.node = node.id.clone();
                    port.node_instance = Some(_instance.clone());
                    port.name = name.clone();
                });

            instance
                .get_outports_mut()
                .ports
                .iter_mut()
                .for_each(|(name, port)| {
                    port.node = node.id.clone();
                    port.node_instance = Some(_instance.clone());
                    port.name = name.clone();
                });

            if instance.is_subgraph() {
                let _ = Network::subscribe_subgraph(network.clone(), process.clone())
                    .expect("expected to subscribe to subgraph");
            }

            let _ = Network::subscribe_node(network.clone(), process.clone())
                .expect("expected to subscribe to node");

            // Store and return the process instance
            _network
                .get_processes()
                .insert(node.id.clone(), process.clone());
            if let Ok(graph) = _network.get_graph().clone().try_lock().as_mut() {
                let op = if let Some(op) = options {
                    json!(op).as_object().cloned()
                } else {
                    None
                };
                graph.add_node(&node.id, &node.component, op);
            }
            return Ok(process);
        }

        Err(format!("Error while adding node {} to network", node.id))
    }

    // Add edge to network
    pub fn add_edge<Net>(
        network: Arc<Mutex<Net>>,
        edge: GraphEdge,
        options: Option<HashMap<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String>
    where
        Net: BaseNetwork + Send + Sync + 'static,
    {
        let binding = network.clone();
        let mut binding = binding.try_lock();
        let _network = binding.as_mut().unwrap();
        let from = _network.ensure_node(&edge.from.node_id, "outbound")?;
        let socket = InternalSocket::create(edge.metadata); // Todo: configure socket to support async and debug
        let to = _network.ensure_node(&edge.to.node_id, "inbound")?;
        // Subscribe to events from the socket
        Network::subscribe_socket(network.clone(), socket.clone(), Some(from.clone()))?;
        connect_port(socket.clone(), to, &edge.to.port, edge.to.index, true)?;
        connect_port(
            socket.clone(),
            from,
            &edge.from.port,
            edge.from.index,
            false,
        )?;
        _network.get_connections_mut().push(socket.clone());
        if let Ok(graph) = _network.get_graph().clone().try_lock().as_mut() {
            let op = if let Some(op) = options {
                json!(op).as_object().cloned()
            } else {
                None
            };
            graph.add_edge(&edge.to.node_id, &edge.to.port, &edge.from.node_id, &edge.from.port, op);
        }
        Ok(socket.clone())
    }
    /// Add initial packets
    pub fn add_initial<Net>(
        network: Arc<Mutex<Net>>,
        initializer: GraphIIP,
        options: Option<HashMap<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String>
    where
        Net: BaseNetwork + Send + Sync + 'static,
    {
        let binding = network.clone();
        let mut binding = binding.try_lock();
        let _network = binding.as_mut().unwrap();
        if let Some(leaf) = initializer.to {
            let to = _network.ensure_node(&leaf.node_id, "inbound")?;
            // Todo: configure socket to support async and debug
            let socket = InternalSocket::create(initializer.metadata);
            // Subscribe to events from the socket
            Network::subscribe_socket(network.clone(), socket.clone(), None)?;
            let socket = connect_port(socket.clone(), to, &leaf.port, leaf.index, true)?;
            _network.get_connections_mut().push(socket.clone());
            let init = NetworkIIP {
                socket: socket.clone(),
                data: initializer.from.clone().unwrap().data,
            };
            _network.get_initials_mut().push(init.clone());
            _network.get_next_initials_mut().push(init.clone());

            if _network.is_running() {
                // Network is running now, send initials immediately
                _network.send_initials()?;
            } else if !_network.is_stopped() {
                // Network has finished but hasn't been stopped, set
                // started and set
                _network.set_started(true);
                _network.send_initials()?;
            }
            if let Ok(graph) = _network.get_graph().clone().try_lock().as_mut() {
                let op = if let Some(op) = options {
                    json!(op).as_object().cloned()
                } else {
                    None
                };
                graph.add_initial_index(initializer.from.unwrap().data, &leaf.node_id, &leaf.port, leaf.index, op);
            }
            return Ok(socket.clone());
        }
        Err("".to_string())
    }

    pub fn add_defaults<Net>(network: Arc<Mutex<Net>>, node: GraphNode) -> Result<(), String>
    where
        Net: BaseNetwork + Send + Sync + 'static,
    {
        let process = network
            .clone()
            .try_lock()
            .as_mut()
            .unwrap()
            .ensure_node(&node.id, "inbound")?;

        if let Some(component) = process.clone().component {
            if let Ok(component) = component.clone().try_lock().as_mut() {
                let _: Vec<()> = component
                    .get_inports_mut()
                    .ports
                    .par_iter()
                    .map(move |(key, port)| {
                        // Attach a socket to any defaulted inPorts as long as they aren't already attached.
                        if !port.has_default() || port.is_attached(None) {
                            return;
                        }
                        let socket = InternalSocket::create(None);
                        // Subscribe to events from the socket
                        let _ =
                            Network::subscribe_socket::<Net>(network.clone(), socket.clone(), None);
                        if let Ok(this) = network.clone().try_lock().as_mut() {
                            if connect_port(socket.clone(), process.clone(), key, None, true)
                                .is_ok()
                            {
                                this.get_connections_mut().push(socket.clone());
                                this.get_defaults_mut().push(socket.clone());
                            }
                        }
                    })
                    .collect();
            }
        }

        Ok(())
    }
}

impl BaseNetwork for Network
{
    fn get_loader(&mut self) -> &mut Option<ComponentLoader> {
        &mut self.loader
    }

    fn is_running(&self) -> bool {
        self.get_active_processes().len() > 0
    }

    fn load(
        &mut self,
        component: &str,
        metadata: Value,
    ) -> Result<Arc<Mutex<Component>>, String> {
        if let Some(loader) = self.loader.as_mut() {
            return loader.load(component, metadata);
        }
        Err("failed to load component".to_string())
    }

    fn is_started(&self) -> bool {
        self.started
    }

    fn is_stopped(&self) -> bool {
        self.stopped
    }

    fn set_started(&mut self, started: bool) {
        if self.started == started {
            return;
        }

        if !self.started {
            // Ending the execution
            self.started = false;

            // buffered emit
            let now: u128 = SystemTime::now().elapsed().unwrap().as_millis();
            let started = self.startup_time.unwrap().elapsed().unwrap().as_millis();
            self.buffered_emit(NetworkEvent::End(json!({
                "start": started,
                "end": now,
                "uptime": self.uptime()
            })));
            return;
        }
        // Starting the execution
        if self.startup_time.is_none() {
            self.startup_time = Some(SystemTime::now());
        }
        self.started = true;
        self.stopped = false;
        // buffered emit
        let started = self.startup_time.unwrap().elapsed().unwrap().as_millis();
        self.buffered_emit(NetworkEvent::Start(json!({ "start": started })));
    }

    fn get_processes(&self) -> HashMap<String, NetworkProcess> {
        self.processes.clone()
    }

    fn get_startup_time(&self) -> Option<SystemTime> {
        self.startup_time
    }

    fn remove_node(&mut self, node: GraphNode) -> Result<(), String> {
        if let Some(process) = self.get_node(&node.id) {
            if process.component.is_none() {
                self.processes.remove(&node.id);
                return Ok(());
            }
            Component::shutdown(process.component.unwrap().clone())?;
            self.processes.remove(&node.id);
            if let Ok(graph) = self.get_graph().try_lock().as_mut() {
                graph.remove_node(&node.id);
            }
            return Ok(());
        }

        Err(format!("Process {} not found", node.id))
    }

    fn rename_node(&mut self, old_id: &str, new_id: &str) -> Result<(), String> {
        if let Some(mut process) = self.get_node(old_id) {
            // Inform the process of its ID
            process.id = new_id.to_owned();
            if let Some(component) = process.component.clone() {
                if let Ok(instance) = component.try_lock().as_mut() {
                    // Inform the ports of the node name
                    instance
                        .get_inports_mut()
                        .ports
                        .iter_mut()
                        .for_each(|(_, port)| {
                            port.node = new_id.to_owned();
                        });

                    instance
                        .get_outports_mut()
                        .ports
                        .iter_mut()
                        .for_each(|(_, port)| {
                            port.node = new_id.to_owned();
                        });

                    self.processes.insert(new_id.to_owned(), process);
                    self.processes.remove(old_id);
                    if let Ok(graph) = self.get_graph().try_lock().as_mut() {
                        graph.rename_node(old_id, new_id);
                    }
                    return Ok(());
                }
            }
        }
        Err(format!("Process {} not found", old_id))
    }

    fn get_node(&self, id: &str) -> Option<NetworkProcess> {
        self.processes.get(id).cloned()
    }

    fn ensure_node(&self, node: &str, direction: &str) -> Result<NetworkProcess, String> {
        if let Some(process) = self.get_node(node) {
            if process.component.is_none() {
                return Err(format!(
                    "No component defined for {} node {}",
                    direction, node
                ));
            }
            if let Ok(component) = process
                .component
                .as_ref()
                .unwrap()
                .clone()
                .try_lock()
                .as_mut()
            {
                if !component.is_ready() {
                    let mut ready = false;
                    component.on(move |event| match event.as_ref() {
                        ComponentEvent::Ready => {
                            ready = true;
                        }
                        _ => {}
                    });
                    while !ready {
                        // block until ready
                    }
                    return Ok(process);
                }
            }
            return Ok(process);
        }

        Err(format!(
            "No process defined for {} node {}",
            direction, node
        ))
    }

    fn remove_edge(&mut self, edge: GraphEdge) -> Result<(), String> {
        self.connections.iter_mut().for_each(|connection| {
            if let Ok(connection) = connection.clone().try_lock().as_mut() {
                if edge.to.node_id
                    != connection
                        .to
                        .clone()
                        .expect("expected connection to have 'to' definition")
                        .process
                        .id
                    || edge.to.port
                        != connection
                            .to
                            .clone()
                            .expect("expected connection to have 'to'")
                            .port
                {
                    return;
                }
                // detach port
                connection
                    .to
                    .as_mut()
                    .expect("expected connection to have 'to' definition")
                    .process
                    .component
                    .as_mut()
                    .expect("expected process to have component")
                    .clone()
                    .try_lock()
                    .unwrap()
                    .get_inports_mut()
                    .ports
                    .get_mut(
                        connection
                            .to
                            .clone()
                            .expect("expected connection to have 'to'")
                            .port
                            .as_str(),
                    )
                    .unwrap()
                    .detach(connection.index);

                if !edge.from.node_id.is_empty() {
                    if connection.from.is_some()
                        && edge.from.node_id == connection.from.clone().unwrap().process.id
                        || edge.from.port == connection.from.clone().unwrap().port
                    {
                        connection
                            .from
                            .as_mut()
                            .unwrap()
                            .process
                            .component
                            .as_mut()
                            .unwrap()
                            .clone()
                            .try_lock()
                            .as_mut()
                            .unwrap()
                            .get_outports_mut()
                            .ports
                            .get_mut(&connection.from.clone().unwrap().port)
                            .unwrap()
                            .detach(connection.index);
                    }
                }
            }
        });
        if let Ok(graph) = self.get_graph().try_lock().as_mut() {
            graph.remove_edge(&edge.from.node_id, &edge.from.port, Some(&edge.to.node_id), Some(&edge.to.port));
        }
        Ok(())
    }

    fn remove_initial(&mut self, initializer: GraphIIP) -> Result<(), String> {
        self.connections.clone().iter_mut().for_each(|_connection| {
            if let Ok(connection) = _connection.clone().try_lock().as_mut() {
                if initializer
                    .clone()
                    .to
                    .expect("expected initiliazer to have 'to' definition")
                    .node_id
                    == connection
                        .to
                        .clone()
                        .expect("expected connection to have 'to' definition")
                        .process
                        .id
                    || initializer
                        .to
                        .clone()
                        .expect("expected initiliazer to have 'to' definition")
                        .port
                        == connection
                            .to
                            .clone()
                            .expect("expected connection to have 'to' definition")
                            .port
                {
                    return;
                }

                // detach port
                connection
                    .to
                    .as_mut()
                    .expect("expected connection to have 'to' definition")
                    .process
                    .component
                    .as_mut()
                    .expect("expected process to have component")
                    .clone()
                    .try_lock()
                    .unwrap()
                    .get_inports_mut()
                    .ports
                    .get_mut(
                        connection
                            .to
                            .clone()
                            .expect("expected connection to have 'to'")
                            .port
                            .as_str(),
                    )
                    .unwrap()
                    .detach(connection.index);

                let pos = self
                    .connections
                    .iter()
                    .position(|c| Arc::ptr_eq(c, &_connection));
                if let Some(index) = pos {
                    self.connections.remove(index);
                }

                for (i, init) in self.initials.clone().iter().enumerate() {
                    if Arc::ptr_eq(&init.socket, &_connection) {
                        self.initials.remove(i);
                    }
                }
                for (i, init) in self.next_initials.clone().iter().enumerate() {
                    if Arc::ptr_eq(&init.socket, &_connection) {
                        self.next_initials.remove(i);
                    }
                }
            }
        });
        if let Ok(graph) = self.get_graph().try_lock().as_mut() {
            graph.remove_initial(&initializer.to.clone().unwrap().node_id, &initializer.to.clone().unwrap().port);
        }
        Ok(())
    }

    fn send_initials(&mut self) -> Result<(), String> {
        let _ = self.initials.clone().par_iter().map(|initial| {
            if let Ok(socket) = initial.socket.clone().try_lock().as_mut() {
                socket
                    .post(
                        Some(IP::new(
                            IPType::Data(json!("data")),
                            IPOptions {
                                initial: true,
                                ..IPOptions::default()
                            },
                        )),
                        true,
                    )
                    .expect("expected to post initials");
            }
        });
        self.initials.clear();
        Ok(())
    }

    fn send_defaults(&mut self) -> Result<(), String> {
        let _: Vec<()> = self
            .defaults
            .par_iter()
            .map(|socket| {
                // Don't send defaults if more than one socket is present on the port.
                // This case should only happen when a subgraph is created as a component
                // as its network is instantiated and its inputs are serialized before
                // a socket is attached from the "parent" graph.
                if let Ok(socket) = socket.clone().try_lock().as_mut() {
                    if socket
                        .to
                        .clone()
                        .unwrap()
                        .process
                        .component
                        .clone()
                        .unwrap()
                        .try_lock()
                        .unwrap()
                        .get_inports_mut()
                        .ports[&socket.to.clone().unwrap().port]
                        .sockets
                        .len()
                        != 1
                    {
                        return;
                    }
                    let _ = socket.connect();
                    let _ = socket.send(None);
                    let _ = socket.disconnect();
                }
            })
            .collect();
        Ok(())
    }

    fn start(&mut self) -> Result<(), String> {
        if self.debounce_end {
            self.abort_debounce = true;
        }
        if self.started {
            self.stop()?;
            self.start()?;
            return Ok(());
        }
        self.initials = self.next_initials.clone();
        self.event_buffer.clear();
        self.start_components()?;
        self.send_initials()?;
        self.send_defaults()?;
        self.set_started(true);

        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        if self.debounce_end {
            self.abort_debounce = true;
        }

        if !self.started {
            self.stopped = true;
            return Ok(());
        }

        // Disconnect all connections
        self.connections
            .clone()
            .par_iter_mut()
            .for_each(|connection| {
                if let Ok(connection) = connection.clone().try_lock().as_mut() {
                    if !connection.is_connected() {
                        return;
                    }
                    let _ = connection.disconnect();
                }
            });

        if self.processes.is_empty() {
            // No processes to stop
            self.set_started(false);
            self.stopped = true;
            return Ok(());
        }

        // Emit stop event when all processes are stopped
        let _: Vec<_> = self
            .processes
            .clone()
            .par_iter_mut()
            .map(|(_, process)| {
                if process.component.is_none() {
                    return Ok(());
                }
                return Component::shutdown(process.component.clone().unwrap());
            })
            .collect();

        self.set_started(false);
        self.stopped = true;
        Ok(())
    }

    fn get_debug(&self) -> bool {
        self.debug
    }

    fn set_debug(&mut self, active: bool) {
        if active == self.debug {
            return;
        }

        self.debug = active;

        self.connections.iter().for_each(|socket| {
            if let Ok(socket) = socket.clone().try_lock().as_mut() {
                socket.set_debug(active);
            }
        });
        self.processes.iter_mut().for_each(|(_, process)| {
            if process.component.is_none() {
                return;
            }
            if let Some(instance) = process.component.clone() {
                if let Ok(instance) = instance.clone().try_lock().as_mut() {
                    if instance.is_subgraph() {
                        if let Some(network) = instance.get_network() {
                            if let Ok(network) = network.clone().try_lock().as_mut() {
                                network.set_debug(active);
                            }
                        }
                    }
                }
            }
        });
    }

    fn get_publisher(&self) -> Arc<Mutex<Publisher<NetworkEvent>>> {
        self.publisher.clone()
    }

    fn get_graph(&self) -> Arc<Mutex<Graph>> {
        self.graph.clone()
    }

    fn get_connections(&self) -> Vec<Arc<Mutex<InternalSocket>>> {
        self.connections.clone()
    }

    fn get_connections_mut(&mut self) -> &mut Vec<Arc<Mutex<InternalSocket>>> {
        &mut self.connections
    }

    fn get_defaults(&self) -> Vec<Arc<Mutex<InternalSocket>>> {
        self.defaults.clone()
    }

    fn get_defaults_mut(&mut self) -> &mut Vec<Arc<Mutex<InternalSocket>>> {
        &mut self.defaults
    }

    fn cancel_debounce(&mut self, abort: bool) {
        self.abort_debounce = abort;
    }

    fn is_abort_debounce(&self) -> bool {
        self.abort_debounce
    }

    fn set_debounce_ended(&mut self, end: bool) {
        self.debounce_end = end;
    }

    fn get_debounce_ended(&self) -> bool {
        self.debounce_end
    }

    fn get_processes_mut(&mut self) -> &mut HashMap<String, NetworkProcess> {
        &mut self.processes
    }

    fn get_initials(&self) -> Vec<NetworkIIP> {
        self.initials.clone()
    }

    fn get_initials_mut(&mut self) -> &mut Vec<NetworkIIP> {
        &mut self.initials
    }

    fn get_next_initials(&self) -> Vec<NetworkIIP> {
        self.next_initials.clone()
    }

    fn get_next_initials_mut(&mut self) -> &mut Vec<NetworkIIP> {
        &mut self.next_initials
    }

    fn buffered_emit(&mut self, event: NetworkEvent) {
        // Add the event to Flowtrace immediately
        // Todo: flow tracer
        match event {
            NetworkEvent::End(_) | NetworkEvent::Error(_) => {
                self.publisher.clone().try_lock().unwrap().publish(event);
                return;
            },
            _ =>{}
        }
        if !self.is_started() {
            match event {
                NetworkEvent::End(_) =>{}
                _ =>{
                    self.event_buffer.push(event);
                    return;
                }
            }
        }
        self.publisher.clone().try_lock().unwrap().publish(event.clone());
        match event {
            NetworkEvent::Start(_) =>{
                // Once network has started we can send the IP-related events
                self.event_buffer.clone().par_iter().for_each(|ev|{
                    self.publisher.clone().try_lock().unwrap().publish(ev.clone());
                });
                self.event_buffer = Vec::new();
            }
            NetworkEvent::IP(ip) =>{
                // Emit also the legacy events from IP
                // We don't really support legacy NoFlo stuff but just incase...
                if let Ok(ip) = IP::deserialize(ip) {
                    match ip.datatype {
                        IPType::OpenBracket(data) => {
                            self.publisher.clone().try_lock().unwrap().publish(NetworkEvent::OpenBracket(data));
                            return;
                        },
                        IPType::CloseBracket(data) => {
                            self.publisher.clone().try_lock().unwrap().publish(NetworkEvent::CloseBracket(data));
                            return;
                        },
                        IPType::Data(data) => {
                            self.publisher.clone().try_lock().unwrap().publish(NetworkEvent::IP(data));
                            return;
                        },
                        _ =>{}
                    }
                }
            }
            _ =>{}
        }
    }
}

fn connect_port(
    _socket: Arc<Mutex<InternalSocket>>,
    process: NetworkProcess,
    port: &str,
    index: Option<usize>,
    inbound: bool,
) -> Result<Arc<Mutex<InternalSocket>>, String> {
    if inbound {
        if let Ok(socket) = _socket.clone().try_lock().as_mut() {
            socket.to = Some(SocketConnection {
                port: port.to_string(),
                index,
                process: process.clone(),
            });

            if process.component.is_none() {
                return Err(format!(
                    "No inport '{}' defined in process {} ({})",
                    port,
                    process.id,
                    socket.get_id()
                ));
            }
            if let Ok(component) = process.component.clone().unwrap().try_lock().as_mut() {
                if component.get_inports().ports.is_empty()
                    || !component.get_inports().ports.contains_key(port)
                {
                    return Err(format!(
                        "No inport '{}' defined in process {} ({})",
                        port,
                        process.id,
                        socket.get_id()
                    ));
                }

                if (&component.get_inports().ports[port]).is_addressable() && index.is_none() {
                    return Err(format!(
                        "No inport '{}' defined in process {} ({})",
                        port,
                        process.id,
                        socket.get_id()
                    ));
                }
                component
                    .get_inports_mut()
                    .ports
                    .get_mut(port)
                    .unwrap()
                    .attach(_socket.clone(), index);
                return Ok(_socket.clone());
            }
        }
    }

    if let Ok(socket) = _socket.clone().try_lock().as_mut() {
        socket.from = Some(SocketConnection {
            port: port.to_string(),
            index,
            process: process.clone(),
        });

        if process.component.is_none() {
            return Err(format!(
                "No outport '{}' defined in process {} ({})",
                port,
                process.id,
                socket.get_id()
            ));
        }
        if let Ok(component) = process.component.clone().unwrap().try_lock().as_mut() {
            if component.get_outports().ports.is_empty()
                || !component.get_outports().ports.contains_key(port)
            {
                return Err(format!(
                    "No ioutport '{}' defined in process {} ({})",
                    port,
                    process.id,
                    socket.get_id()
                ));
            }

            if (&component.get_outports().ports[port]).is_addressable() && index.is_none() {
                return Err(format!(
                    "No outport '{}' defined in process {} ({})",
                    port,
                    process.id,
                    socket.get_id()
                ));
            }
            component
                .get_outports_mut()
                .ports
                .get_mut(port)
                .unwrap()
                .attach(_socket.clone(), index);
            return Ok(_socket.clone());
        }
    }

    Err("".to_string())
}
