use std::{
    any::Any,
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use async_trait::async_trait;
use serde_json::{Map, Value};
use zflow::graph::{
    graph::Graph,
    types::{GraphEdge, GraphIIP, GraphNode},
};

use crate::{
    component::{BaseComponentTrait, ComponentTrait},
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

// ## The ZFlow Network
//
// ZFlow networks consist of processes connected to each other
// via sockets attached from outports to inports.
//
// The role of the network coordinator is to take a graph and
// instantiate all the necessary processes from the designated
// components, attach sockets between them, and handle the sending
// of Initial Information Packets.
#[async_trait]
pub trait BaseNetwork
where
    Self::ComponentType: ComponentTrait,
{
    type Network;
    type ComponentType;

    fn is_started(&self) -> bool;
    fn is_stopped(&self) -> bool;
    fn is_running(&self) -> bool {
        self.get_active_processes().len() > 0
    }
    fn set_started(&mut self, started: bool);

    /// Processes contains all the instantiated components for this network
    fn get_processes(&self) -> HashMap<String, NetworkProcess<Self::ComponentType>>;
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
        component: Arc<Mutex<Self::ComponentType>>,
        metadata: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<Self::ComponentType>>, String> {
        Err("unimplemented".to_string())
    }
    /// ## Loading components
    /// from path
    fn load_from_path(
        &mut self,
        component: &Path,
        metadata: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<Self::ComponentType>>, String> {
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
    async fn add_node(
        &mut self,
        node: GraphNode,
        options: Option<Map<String, Value>>,
    ) -> Result<NetworkProcess<Self::ComponentType>, String>;

    /// Remove node from network
    async fn remove_node(&mut self, node: GraphNode) -> Result<(), String>;
    /// Rename a node in the  network
    async fn rename_node(&mut self, old_id: &str, new_id: &str) -> Result<(), String>;

    /// Get process by its ID.
    fn get_node(&self, id: &str) -> Option<NetworkProcess<Self::ComponentType>>;

    async fn connect(&self);

    /// Add edge
    async fn add_edge(
        &mut self,
        edge: GraphEdge,
        options: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String>;

    /// Remove edge
    async fn remove_edge(&mut self, node: GraphEdge) -> Result<(), String>;

    /// Add initial packets
    async fn add_initial(
        &mut self,
        initializer: GraphIIP,
        options: Option<Map<String, Value>>,
    ) -> Result<Arc<Mutex<InternalSocket>>, String>;

    /// Remove initial packets
    async fn remove_initial(&mut self, initializer: GraphIIP) -> Result<(), String>;

    async fn send_defaults(&self) -> Result<(), String>;

    fn start(&self) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;

    fn get_debug(&self) -> bool;
    fn set_debug(&mut self, active: bool);
}

pub trait NetworkSubsciption<T: BaseNetwork> {
    fn subscribe_subgraph(
        network: Arc<Mutex<T>>,
        node: NetworkProcess<T::ComponentType>,
    ) -> Result<(), String>;

    /// Subscribe to events from all connected sockets and re-emit them
    fn subscribe_socket(
        network: Arc<Mutex<T>>,
        socket: Arc<Mutex<InternalSocket>>,
        source: NetworkProcess<T::ComponentType>,
    ) -> Result<(), String>;

    fn subscribe_node(
        network: Arc<Mutex<T>>,
        node: NetworkProcess<T::ComponentType>,
    ) -> Result<(), String>;
}

pub struct Network<T: ComponentTrait> {
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
    // pub loader:ComponentLoader
}

impl<T> BaseNetwork for Network<T>
where
    T: ComponentTrait,
{
    type Network = Self;
    type ComponentType = T;

    fn is_started(&self) -> bool {
        todo!()
    }

    fn is_stopped(&self) -> bool {
        todo!()
    }

    fn set_started(&mut self, started: bool) {
        todo!()
    }

    fn get_processes(&self) -> HashMap<String, NetworkProcess<Self::ComponentType>> {
        todo!()
    }

    fn get_startup_time(&self) -> Option<SystemTime> {
        todo!()
    }

    fn add_node<'life0, 'async_trait>(
        &'life0 mut self,
        node: GraphNode,
        options: Option<Map<String, Value>>,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = Result<NetworkProcess<Self::ComponentType>, String>>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn remove_node<'life0, 'async_trait>(
        &'life0 mut self,
        node: GraphNode,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = Result<(), String>>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn rename_node<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 mut self,
        old_id: &'life1 str,
        new_id: &'life2 str,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = Result<(), String>>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn get_node(&self, id: &str) -> Option<NetworkProcess<Self::ComponentType>> {
        todo!()
    }

    fn connect<'life0, 'async_trait>(
        &'life0 self,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = ()> + core::marker::Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn add_edge<'life0, 'async_trait>(
        &'life0 mut self,
        edge: GraphEdge,
        options: Option<Map<String, Value>>,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = Result<Arc<Mutex<InternalSocket>>, String>>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn remove_edge<'life0, 'async_trait>(
        &'life0 mut self,
        node: GraphEdge,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = Result<(), String>>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn add_initial<'life0, 'async_trait>(
        &'life0 mut self,
        initializer: GraphIIP,
        options: Option<Map<String, Value>>,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = Result<Arc<Mutex<InternalSocket>>, String>>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn remove_initial<'life0, 'async_trait>(
        &'life0 mut self,
        initializer: GraphIIP,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = Result<(), String>>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn send_defaults<'life0, 'async_trait>(
        &'life0 self,
    ) -> core::pin::Pin<
        Box<
            dyn core::future::Future<Output = Result<(), String>>
                + core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn start(&self) -> Result<(), String> {
        todo!()
    }

    fn stop(&self) -> Result<(), String> {
        todo!()
    }

    fn get_debug(&self) -> bool {
        todo!()
    }

    fn set_debug(&mut self, active: bool) {
        todo!()
    }
}
