use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::SystemTime,
};


use serde_json::{Map, Value};
use zflow::graph::{
    graph::Graph,
    types::{GraphEdge, GraphIIP, GraphNode},
};

use crate::{
    component::{BaseComponentTrait, ComponentTrait},
    graph_component::GraphComponentTrait,
    loader::ComponentLoader,
    sockets::InternalSocket,
};

pub struct NetworkProcess<T: ComponentTrait> {
    pub id: String,
    pub component_name: String,
    pub component: Arc<Mutex<T>>,
}

pub struct NetworkIIP {
    pub socket: Arc<Mutex<InternalSocket>>,
    pub data: Value,
}

pub enum NetworkEvent {
    OpenBracket(Value),
    CloseBracket(Value),
    Start,
    End,
}

pub struct NetworkOptions {}

/// ## The ZFlow Network
///
/// ZFlow networks consist of processes connected to each other
/// via sockets attached from outports to inports.
///
/// The role of the network coordinator is to take a graph and
/// instantiate all the necessary processes from the designated
/// components, attach sockets between them, and handle the sending
/// of Initial Information Packets.
pub trait BaseNetwork<T:ComponentTrait>
{


    fn is_started(&self) -> bool;
    fn is_stopped(&self) -> bool;
    fn is_running(&self) -> bool {
        self.get_active_processes().len() > 0
    }
    fn set_started(&mut self, started: bool);

    fn get_loader(&mut self) -> &mut Option<ComponentLoader<T>>;

    /// Processes contains all the instantiated components for this network
    fn get_processes(&self) -> HashMap<String, NetworkProcess<T>>;
    fn get_active_processes(&self) -> Vec<String> {
        let mut active = vec![];
        if !self.is_started() {
            return active;
        }

        self.get_processes().iter().for_each(|(name, process)| {
            if let Ok(component) = &process.component.try_lock() {
                if component.get_load() > 0 {
                    active.push(name.to_string());
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
    /// with component instance object
    fn load(
        &mut self,
        component: Arc<Mutex<T>>,
        metadata: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<T>>, String> {
        Err("unimplemented".to_string())
    }
    /// ## Loading components
    /// from path
    fn load_from_path(
        &mut self,
        component: &Path,
        metadata: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<T>>, String> {
        Err("unimplemented".to_string())
    }

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
        options: Option<Map<String, Value>>,
    ) -> Result<NetworkProcess<T>, String>;

    /// Remove node from network
    fn remove_node(&mut self, node: GraphNode) -> Result<(), String>;
    /// Rename a node in the  network
    fn rename_node(&mut self, old_id: &str, new_id: &str) -> Result<(), String>;

    /// Get process by its ID.
    fn get_node(&self, id: &str) -> Option<NetworkProcess<T>>;

    fn connect(&self);

    /// Add edge
    fn add_edge(
        &mut self,
        edge: GraphEdge,
        options: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String>;

    /// Remove edge
    fn remove_edge(&mut self, node: GraphEdge) -> Result<(), String>;

    /// Add initial packets
    fn add_initial(
        &mut self,
        initializer: GraphIIP,
        options: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String>;

    /// Remove initial packets
    fn remove_initial(&mut self, initializer: GraphIIP) -> Result<(), String>;

    fn send_defaults(&self) -> Result<(), String>;

    fn start(&self) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;

    fn get_debug(&self) -> bool;
    fn set_debug(&mut self, active: bool);
}

pub trait NetworkSubsciption<'a,C:ComponentTrait, T: BaseNetwork<C>>
{
    type Network;
    fn subscribe_subgraph(
        network: Arc<Mutex<T>>,
        node: NetworkProcess<C>,
    ) -> Result<(), String>;

    /// Subscribe to events from all connected sockets and re-emit them
    fn subscribe_socket(
        network: Arc<Mutex<T>>,
        socket: Arc<Mutex<InternalSocket>>,
        source: NetworkProcess<C>,
    ) -> Result<(), String>;

    fn subscribe_node(
        network: Arc<Mutex<T>>,
        node: NetworkProcess<C>,
    ) -> Result<(), String>;
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
pub struct Network<T: ComponentTrait + GraphComponentTrait> {
    pub options: NetworkOptions,
    /// Processes contains all the instantiated components for this network
    pub processes: HashMap<String, NetworkProcess<T>>,
    /// Initials contains all Initial Information Packets (IIPs)
    pub initials: Vec<NetworkIIP>,
    pub next_initials: Vec<NetworkIIP>,
    /// Connections contains all the socket connections in the network
    pub connections: Vec<Arc<Mutex<InternalSocket>>>,
    /// Container to hold sockets that will be sending default data.
    pub defaults: Vec<Arc<Mutex<InternalSocket>>>,
    /// The Graph this network is instantiated with
    pub graph: Graph,
    pub started: bool,
    pub stopped: bool,
    pub debug: bool,
    pub event_buffer: Vec<NetworkEvent>,
    pub loader: Option<ComponentLoader<T>>,
}

impl<T:ComponentTrait + GraphComponentTrait> Network<T> {
    pub fn new(graph:Graph, options: NetworkOptions) -> Self {
        Self {
            options,
            processes: HashMap::new(),
            initials: Vec::new(),
            next_initials: Vec::new(),
            connections: Vec::new(),
            defaults: Vec::new(),
            graph,
            started: false,
            stopped: false,
            debug: false,
            event_buffer: Vec::new(),
            loader: None
        }
    }
}

impl<T> BaseNetwork<T> for Network<T>
where
    T: ComponentTrait + GraphComponentTrait,
{


    fn get_loader(&mut self) -> &mut Option<ComponentLoader<T>> {
        &mut self.loader
    }

    fn is_running(&self) -> bool {
        self.get_active_processes().len() > 0
    }

    fn get_active_processes(&self) -> Vec<String> {
        let mut active = vec![];
        if !self.is_started() {
            return active;
        }
        self.get_processes().iter().for_each(|(name, process)| {
            if let Ok(component) = &process.component.try_lock() {
                if component.get_load() > 0 {
                    active.push(name.to_string());
                }
            }
        });
        active
    }

    fn uptime(&self) -> u128 {
        if let Some(earlier) = self.get_startup_time() {
            return SystemTime::now()
                .duration_since(earlier)
                .unwrap()
                .as_millis();
        }
        0
    }

    fn load(
        &mut self,
        component: Arc<Mutex<T>>,
        metadata: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<T>>, String> {
        Err("unimplemented".to_string())
    }

    fn load_from_path(
        &mut self,
        component: &Path,
        metadata: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<T>>, String> {
        Err("unimplemented".to_string())
    }

    fn is_started(&self) -> bool {
        todo!()
    }

    fn is_stopped(&self) -> bool {
        todo!()
    }

    fn set_started(&mut self,started:bool) {
        todo!()
    }

    fn get_processes(&self) -> HashMap<String,NetworkProcess<T> >  {
        todo!()
    }

    fn get_startup_time(&self) -> Option<SystemTime>  {
        todo!()
    }

    fn add_node(&mut self,node:GraphNode,options:Option<Map<String,Value> > ,) -> Result<NetworkProcess<T> ,String>  {
        todo!()
    }

    fn remove_node(&mut self,node:GraphNode) -> Result<(),String>  {
        todo!()
    }

    fn rename_node(&mut self,old_id: &str,new_id: &str) -> Result<(),String>  {
        todo!()
    }

    fn get_node(&self,id: &str) -> Option<NetworkProcess<T> >  {
        todo!()
    }

    fn connect(&self) {
        todo!()
    }

    fn add_edge(&mut self,edge:GraphEdge,options:Option<Map<String,Value> > ,) ->  Result<Arc<Mutex<InternalSocket> > ,String> {
        todo!()
    }

    fn remove_edge(&mut self,node:GraphEdge) -> Result<(),String>  {
        todo!()
    }

    fn add_initial(&mut self,initializer:GraphIIP,options:Option<Map<String,Value> > ,) -> Result<Arc<Mutex<InternalSocket> > ,String>  {
        todo!()
    }

    fn remove_initial(&mut self,initializer:GraphIIP) -> Result<(),String> {
        todo!()
    }

    fn send_defaults(&self) -> Result<(),String> {
        todo!()
    }

    fn start(&self) -> Result<(),String>  {
        todo!()
    }

    fn stop(&self) -> Result<(),String>  {
        todo!()
    }

    fn get_debug(&self) -> bool {
        todo!()
    }

    fn set_debug(&mut self,active:bool) {
        todo!()
    }
}
