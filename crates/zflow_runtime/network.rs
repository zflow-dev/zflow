use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    sync::{mpsc, Arc, Mutex, RwLock},
    time::Instant,
};

use crate::{
    component::{Component, ComponentEvent, ComponentOptions},
    ip::{IPOptions, IPType, IP},
    network_manager::NetworkManager,
    port::{BasePort, InPort, OutPort, PortOptions},
    process::{ProcessError, ProcessHandle, ProcessResult},
    provider::{build_node_id, BuiltInProvider, DynProviderRunner, Provider},
    runner::{DynRunFunc, RunFunc},
    sockets::{InternalSocket, SocketConnection, SocketEvent},
};
use array_tool::vec::Shift;
use fp_rust::{handler::Handler, publisher::Publisher};
use futures::executor::block_on;
use log::log;
use log::Level;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use zflow_graph::Graph;
use zflow_graph::{
    internal::utils::guid,
    types::{GraphEdge, GraphIIP, GraphNode},
};
use zflow_plugin::Platform;

#[derive(Default, Clone, Debug)]
pub struct NetworkProcess {
    pub id: String,
    pub component_name: String,
    pub component: Option<Arc<Mutex<Component>>>,
}

#[derive(Clone, Debug)]
pub struct NetworkIIP {
    pub socket: Arc<Mutex<InternalSocket>>,
    pub data: Value,
}

#[derive(Clone, Debug)]
pub enum NetworkEvent {
    OpenBracket(Value),
    CloseBracket(Value),
    Start(Value),
    End(Value),
    Terminate,
    IP(Value),
    Error(Value),
    Custom(String, Value),
    ComponentDeactivated { id: String },
    ComponentActivated { id: String },
    ComponentEvent { id: String, event: ComponentEvent },
}

#[derive(Clone, Debug, Default)]
pub struct NetworkOptions {
    pub subscribe_graph: bool,
    pub debug: bool,
    pub delay: bool,
    pub workspace_dir: String,
    // pub loader: Option<ComponentLoader>,
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
    fn set_started(&mut self, started: bool);

    // fn get_loader(&mut self) -> &mut ComponentLoader;

    /// List all available components in the network
    fn list_components(&mut self) -> Vec<String>;

    /// With the `register_component` method you can register
    /// a ZFlow Component constructor or subgraph to the Network
    ///
    /// WARNING: If component of the same namespace and name exists, this function will override it
    fn register_component(
        &mut self,
        namespace: &str,
        name: &str,
        component: Component,
    ) -> Result<(), anyhow::Error>;

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

    /// Get all active processes
    fn get_active_processes(&self) -> Vec<String>;
    fn get_startup_time(&self) -> Option<Instant>;
    /// The uptime of the network is the current time minus the start-up
    /// time, in seconds.
    fn uptime(&self) -> u128 {
        if let Some(earlier) = self.get_startup_time() {
            return Instant::now().duration_since(earlier).as_millis();
        }
        0
    }

    /// ## Loading components
    fn load(
        &mut self,
        component: &str,
        metadata: Value,
    ) -> Result<Arc<Mutex<Component>>, anyhow::Error>;

    // fn add_defaults(&mut self, node: GraphNode) -> Result<(), anyhow::Error>;

    /// ## Add a process to the network
    ///
    /// Processes can be added to a network at either start-up time
    /// or later. The processes are added with a node definition object
    /// that includes the following properties:
    ///
    /// * `id`: Identifier of the process in the network. Typically a string
    /// * `component`: A ZFlow component instance object
    fn add_node(
        &mut self,
        node: GraphNode,
        _: Option<HashMap<String, Value>>,
    ) -> Result<NetworkProcess, anyhow::Error>;
    /// Remove node from network
    fn remove_node(&mut self, node: GraphNode) -> Result<(), anyhow::Error>;
    /// Rename a node in the  network
    fn rename_node(&mut self, old_id: &str, new_id: &str) -> Result<(), anyhow::Error>;

    /// Get process by its ID.
    fn get_node(&self, id: &str) -> Option<NetworkProcess>;

    fn ensure_node(&self, node: &str, direction: &str) -> Result<NetworkProcess, anyhow::Error>;

    // fn connect(&mut self) -> Result<(), anyhow::Error>;

    /// Remove edge
    fn remove_edge(&mut self, node: GraphEdge) -> Result<(), anyhow::Error>;
    /// Remove initial packets
    fn remove_initial(&mut self, initializer: GraphIIP) -> Result<(), anyhow::Error>;
    fn send_initials(&mut self) -> Result<(), anyhow::Error>;

    fn send_defaults(&mut self) -> Result<(), anyhow::Error>;

    /// Start the network
    fn start(&mut self) -> Result<(), anyhow::Error>;
    /// Stop the network
    fn stop(&mut self) -> Result<(), anyhow::Error>;

    fn get_debug(&self) -> bool;
    fn set_debug(&mut self, active: bool);
    fn get_graph(&self) -> Arc<Mutex<Graph>>;

    fn buffered_emit(&mut self, event: NetworkEvent);

    fn get_workspace_dir(&self) -> String;
    fn on(&mut self, callback: Box<dyn FnMut(Arc<NetworkEvent>) -> () + Send + Sync + 'static>);
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
// #[derive(Clone)]
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
    pub debug: bool,
    pub event_buffer: Vec<NetworkEvent>,
    // pub loader: ComponentLoader,
    pub(crate) providers: Vec<Box<dyn Provider>>,
    pub(crate) script_runners: HashMap<String, DynProviderRunner>,
    pub(crate) scripts: HashMap<String, Box<[u8]>>,
    pub(crate) manager: Arc<Mutex<NetworkManager>>,
    _manager: NetworkManager,
    pub publisher: Arc<Mutex<Publisher<NetworkEvent>>>,
    startup_time: Option<Instant>,
    component_events: Arc<Mutex<VecDeque<(String, ComponentEvent)>>>,
    socket_events: Arc<Mutex<VecDeque<(String, Value)>>>,
    workspace_dir: String,
    pub(crate) native_components: HashMap<String, Arc<Mutex<Component>>>,
}

unsafe impl Send for Network {}
unsafe impl Sync for Network {}

impl Network {
    fn new(graph: Graph, options: NetworkOptions) -> Self {
        let workspace_dir = options.clone().workspace_dir;

        Self {
            options,
            processes: HashMap::new(),
            initials: Vec::new(),
            next_initials: Vec::new(),
            connections: Vec::new(),
            defaults: Vec::new(),
            graph: Arc::new(Mutex::new(graph)),
            debug: true,
            event_buffer: Vec::new(),
            publisher: Arc::new(Mutex::new(Publisher::new())),
            startup_time: None,
            component_events: Arc::new(Mutex::new(VecDeque::new())),
            socket_events: Arc::new(Mutex::new(VecDeque::new())),
            providers: vec![],
            script_runners: HashMap::new(),
            scripts: HashMap::new(),
            manager: Arc::new(Mutex::new(NetworkManager::new())),
            _manager: NetworkManager::new(),
            workspace_dir,
            native_components: HashMap::new(),
        }
    }

    fn subscribe_subgraph(
        network: Arc<Mutex<(impl BaseNetwork + Send + Sync + 'static + ?Sized)>>,
        node: NetworkProcess,
    ) -> Result<(), anyhow::Error> {
        if node.component.is_none() {
            return Ok(());
        }
        if let Ok(component) = node.clone().component.unwrap().clone().try_lock().as_mut() {
            if !component.is_subgraph() {
                return Ok(());
            }
            if !component.is_ready() {
                // component.on(move |event| match event.as_ref() {
                //     ComponentEvent::Ready => {
                //         Network::subscribe_subgraph(network, node.clone())
                //             .expect("expected to subscribe to subgraph");
                //     }
                //     _ => {}
                // });
                // return Ok(());
                return Network::subscribe_subgraph(network.clone(), node.clone());
            }

            // if component.get_network().is_none() {
            //     return Ok(());
            // }

            if let Ok(_this) = network.clone().try_lock().as_mut() {
                // if let Ok(_network) = component.get_network().unwrap().clone().try_lock().as_mut() {
                //     _network.set_debug(this.get_debug());
                //     // Todo: async_delivery?
                //     // Todo: enable flow_trace?
                // }
            }
            if let Ok(this) = network.clone().try_lock().as_mut() {
                if let Ok(bus) = this.get_publisher().try_lock().as_mut() {
                    bus.subscribe_fn(move |event| match event.as_ref() {
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
                                if let Ok(this) = network.clone().try_lock().as_mut() {
                                    this.buffered_emit(NetworkEvent::IP(json!(_data)));
                                }
                            }
                        }
                        NetworkEvent::Error(err) => {
                            if let Ok(this) = network.clone().try_lock().as_mut() {
                                this.buffered_emit(NetworkEvent::Error(err.clone()));
                            }
                        }
                        _ => {}
                    });
                }
            }
        }
        Ok(())
    }

    fn subscribe_node(&mut self, node: NetworkProcess) -> Result<(), anyhow::Error> {
        if node.component.is_none() {
            return Ok(());
        }

        let binding = node.component.unwrap().clone();
        let mut binding = binding.try_lock();
        let component = binding.as_mut().unwrap();

        let component_id = node.id;

        let component_events = self.component_events.clone();
        component.on(move |event| {
            if let Ok(component_events) = component_events.clone().lock().as_mut() {
                component_events.push_back((component_id.clone(), event.as_ref().clone()));
            }
        });

        Ok(())
    }
    /// Subscribe to events from all connected sockets and re-emit them
    fn subscribe_socket(
        &mut self,
        // network: Arc<Mutex<impl BaseNetwork + Send + Sync + 'static + ?Sized>>,
        socket: Arc<Mutex<InternalSocket>>,
        _: Option<NetworkProcess>,
    ) -> Result<(), anyhow::Error> {
        if let Ok(_socket) = socket.clone().try_lock().as_mut() {
            let id = _socket.get_id();
            let metadata = _socket.metadata.clone();
            let socket_events = self.socket_events.clone();
            _socket.on(move |event| match event.as_ref() {
                crate::sockets::SocketEvent::IP(ip, _) => {
                    socket_events.clone().lock().unwrap().push_back((
                        "ip".to_string(),
                        json!({
                        "id": id,
                        "data": ip.datatype,
                        "metadata": metadata
                        }),
                    ));
                }
                crate::sockets::SocketEvent::Error(err, _) => {
                    socket_events.clone().lock().unwrap().push_back((
                        "error".to_string(),
                        json!({
                        "id": id,
                        "error": err.to_owned(),
                        "metadata": metadata
                        }),
                    ));
                }
                _ => {}
            });
        }
        Ok(())
    }

    pub fn start_components(&mut self) -> Result<(), anyhow::Error> {
        if self.processes.is_empty() {
            return Ok(());
        }
        // Perform any startup routines necessary for every component.
        self.processes
            .clone()
            .par_iter_mut()
            .for_each(|(_, process)| {
                if let Some(component) = process.component.clone() {
                    if let Ok(comp) = component
                        .try_lock().as_mut() {
                            comp.start().map_err(|err|{
                                log!(target:format!("Component {}", comp.get_name().unwrap()).as_str(),log::Level::Error, "Could not start: {}", err);
                            }).expect("expected component to start");
                        }
                }
            });

        Ok(())
    }

    pub fn create(graph: Graph, options: NetworkOptions) -> Network {
        Network::new(graph, options)
    }

    /// Connect to ZFlow Network
    pub fn connect(&mut self) -> Result<&mut Self, String> {
        let builtin_provider = BuiltInProvider::new().map_err(|err| err.to_string())?;
        self.script_runners.extend(builtin_provider.dynamic_runners.clone());
        self.providers.push(Box::new(builtin_provider));

        let binding = self.get_graph();
        let graph = binding.try_lock().unwrap().clone();

        for node in graph.nodes.clone() {
            let res = self.add_node(
                node.clone(),
                Some(HashMap::from_iter([(String::from("initial"), json!(true))])),
            );

            if res.is_err() {
                return Err(format!(
                    "Could not add node {} to network: {}",
                    node.id,
                    res.err().unwrap()
                ));
            }
        }

        for edge in graph.edges.clone() {
            let res = self.add_edge(
                edge.clone(),
                Some(HashMap::from_iter([(String::from("initial"), json!(true))])),
            );
            if res.is_err() {
                return Err(format!(
                    "Could not add edge {:?} to network: {}",
                    edge,
                    res.err().unwrap()
                ));
            }
        }

        for iip in graph.initializers.clone() {
            let res = self.add_initial(
                iip.clone(),
                Some(HashMap::from_iter([(String::from("initial"), json!(true))])),
            );

            if res.is_err() {
                return Err(format!(
                    "Could not add IIP {:?} to network: {:?}",
                    iip,
                    res.err().unwrap()
                ));
            }
        }

        for node in graph.nodes.clone() {
            let res = self.add_defaults(node.clone());
            if res.is_err() {
                return Err(format!(
                    "Could not add defaults {} to network: {:?}",
                    node.id,
                    res.err().unwrap()
                ));
            }
        }

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(0)
            .build()
            .unwrap();

        // Let network sync with the components' events
        let (cmp_tx, cmp_rx) = mpsc::sync_channel(2);
        let component_events = self.component_events.clone();
        let publisher = self.publisher.clone();
        let manager = self.manager.clone();

        pool.spawn(move || {
            'outter: loop {
                let binding = component_events.clone();
                let mut binding = binding.lock();
                let events = binding.as_mut().unwrap();

                while events.is_empty() {
                    match cmp_rx.try_recv() {
                        Ok("end_network") => {
                            break 'outter;
                        }
                        _ => {
                            break;
                        }
                    }
                }
                // while !events.is_empty() {
                events
                    .clone()
                    .iter()
                    .enumerate()
                    .for_each(|(i, (id, event))| match event {
                        ComponentEvent::Activate(c) => {
                            if let Ok(pu) = publisher.clone().try_lock().as_mut() {
                                pu.publish(NetworkEvent::ComponentActivated { id: id.clone() });
                            }
                            let manager = manager.clone();
                            if let Ok(manager) = manager.clone().try_lock().as_mut() {
                                if (manager).debounce_end {
                                    (manager).abort_debounce = true;
                                }

                                (manager).weight = *c;
                            }

                            events.remove(i);
                        }
                        ComponentEvent::Deactivate(c) => {
                            if let Ok(pu) = publisher.clone().try_lock().as_mut() {
                                pu.publish(NetworkEvent::ComponentDeactivated { id: id.clone() });
                            }

                            let manager = manager.clone();
                            if let Ok(manager) = manager.clone().try_lock().as_mut() {
                                (manager).weight = *c;
                                (manager).check_if_finished();
                            }
                            drop(manager);

                            events.remove(i);
                        }
                        ComponentEvent::Icon(new_icon) => {
                            if let Ok(pu) = publisher.clone().try_lock().as_mut() {
                                pu.publish(NetworkEvent::Custom(
                                    "icon".to_owned(),
                                    new_icon.clone(),
                                ));
                            }
                            events.remove(i);
                        }
                        _ => {
                            if let Ok(pu) = publisher.clone().try_lock().as_mut() {
                                pu.publish(NetworkEvent::ComponentEvent {
                                    id: id.clone(),
                                    event: event.clone(),
                                });
                            }
                            events.remove(i);
                        }
                    });
                // }
            }
        });

        // Let network sync with the sockets' events
        let (sck_tx, sck_rx) = mpsc::sync_channel(2);
        let socket_events = self.socket_events.clone();
        let _publisher = self.publisher.clone();
        pool.spawn(move || 'outter: loop {
            let binding = socket_events.clone();
            let mut binding = binding.lock();
            let events = binding.as_mut().unwrap();

            while events.is_empty() {
                match sck_rx.try_recv() {
                    Ok("end_network") => {
                        break 'outter;
                    }
                    _ => {
                        break;
                    }
                }
            }

            // events
            //     .clone()
            //     .iter()
            //     .enumerate()
            //     .for_each(|(i, event)| match event {
            //         (evtype, data) => {
            //             match evtype.as_str() {
            //                 "ip" => {
            //                     let notifier = publisher.clone();

            //                     notifier
            //                         .try_lock()
            //                         .unwrap()
            //                         .publish(NetworkEvent::IP(data.clone()));
            //                     drop(notifier);
            //                 }
            //                 "error" => {
            //                     publisher
            //                         .clone()
            //                         .try_lock()
            //                         .unwrap()
            //                         .publish(NetworkEvent::Error(data.clone()));
            //                 }
            //                 _ => {}
            //             }
            //             events.remove(i);
            //         }
            //     });
        });

        let pubsub = self.publisher.clone();
        let mut pubsub = pubsub.try_lock();
        let publisher = pubsub.as_mut().unwrap();
        let pool = Arc::new(Mutex::new(pool));
        publisher.subscribe_fn(move |event| match event.as_ref() {
            NetworkEvent::Terminate => {
                cmp_tx.send("end_network").unwrap();
                sck_tx.send("end_network").unwrap();
                if let Ok(pool) = pool.clone().try_lock() {
                    drop(pool);
                }
            }
            _ => {}
        });

        Ok(self)
    }

    // Add edge to network
    pub fn add_edge(
        &mut self,
        edge: GraphEdge,
        options: Option<HashMap<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, anyhow::Error> {
        let socket = InternalSocket::create(edge.clone().metadata);

        let from = self.ensure_node(&edge.from.node_id, "outbound")?;

        // Todo: configure socket to support async and debug
        let to = self.ensure_node(&edge.to.node_id, "inbound")?;

        // Subscribe to events from the socket
        self.subscribe_socket(socket.clone(), Some(from.clone()))?;
        connect_port(socket.clone(), to, &edge.to.port, edge.to.index, true)?;
        connect_port(
            socket.clone(),
            from.clone(),
            &edge.from.port,
            edge.from.index,
            false,
        )?;

        self.get_connections_mut().push(socket.clone());
        if let Ok(graph) = self.get_graph().clone().try_lock().as_mut() {
            let op = if let Some(op) = options {
                json!(op).as_object().cloned()
            } else {
                None
            };
            graph.add_edge(
                &edge.from.node_id,
                &edge.from.port,
                &edge.to.node_id,
                &edge.to.port,
                op,
            );
        }

        Ok(socket.clone())
    }
    /// Add initial packets
    pub fn add_initial(
        &mut self,
        initializer: GraphIIP,
        options: Option<HashMap<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, anyhow::Error> {
        if let Some(leaf) = initializer.clone().to {
            let to = self.ensure_node(&leaf.node_id, "inbound")?;

            // Todo: configure socket to support async and debug
            let socket = InternalSocket::create(initializer.metadata);
            // Subscribe to events from the socket
            self.subscribe_socket(socket.clone(), None)?;
            let socket = connect_port(socket.clone(), to, &leaf.port, leaf.index, true)?;

            self.get_connections_mut().push(socket.clone());
            let init = NetworkIIP {
                socket: socket.clone(),
                data: initializer.from.clone().unwrap().data,
            };

            self.get_initials_mut().push(init.clone());
            self.get_next_initials_mut().push(init.clone());

            let manager = self.manager.clone();
            let manager = manager.lock().unwrap();

            if manager.is_running() {
                // Network is running now, send initials immediately
                self.send_initials()?;
            } else if !manager.is_stopped() {
                drop(manager);
                // Network has finished but hasn't been stopped, set
                // started and set
                self.set_started(true);
                self.send_initials()?;
            }

            if let Ok(graph) = self.get_graph().clone().try_lock().as_mut() {
                let op = if let Some(op) = options {
                    json!(op).as_object().cloned()
                } else {
                    None
                };
                if let Some(op) = op {
                    if op.contains_key("initial") {
                        graph.add_initial_index(
                            initializer.from.unwrap().data,
                            &leaf.node_id,
                            &leaf.port,
                            leaf.index,
                            Some(op),
                        );
                    }
                }
            }
            return Ok(socket.clone());
        }
        Err(anyhow::Error::msg("Could not add initial packet".to_string()))
    }

    pub fn add_defaults(&mut self, node: GraphNode) -> Result<(), anyhow::Error> {
        let process = self.ensure_node(&node.id, "inbound")?;

        if let Some(component) = process.clone().component {
            let binding = component.clone();
            let mut binding = binding.try_lock();
            let component = binding.as_mut().unwrap();
            let ports = component.clone().get_inports_mut().ports.clone();
            drop(binding);
            for (key, port) in ports {
                // Attach a socket to any defaulted inPorts as long as they aren't already attached.
                if !port.has_default() || port.is_attached(None) {
                    break;
                }

                let socket = InternalSocket::create(None);
                // Subscribe to events from the socket
                let _ = self.subscribe_socket(socket.clone(), None)?;

                let connect =
                    connect_port(socket.clone(), process.clone(), key.as_str(), None, true);

                if connect.is_ok() {
                    self.get_connections_mut().push(socket.clone());
                    self.get_defaults_mut().push(socket.clone());
                }
            }
        }

        Ok(())
    }

    /// Register a ZFlow Provider.
    pub fn register_provider<T: Provider + Sized + 'static>(&mut self, provider: T) {
        self.providers.push(Box::new(provider));
    }

    /// Register a ZFlow script runner. A script runner is a callback function that can run a code script from it's source string, it also provides a handle to access network node inputs
    pub fn register_script_runner(&mut self, id: &str, runner: Box<DynRunFunc>) {
        let runner = Box::leak(runner);
        self.script_runners.insert(
            id.to_owned(),
            DynProviderRunner {
                runner_id: id.to_owned(),
                runner_func: Arc::new(Mutex::new(move |src, handle: Arc<Mutex<ProcessHandle>>| {
                    (runner)(src, handle)
                })),
                platforms: vec![Platform::System],
            },
        );
    }

    pub fn load_code(
        &mut self,
        name: &str,
        source: Box<[u8]>,
        runner_id: &str,
        metadata: Option<Map<String, Value>>,
    ) -> Result<NetworkProcess, anyhow::Error> {
        if !self.script_runners.contains_key(runner_id) {
            return Err(anyhow::Error::msg(
                "No runner available to execute this stript",
            ));
        }


        let mut in_ports = HashMap::from_iter([("in".to_owned(), InPort::new(PortOptions{
            triggering: true,
            required: true,
            ..Default::default()
        }))]);
        let mut out_ports = HashMap::from_iter([("out".to_owned(), OutPort::new(PortOptions{
            required: true,
            ..Default::default()
        }))]);

        if let Some(meta) = metadata.clone() {
            if let Some(inports) = meta.get("inports") {
                if let Ok(_in) = HashMap::<String, PortOptions>::deserialize(inports) {
                    in_ports = _in.iter().map(|(k, v)| (k.clone(), InPort::new(v.clone()))).collect();
                }
            }
            if let Some(outports) = meta.get("outports") {
                if let Ok(_out) = HashMap::<String, PortOptions>::deserialize(outports) {
                    out_ports = _out.iter().map(|(k, v)| (k.clone(), OutPort::new(v.clone()))).collect();
                }
            }
        }
        
       
        let runner = &self.script_runners.clone()[runner_id];
        let func = runner.runner_func.clone();
        let code = source.clone();
        let component = ComponentOptions {
            in_ports,
            out_ports,
            activate_on_input: true,
            process: Some(Box::new(move |handle| {
                let binding = func.clone();
                let mut func = binding.try_lock()
                    .map_err(|err| ProcessError(err.to_string()))?;
                (func)(code.clone(), handle.clone())
            })),
            ..Default::default()
        };

        
        self.register_component("script", name, Component::new(component))?;
        self.add_node(
            GraphNode {
                id: build_node_id("script", name),
                component: String::from_utf8(source.into()).unwrap(),
                metadata,
            },
            None,
        )
    }

    pub fn load_script(
        &mut self,
        name: &str,
        source: &str,
        runner_id: &str,
        metadata: Option<Map<String, Value>>,
    ) -> Result<NetworkProcess, anyhow::Error> {
        self.load_code(name, source.as_bytes().into(), runner_id, metadata)
    }

    pub(crate) fn get_subgraph_runner(
        graph: Graph,
    ) -> (
        Box<RunFunc>,
        Arc<Mutex<NetworkManager>>,
        Arc<Mutex<Publisher<NetworkEvent>>>,
    ) {
        let mut graph_network = Network::new(
            graph,
            NetworkOptions {
                subscribe_graph: true,
                ..Default::default()
            },
        );

        let graph_network_manager = graph_network.manager.clone();
        let graph_network_pub = graph_network.publisher.clone();

        let ret_manager = graph_network_manager.clone();

        return (
            Box::new(move |handle| {
                if let Ok(this) = handle.clone().try_lock().as_mut() {
                    let network = graph_network.connect().map_err(|err| ProcessError(err))?;
                    network.start().map_err(|err| ProcessError(err.to_string()))?;
                    loop {
                        if let Ok(manager) = graph_network_manager.clone().try_lock() {
                            if manager.weight == 0 || (*manager).is_stopped() {
                                this.output.send_done(&json!({}))?;
                                break;
                            }
                        }
                    }
                }
                Ok(ProcessResult::default())
            }),
            ret_manager.clone(),
            graph_network_pub,
        );
    }
}

impl BaseNetwork for Network {
    fn on(&mut self, callback: Box<dyn FnMut(Arc<NetworkEvent>) -> () + Send + Sync + 'static>) {
        self.publisher
            .clone()
            .try_lock()
            .unwrap()
            .subscribe_fn(callback);
    }

    fn load(
        &mut self,
        component: &str,
        metadata: Value,
    ) -> Result<Arc<Mutex<Component>>, anyhow::Error> {
        if let Some(component) = self.native_components.get(component) {
            return Ok(component.clone());
        }
        let base_dir = self.workspace_dir.clone();
        let par_iter = self.providers.par_iter_mut();
        if let Some((_component, runner)) = par_iter.find_map_any(|provider| {
            provider.set_workspace(base_dir.clone());
            return provider
                .load_component(component.to_owned())
                .map_err(|err| {
                    log::log!(Level::Error, "{:?}", err);
                    err
                })
                .ok();
        }) {
            let comp = _component;
            let in_ports: HashMap<String, InPort> = comp
                .get_inports()
                .par_iter()
                .map(|(name, options)| (name.to_owned(), InPort::new(options.clone())))
                .collect();
            let out_ports: HashMap<String, OutPort> = comp
                .get_outports()
                .par_iter()
                .map(|(name, options)| (name.to_owned(), OutPort::new(options.clone())))
                .collect();

            let description = comp.get_description();
            let icon = comp.get_icon()?;
            let ordered = comp.is_ordered();
            let activate_on_input = comp.should_activate_on_input();
            let forward_brackets = comp.get_forward_brackets();
            let mut _meta = comp.get_metadata();

            if let Some(metadata) = metadata.as_object() {
                _meta.extend(metadata.clone());
            }

            let is_graph = comp.is_subgraph();

            let process_func = if !is_graph {
                runner.runner_func
            } else {
                if let Some(graph) = _component.as_any().downcast_ref::<Graph>() {
                    let (graph_runner, _graph_network_manager, _graph_network_publisher) =
                        Self::get_subgraph_runner(graph.clone());
                    //Todo: sync graph network with main network
                    graph_runner
                } else {
                    Box::new(|_| Ok(ProcessResult::default()))
                }
            };

            let process = Component::init(ComponentOptions {
                in_ports,
                out_ports,
                description,
                ordered,
                activate_on_input,
                forward_brackets,
                icon,
                metadata: Some(_meta),
                process: Some(process_func),
                ..Default::default()
            });
            return Ok(process);
        }

        Err(anyhow::Error::msg("Component not found"))
    }

    fn get_active_processes(&self) -> Vec<String> {
        let mut active = vec![];
        let manager = self.manager.clone();
        let manager = manager.try_lock().unwrap();
        if !(manager).started {
            return active;
        }

        self.get_processes().iter().for_each(|(name, process)| {
            if let Some(component) = process.component.clone() {
                if let Ok(component) = &component.clone().try_lock() {
                    let inner_thread_alive = component
                        .get_handler_thread()
                        .try_lock()
                        .unwrap()
                        .is_alive();

                    if component.get_load() > 0 || inner_thread_alive {
                        active.push(name.to_string());
                    }
                }
            }
        });

        active
    }

    fn set_started(&mut self, started: bool) {
        let manager = self.manager.clone();
        let mut manager = manager.lock().unwrap();
        if manager.started == started {
            return;
        }

        if !started {
            // Ending the execution
            manager.started = false;
            drop(manager);

            // buffered emit
            let now: u128 = Instant::now().elapsed().as_millis();
            if let Some(started) = self.startup_time {
                let started = started.elapsed().as_millis();
                self.buffered_emit(NetworkEvent::End(json!({
                    "start": started/1000,
                    "end": now/1000,
                    "uptime": self.uptime()/1000
                })));
            } else {
                self.buffered_emit(NetworkEvent::End(json!({
                    "start": 0,
                    "end": now/1000,
                    "uptime": self.uptime()/1000
                })));
            }

            return;
        }

        // Starting the execution
        if self.startup_time.is_none() {
            self.startup_time = Some(Instant::now());
        }

        manager.started = true;

        manager.stopped = false;

        drop(manager);

        // buffered emit
        let started = self.startup_time.unwrap().elapsed().as_millis();
        self.buffered_emit(NetworkEvent::Start(
            json!({ "start": started, "end": null, "uptime": null }),
        ));
    }

    fn get_processes(&self) -> HashMap<String, NetworkProcess> {
        self.processes.clone()
    }

    fn get_startup_time(&self) -> Option<Instant> {
        self.startup_time
    }

    fn add_node(
        &mut self,
        node: GraphNode,
        _: Option<HashMap<String, Value>>,
    ) -> Result<NetworkProcess, anyhow::Error> {
        // Processes are treated as singletons by their identifier. If
        // we already have a process with the given ID, return that.
        if self.get_processes().contains_key(&node.id) {
            return Ok(self.get_processes().get(&node.id.clone()).unwrap().clone());
        }
        let mut process = NetworkProcess {
            id: node.id.clone(),
            ..NetworkProcess::default()
        };
        if node.component.is_empty() {
            // No component defined, just register the process but don't start.
            self.get_processes_mut().insert(node.id.clone(), process);
            return Ok(self.get_processes().get(&node.id.clone()).unwrap().clone());
        }

        // Load the component for the process.
        let _instance = self.load(&node.id, json!(node.metadata))?;
        let binding = _instance.clone();
        let mut binding = binding.try_lock();
        let instance = binding.as_mut().unwrap();

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

        // let _ = Network::subscribe_subgraph(network.clone(), process.clone())?;

        // Store and return the process instance
        self.get_processes_mut()
            .insert(node.id.clone(), process.clone());
        if let Ok(graph) = self.get_graph().clone().try_lock().as_mut() {
            graph.add_node(&node.id, &node.component, node.metadata);
        }

        drop(binding);
        let _ = self.subscribe_node(process.clone())?;
        Ok(process)
    }

    fn remove_node(&mut self, node: GraphNode) -> Result<(), anyhow::Error> {
        if let Some(process) = self.get_node(&node.id) {
            if process.component.is_none() {
                self.processes.remove(&node.id);
                return Ok(());
            }
            let _ = process
                .component
                .unwrap()
                .clone()
                .try_lock()
                .unwrap()
                .shutdown();
            self.processes.remove(&node.id);
            if let Ok(graph) = self.get_graph().try_lock().as_mut() {
                graph.remove_node(&node.id);
            }
            return Ok(());
        }

        Err(anyhow::Error::msg(format!("Process {} not found", node.id)))
    }

    fn rename_node(&mut self, old_id: &str, new_id: &str) -> Result<(), anyhow::Error> {
        if let Some(mut process) = self.get_node(old_id) {
            // Inform the process of its ID
            process.id = new_id.to_owned();

            if let Some(component) = process.component.clone() {
                let binding = component.clone();
                let mut binding = binding.try_lock();
                let binding = binding.as_mut();
                let instance = binding.unwrap();

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
                instance.node_id = new_id.to_string();
                return Ok(());
            }
        }
        Err(anyhow::Error::msg(format!("Process {} not found", old_id)))
    }

    fn get_node(&self, id: &str) -> Option<NetworkProcess> {
        self.processes.get(id).cloned()
    }

    fn ensure_node(&self, node: &str, direction: &str) -> Result<NetworkProcess, anyhow::Error> {
        if let Some(process) = self.get_node(node) {
            if process.component.is_none() {
                return Err(anyhow::Error::msg(format!(
                    "No component defined for {} node {}",
                    direction, node
                )));
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
                    let ready = Arc::new(RwLock::new(json!(false)));
                    component.on(move |event| match event.as_ref() {
                        ComponentEvent::Ready => {
                            // let ready = ready.clone();
                            let mut r = ready.write().unwrap();
                            *r = json!(true);
                        }
                        _ => {}
                    });

                    return self.ensure_node(node, direction);
                }
            }
            return Ok(process);
        }

        Err(anyhow::Error::msg(format!(
            "No process defined for {} node {}",
            direction, node
        )))
    }

    fn remove_edge(&mut self, edge: GraphEdge) -> Result<(), anyhow::Error> {
        self.connections
            .clone()
            .iter()
            .enumerate()
            .for_each(|(i, connection)| {
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
                    if let Some(inport) = connection
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
                    {
                        if connection.index == i {
                            inport.sockets.remove(i);
                            inport
                                .bus
                                .clone()
                                .try_lock()
                                .as_mut()
                                .expect("expected instance of publisher")
                                .publish(SocketEvent::Detach(connection.index));
                        }
                    }

                    if !edge.from.node_id.is_empty() {
                        if connection.from.is_some()
                            && edge.from.node_id == connection.from.clone().unwrap().process.id
                            || edge.from.port == connection.from.clone().unwrap().port
                        {
                            if let Some(outport) = connection
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
                            {
                                if connection.index == i {
                                    outport.sockets.remove(i);
                                    outport
                                        .bus
                                        .clone()
                                        .try_lock()
                                        .as_mut()
                                        .expect("expected instance of publisher")
                                        .publish(SocketEvent::Detach(connection.index));
                                }
                            }
                        }
                    }
                    self.connections.remove(i);
                }
            });
        if let Ok(graph) = self.get_graph().try_lock().as_mut() {
            graph.remove_edge(
                &edge.from.node_id,
                &edge.from.port,
                Some(&edge.to.node_id),
                Some(&edge.to.port),
            );
        }
        Ok(())
    }

    fn remove_initial(&mut self, initializer: GraphIIP) -> Result<(), anyhow::Error> {
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
            graph.remove_initial(
                &initializer.to.clone().unwrap().node_id,
                &initializer.to.clone().unwrap().port,
            );
        }
        Ok(())
    }

    fn send_initials(&mut self) -> Result<(), anyhow::Error> {
        self.initials = self
            .initials
            .clone()
            .par_iter()
            .filter(|initial| {
                if let Ok(socket) = initial.socket.clone().try_lock().as_mut() {
                    let post = socket.post(
                        Some(IP::new(
                            IPType::Data(initial.data.clone()),
                            IPOptions {
                                initial: true,
                                ..IPOptions::default()
                            },
                        )),
                        true,
                    );

                    if post.is_err() {
                        return true;
                    }

                    return false;
                }
                true
            })
            .map(|iip| iip.clone())
            .collect();
        // self.initials.clear();
        Ok(())
    }

    fn send_defaults(&mut self) -> Result<(), anyhow::Error> {
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
                    let _ = block_on(socket.send(None));
                    let _ = socket.disconnect();
                }
            })
            .collect();
        Ok(())
    }

    fn start(&mut self) -> Result<(), anyhow::Error> {
        let _manager = self.manager.clone();
        let m = _manager.lock();
        let mut manager = m.unwrap();
        if manager.debounce_end {
            manager.abort_debounce = true;
        }

        let is_started = manager.started;

        if is_started {
            drop(manager);
            self.stop()?;
            self.start()?;
            return Ok(());
        }
        drop(manager);

        self.initials = self.next_initials.clone();
        self.event_buffer.clear();

        // start providers
        self.providers.par_iter_mut().for_each(|prov| {
            prov.start().unwrap();
        });

        self.start_components()?;
        self.send_initials()?;
        self.send_defaults()?;
        self.set_started(true);

        Ok(())
    }

    fn stop(&mut self) -> Result<(), anyhow::Error> {
        if (self._manager).debounce_end {
            self._manager.abort_debounce = true;
        }
        if !self._manager.started {
            self._manager.stopped = true;

            self.publisher
                .clone()
                .try_lock()
                .unwrap()
                .publish(NetworkEvent::Terminate);
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

            self._manager.stopped = true;

            self.publisher
                .clone()
                .try_lock()
                .unwrap()
                .publish(NetworkEvent::Terminate);
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
                return process
                    .component
                    .clone()
                    .unwrap()
                    .try_lock()
                    .unwrap()
                    .shutdown();
            })
            .collect();

        self.set_started(false);

        self._manager.stopped = true;

        self.publisher
            .clone()
            .try_lock()
            .unwrap()
            .publish(NetworkEvent::Terminate);

        if let Ok(manager) = self.manager.clone().try_lock().as_mut() {
            self._manager = manager.update(self._manager);
        }

        // Stop remote providers
        // Todo: wait for remote processes to finish?
        let _processes = self.processes.clone();

        loop {
            let has_load = _processes
                .iter()
                .all(|(_, p)| p.component.clone().unwrap().lock().unwrap().get_load() > 0);
            if !has_load {
                self.providers.par_iter_mut().for_each(|provider| {
                    let _ = provider.stop();
                });
                break;
            }
        }

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

        self.connections.par_iter().for_each(|socket| {
            if let Ok(socket) = socket.clone().try_lock().as_mut() {
                socket.set_debug(active);
            }
        });
        for (_, process) in self.processes.clone() {
            if process.component.is_none() {
                return;
            }
            if let Some(instance) = process.component.clone() {
                if let Ok(instance) = instance.clone().try_lock().as_mut() {
                    if instance.is_subgraph() {
                        // if let Some(network) = instance.get_network() {
                        //     if let Ok(network) = network.clone().try_lock().as_mut() {

                        //     }
                        // }
                        self.set_debug(active);
                    }
                }
            }
        }
        self.processes.iter_mut().for_each(move |(_, _)| {});
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
            NetworkEvent::End(_) | NetworkEvent::Error(_) | NetworkEvent::Custom(_, _) => {
                self.publisher.clone().try_lock().unwrap().publish(event);
                return;
            }
            _ => {}
        }
        let manager = self.manager.clone();
        let manager = manager.try_lock().unwrap();
        if !(manager).started {
            match event {
                NetworkEvent::End(_) => {}
                _ => {
                    self.event_buffer.push(event);
                    return;
                }
            }
        }

        self.publisher
            .clone()
            .try_lock()
            .unwrap()
            .publish(event.clone());
        match event {
            NetworkEvent::Start(_) => {
                // Once network has started we can send the IP-related events
                self.event_buffer.clone().par_iter().for_each(|ev| {
                    self.publisher
                        .clone()
                        .try_lock()
                        .unwrap()
                        .publish(ev.clone());
                });
                self.event_buffer = Vec::new();
            }
            NetworkEvent::IP(ip) => {
                // Emit also the legacy events from IP
                // We don't really support legacy NoFlo stuff but just incase...
                if let Ok(ip) = IP::deserialize(ip) {
                    match ip.datatype {
                        IPType::OpenBracket(data) => {
                            self.publisher
                                .clone()
                                .try_lock()
                                .unwrap()
                                .publish(NetworkEvent::OpenBracket(data));
                            return;
                        }
                        IPType::CloseBracket(data) => {
                            self.publisher
                                .clone()
                                .try_lock()
                                .unwrap()
                                .publish(NetworkEvent::CloseBracket(data));
                            return;
                        }
                        IPType::Data(data) => {
                            self.publisher
                                .clone()
                                .try_lock()
                                .unwrap()
                                .publish(NetworkEvent::IP(data));
                            return;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn get_workspace_dir(&self) -> String {
        self.workspace_dir.clone()
    }

    fn list_components(&mut self) -> Vec<String> {
        let base_dir = self.workspace_dir.clone();
        let par_iter = self.providers.par_iter_mut();
        let mut components: Vec<String> = self
            .native_components
            .par_iter()
            .map(|(k, _)| k.clone())
            .collect();
        components.extend(
            par_iter
                .map(|provider| {
                    provider.set_workspace(base_dir.clone());
                    provider.list_components().unwrap()
                })
                .flatten()
                .collect::<Vec<String>>(),
        );

        components
    }

    fn register_component(
        &mut self,
        namespace: &str,
        name: &str,
        component: Component,
    ) -> Result<(), anyhow::Error> {
        let id = build_node_id(namespace, name);
        self.native_components
            .insert(id, Component::from_instance(component));

        Ok(())
    }
}

fn connect_port(
    _socket: Arc<Mutex<InternalSocket>>,
    process: NetworkProcess,
    port: &str,
    index: Option<usize>,
    inbound: bool,
) -> Result<Arc<Mutex<InternalSocket>>, anyhow::Error> {
    if inbound {
        let socket_id = if let Ok(socket) = _socket.clone().try_lock().as_mut() {
            socket.to = Some(SocketConnection {
                port: port.to_string(),
                index,
                process: process.clone(),
            });

            if process.component.is_none() {
                return Err(anyhow::Error::msg(format!(
                    "No inport '{}' defined in process {} ({})",
                    port,
                    process.id,
                    socket.get_id()
                )));
            }
            Some(socket.get_id())
        } else {
            None
        };

        if let Ok(component) = process.component.clone().unwrap().try_lock().as_mut() {
            if component.get_inports().ports.is_empty()
                || !component.get_inports().ports.contains_key(port)
            {
                return Err(anyhow::Error::msg(format!(
                    "No inport '{}' defined in process {} ({:?})",
                    port,
                    process.id,
                    socket_id.clone()
                )));
            }

            if (&component.get_inports().ports[port]).is_addressable() && index.is_none() {
                return Err(anyhow::Error::msg(format!(
                    "No inport '{}' defined in process {} ({:?})",
                    port, process.id, socket_id
                )));
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

    if let Ok(socket) = _socket.clone().try_lock().as_mut() {
        socket.from = Some(SocketConnection {
            port: port.to_string(),
            index,
            process: process.clone(),
        });

        if process.component.is_none() {
            return Err(anyhow::Error::msg(format!(
                "No outport '{}' defined in process {} ({})",
                port,
                process.id,
                socket.get_id()
            )));
        }
        if let Ok(component) = process.component.clone().unwrap().try_lock().as_mut() {
            if component.get_outports().ports.is_empty()
                || !component.get_outports().ports.contains_key(port)
            {
                return Err(anyhow::Error::msg(format!(
                    "No ioutport '{}' defined in process {} ({})",
                    port,
                    process.id,
                    socket.get_id()
                )));
            }

            if (&component.get_outports().ports[port]).is_addressable() && index.is_none() {
                return Err(anyhow::Error::msg(format!(
                    "No outport '{}' defined in process {} ({})",
                    port,
                    process.id,
                    socket.get_id()
                )));
            }
            component
                .get_outports_mut()
                .ports
                .get_mut(port)
                .unwrap()
                .attach(_socket.clone(), index);
        }
    }
    return Ok(_socket.clone());
}
