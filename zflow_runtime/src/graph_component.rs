use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    fmt::Debug,
    sync::{Arc, Mutex}
};

use fp_rust::{common::SubscriptionFunc, publisher::Publisher};
use serde::Deserialize;
use serde_json::Value;
use zflow::graph::{graph::Graph, types::GraphJson};

use crate::{
    component::{
        BaseComponentTrait, BracketContext, ComponentCallbacks, ComponentEvent, ComponentOptions,
        ComponentTrait,
    },
    ip::IPType,
    loader::ComponentLoader,
    network::{BaseNetwork, NetworkEvent, NetworkProcess},
    port::{InPort, InPorts, InPortsOptions, OutPort, OutPorts, OutPortsOptions, PortOptions},
    process::{ProcessFunc, ProcessResult},
    sockets::SocketEvent,
};

/// A ZFlow component that represents a subgraph
///
/// It is used to wrap ZFlow Networks into components inside
/// another network
#[derive(Clone)]
pub struct GraphComponent {
    pub loader: Option<ComponentLoader<Self>>,
    pub network: Option<Arc<Mutex<dyn BaseNetwork<Self> + Send + Sync>>>,
    pub metadata: HashMap<String, Value>,
    pub in_ports: InPorts,
    pub out_ports: OutPorts,
    /// Set the default component description
    pub description: String,
    /// Set the default component icon
    pub icon: String,
    pub ready: bool,
    /// Whether the component should keep send packets
    /// out in the order they were received
    pub ordered: bool,
    /// Whether the component should activate when it receives packets
    pub activate_on_input: bool,
    /// Bracket forwarding rules. By default we forward
    pub forward_brackets: HashMap<String, Vec<String>>,
    pub bracket_context: BracketContext<Self>,
    pub component_name: Option<String>,
    pub node_id: String,
    /// Queue for handling ordered output packets
    pub output_q: VecDeque<Arc<Mutex<ProcessResult<Self>>>>,
    pub base_dir: String,
    /// Initially the component is not started
    pub started: bool,
    pub starting: bool,
    pub load: usize,
    pub(crate) handle: Option<Arc<Mutex<ProcessFunc<Self>>>>,
    pub(crate) bus: Arc<Mutex<Publisher<ComponentEvent>>>,
    pub auto_ordering: bool,
    setup_fn: Option<Arc<Mutex<dyn FnMut() -> Result<(), String> + Send + Sync + 'static>>>,
    teardown_fn: Option<Arc<Mutex<dyn FnMut() -> Result<(), String> + Send + Sync + 'static>>>,
    tracked_signals: Vec<Arc<SubscriptionFunc<ComponentEvent>>>,
}

impl BaseComponentTrait for GraphComponent {
    type Handle = Arc<Mutex<ProcessFunc<Self>>>;
    type Comp = GraphComponent;

    fn set_started(&mut self, started: bool) {
        self.started = started;
    }

    fn is_started(&self) -> bool {
        self.started
    }
    fn is_ordered(&self) -> bool {
        if self.ordered || self.auto_ordering {
            return true;
        };
        return false;
    }
    fn is_subgraph(&self) -> bool {
        true
    }
    fn is_ready(&self) -> bool {
        return self.ready;
    }
    fn get_description(&self) -> String {
        self.description.clone()
    }

    fn get_name(&self) -> Option<String> {
        self.component_name.clone()
    }

    fn get_node_id(&self) -> String {
        self.node_id.clone()
    }

    fn set_node_id(&mut self, id: String) {
        self.node_id = id;
    }

    fn get_handle(&self) -> Option<Self::Handle> {
        self.handle.clone()
    }

    fn set_handle(&mut self, handle: Box<ProcessFunc<Self::Comp>>) {
        self.handle = Some(Arc::new(Mutex::new(handle)));
    }

    fn get_inports(&self) -> InPorts {
        self.in_ports.clone()
    }

    fn get_inports_mut(&mut self) -> &mut InPorts {
        &mut self.in_ports
    }

    fn get_outports(&self) -> OutPorts {
        self.out_ports.clone()
    }

    fn get_outports_mut(&mut self) -> &mut OutPorts {
        &mut self.out_ports
    }

    fn get_auto_ordering(&self) -> bool {
        self.auto_ordering
    }

    fn set_auto_ordering(&mut self, auto_ordering: bool) {
        self.auto_ordering = auto_ordering;
    }

    fn get_ordered(&self) -> bool {
        self.ordered
    }

    fn get_output_queue(&self) -> VecDeque<Arc<Mutex<ProcessResult<Self>>>> {
        self.output_q.clone()
    }

    fn get_output_queue_mut(&mut self) -> &mut VecDeque<Arc<Mutex<ProcessResult<Self>>>> {
        &mut self.output_q
    }

    fn get_load(&self) -> usize {
        self.load
    }

    fn set_load(&mut self, load: usize) {
        self.load = load
    }

    fn get_icon(&self) -> String {
        self.icon.clone()
    }

    fn get_teardown_function(
        &self,
    ) -> Option<Arc<Mutex<dyn FnMut() -> Result<(), String> + Send + Sync + 'static>>> {
        self.teardown_fn.clone()
    }

    fn get_setup_function(
        &self,
    ) -> Option<Arc<Mutex<dyn FnMut() -> Result<(), String> + Send + Sync + 'static>>> {
        self.setup_fn.clone()
    }

    fn get_subscribers(&self) -> Vec<Arc<SubscriptionFunc<ComponentEvent>>> {
        self.tracked_signals.clone()
    }

    fn get_subscribers_mut(&mut self) -> &mut Vec<Arc<SubscriptionFunc<ComponentEvent>>> {
        &mut self.tracked_signals
    }

    fn get_publisher(&self) -> Arc<Mutex<Publisher<ComponentEvent>>> {
        self.bus.clone()
    }

    fn clear_bracket_context(&mut self) {
        self.bracket_context.r#in.clear();
        self.bracket_context.out.clear();
    }

    fn set_icon(&mut self, icon: String) {
        self.icon = icon;
    }

    fn get_bracket_context_val(&self) -> BracketContext<Self::Comp> {
        self.bracket_context.clone()
    }
    fn get_bracket_context_val_mut(&mut self) -> &mut BracketContext<Self::Comp> {
        &mut self.bracket_context
    }

    fn get_forward_brackets(&self) -> HashMap<String, Vec<String>> {
        self.forward_brackets.clone()
    }

    fn get_forward_brackets_mut(&mut self) -> &mut HashMap<String, Vec<String>> {
        &mut self.forward_brackets
    }

    fn get_base_dir(&self) -> String {
        self.base_dir.clone()
    }

    fn set_base_dir(&mut self, dir: String) {
        self.base_dir = dir;
    }
    fn set_name(&mut self, name: String) {
        self.component_name = Some(name);
    }

    fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
        self.get_publisher()
            .try_lock()
            .expect("expected publisher")
            .publish(ComponentEvent::Ready);
    }

    fn set_loader(&mut self, loader: ComponentLoader<Self::Comp>) {
        self.loader = Some(loader);
    }

    fn set_description(&mut self, desc: String) {
        self.description = desc;
    }
}

impl ComponentCallbacks for GraphComponent {
    /// ### Setup
    /// Provide the setUp function to be called for component-specific initialization.
    /// Called at network start-up.
    fn setup(&mut self, setup_fn: impl FnMut() -> Result<(), String> + Send + Sync + 'static) {
        // self.setup_fn = Some(Arc::new(Mutex::new(setup_fn)));
    }

    /// ### Teardown
    /// Provide the setUp function to be called for component-specific cleanup.
    /// Called at network shutdown.
    fn teardown(
        &mut self,
        teardown_fn: impl FnMut() -> Result<(), String> + Send + Sync + 'static,
    ) {
        // self.teardown_fn = Some(Arc::new(Mutex::new(teardown_fn)));
    }

    /// Subscribe to component's events
    fn on(
        &self,
        callback: impl FnMut(Arc<ComponentEvent>) -> () + Send + Sync + 'static,
    ) -> Arc<SubscriptionFunc<ComponentEvent>> {
        self.bus.clone().try_lock().unwrap().subscribe_fn(callback)
    }
}

impl ComponentTrait for GraphComponent {}

impl Default for GraphComponent {
    fn default() -> Self {
        Self {
            network: None,
            metadata: Default::default(),
            in_ports: InPorts::new(InPortsOptions {
                ports: HashMap::from([(
                    "graph".to_string(),
                    InPort::new(PortOptions {
                        description:
                            "ZFlow graph definition to be used with the subgraph component"
                                .to_owned(),
                        required: true,
                        ..PortOptions::default()
                    }),
                )]),
                ..InPortsOptions::default()
            }),
            out_ports: Default::default(),
            description: Default::default(),
            icon: Default::default(),
            ordered: Default::default(),
            activate_on_input: Default::default(),
            forward_brackets: Default::default(),
            bracket_context: Default::default(),
            component_name: Default::default(),
            node_id: Default::default(),
            output_q: Default::default(),
            base_dir: Default::default(),
            started: Default::default(),
            load: 0,
            handle: None,
            bus: Default::default(),
            auto_ordering: Default::default(),
            setup_fn: Default::default(),
            teardown_fn: Default::default(),
            tracked_signals: Default::default(),
            ready: true,
            starting: false,
            loader: None,
        }
    }
}

impl Debug for GraphComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphComponent")
            .field("metadata", &self.metadata)
            .field("in_ports", &self.in_ports)
            .field("out_ports", &self.out_ports)
            .field("description", &self.description)
            .field("icon", &self.icon)
            .field("ordered", &self.ordered)
            .field("activate_on_input", &self.activate_on_input)
            .field("forward_brackets", &self.forward_brackets)
            .field("bracket_context", &self.bracket_context)
            .field("component_name", &self.component_name)
            .field("node_id", &self.node_id)
            .field("output_q", &self.output_q)
            .field("base_dir", &self.base_dir)
            .field("started", &self.started)
            .field("load", &self.load)
            .field("ready", &self.ready)
            .field("starting", &self.starting)
            .field("handle", &"[process_func]")
            .field("bus", &"[signal]")
            .field("auto_ordering", &self.auto_ordering)
            .field("setup_fn", &"[setup_fn]")
            .field("teardown_fn", &"[teardown_fn]")
            // .field("tracked_signals", &self.tracked_signals)
            .finish()
    }
}

impl GraphComponent {
    pub fn new(
        metadata: Option<HashMap<String, Value>>,
        options: Option<ComponentOptions<Self>>,
    ) -> Arc<Mutex<Self>> {
        let mut s = Self::default();

        if let Some(metadata) = metadata {
            s.metadata = metadata;
        }

        if let Some(options) = &options {
            options.in_ports.iter().for_each(|(name, port)| {
                s.in_ports.ports.insert(name.to_owned(), port.to_owned());
            });
            s.out_ports = OutPorts::new(OutPortsOptions {
                ports: options.out_ports.clone(),
            });
        }

        let _s = Arc::new(Mutex::new(s.clone()));

        if let Ok(s) = _s.clone().try_lock().as_mut() {
            s.in_ports.ports.get_mut("graph").map(|graph_port| {
                let _s = _s.clone();
                graph_port.on(move |event| {
                    if let SocketEvent::IP(ip, _) = event.as_ref() {
                        if let IPType::Data(graph) = &ip.datatype {
                            if let Ok(graph_json) = GraphJson::deserialize(graph) {
                                let _ = Self::setup_graph(_s.clone(), &graph_json);
                            }
                        }
                    }
                });
            });
            let __s = _s.clone();
            s.setup_fn = Some(Arc::new(Mutex::new(move ||{
                return GraphComponent::set_up(__s.clone())
            })));
            let __s = _s.clone();
            s.teardown_fn = Some(Arc::new(Mutex::new(move ||{
                return GraphComponent::tear_down(__s.clone())
            })));
        }
        _s.clone()
    }

    pub fn setup_graph(
        component: Arc<Mutex<impl ComponentTrait>>,
        graph: &dyn Any,
    ) -> Result<(), String> {
        if let Ok(component) = component.clone().try_lock().as_mut() {
            component.set_ready(false);
        }

        if let Some(graph) = graph.downcast_ref::<Graph>() {
            GraphComponent::create_network(component.clone(), graph.clone())?;
        }
        if let Some(graph_json) = graph.downcast_ref::<GraphJson>() {
            GraphComponent::create_network(
                component.clone(),
                Graph::from_json(graph_json.clone(), None),
            )?;
        }
        if let Some(graph_json_string) = graph.downcast_ref::<&str>() {
            let mut ss = graph_json_string.chars().clone();
            if ss.next() != Some('/') && ss.next() != Some(':') {
                GraphComponent::create_network(
                    component.clone(),
                    Graph::load_file(*graph_json_string, None)
                        .expect("expected to load Graph from path"),
                )?;
                return Ok(());
            }
            GraphComponent::create_network(
                component.clone(),
                Graph::from_json_string(*graph_json_string, None)
                    .expect("expected to convert json string to Graph"),
            )?;
        }

        Err(format!(
            "Could not setup graph component with data {:?}",
            graph
        ))
    }

    pub fn create_network(
        component: Arc<Mutex<impl ComponentTrait>>,
        mut graph: Graph,
    ) -> Result<(), String> {
        if let Ok(component) = component.clone().try_lock().as_mut() {
            if let Some(description) = graph.properties.get("description") {
                component.set_description(
                    description
                        .as_str()
                        .expect("expected description string")
                        .to_owned(),
                );
            }

            if let Some(icon) = graph.properties.get("icon") {
                component.set_icon(icon.as_str().expect("expected icon string").to_owned())
            }

            if graph.name.is_empty() && !component.get_node_id().is_empty() {
                graph.name = component.get_node_id();
            }
        }

        // Todo: call create-network factory

        Ok(())
    }

    pub fn subscribe_network(
        component: Arc<Mutex<impl ComponentTrait>>,
        network: Arc<Mutex<dyn BaseNetwork<Self> + Send + Sync>>,
    ) {
        if let Ok(network) = network.clone().try_lock() {
            network
                .get_publisher()
                .try_lock()
                .expect("expected network publisher")
                .subscribe_fn(move |event| {
                    if let Ok(component) = component.clone().try_lock().as_mut() {
                        match event.as_ref() {
                            NetworkEvent::Start => {
                                // activate
                            }
                            NetworkEvent::End => {
                                // deactivate
                            }
                            _ => {}
                        }
                    }
                });
        }
    }

    pub fn is_exported_inport(
        &mut self,
        port: InPort,
        name: &str,
        port_name: &str,
    ) -> Option<String> {
        if self.network.is_none() {
            return None;
        }
        // First we check disambiguated exported ports
        if let Ok(network) = self.network.clone().unwrap().try_lock().as_mut() {
            for key in network.get_graph().inports.keys() {
                if let Some(priv_port) = network.get_graph().inports.get(key) {
                    if priv_port.process == name && priv_port.port == port_name {
                        return Some(key.clone());
                    }
                }
            }
            
        }
        // Component has exported ports and this isn't one of them
        None
    }

    pub fn is_exported_outport(
        &mut self,
        port: OutPort,
        name: &str,
        port_name: &str,
    ) -> Option<String> {
        if self.network.is_none() {
            return None;
        }
        // First we check disambiguated exported ports
        if let Ok(network) = self.network.clone().unwrap().try_lock().as_mut() {
            for key in network.get_graph().outports.keys() {
                if let Some(priv_port) = network.get_graph().outports.get(key) {
                    if priv_port.process == name && priv_port.port == port_name {
                        return Some(key.clone());
                    }
                }
            }
        }
        // Component has exported ports and this isn't one of them
        None
    }

    pub fn find_edge_ports(
        this_graph: Arc<Mutex<Self>>,
        name: &str,
        process: NetworkProcess<impl ComponentTrait>,
    ) -> bool {
        let binding = process.component.clone();
        let process_component = binding
            .try_lock()
            .expect("expected process component instance");

        let in_ports = process_component.get_inports().ports.clone();
        let out_ports = process_component.get_outports().ports.clone();
        let _this_graph = this_graph.clone();
        in_ports.iter().for_each(|(port_name, port)| {
            let binding = _this_graph.clone();
            let mut binding = binding.try_lock();
            let graph_component = binding
                .as_mut()
                .expect("expected process component instance");
            if let Some(target_port_name) =
                graph_component.is_exported_inport(port.clone(), name, port_name)
            {
                graph_component
                    .in_ports
                    .ports
                    .insert(target_port_name.clone(), port.clone());

                let binding = this_graph.clone();
                graph_component
                    .in_ports
                    .ports
                    .get_mut(&target_port_name)
                    .unwrap()
                    .on(move |event| {
                        let p_binding = binding.clone();
                        let mut binding = binding.try_lock();
                        match event.as_ref() {
                            SocketEvent::Connect(_) => {
                                if let Ok(graph_component) = binding.as_mut() {
                                    // Start the network implicitly if we're starting to get data
                                    if graph_component.starting || graph_component.network.is_none()
                                    {
                                        return;
                                    }
                                    if let Ok(network) =
                                        graph_component.network.clone().unwrap().try_lock().as_mut()
                                    {
                                        if network.is_started() {
                                            return;
                                        }

                                        if network.get_startup_time().is_some() {
                                            // Network was started, but did finish. Re-start simply
                                            network.set_started(true);
                                            return;
                                        }

                                        // Network was never started, start properly
                                        let _ = GraphComponent::set_up(p_binding.clone());
                                    }
                                }
                            }
                            _ => {}
                        }
                    });
            }
        });

        let _this_graph = this_graph.clone();
        out_ports.clone().iter().for_each(|(port_name, port)| {
            let binding = _this_graph.clone();
            let mut binding = binding.try_lock();
            let graph_component = binding
                .as_mut()
                .expect("expected process component instance");
            if let Some(target_port_name) =
                graph_component.is_exported_outport(port.clone(), name, port_name)
            {
                graph_component
                    .out_ports
                    .ports
                    .insert(target_port_name.clone(), port.clone());
            }
        });
        return true;
    }

    fn set_up(component: Arc<Mutex<Self>>) -> Result<(), String> {
        let binding = component.clone();
        let mut binding = binding.try_lock();
        let _component = binding.as_mut().unwrap();
        _component.starting = true;

        if !_component.is_ready() {
            if let Ok(publisher) = _component.bus.clone().try_lock().as_mut() {
                publisher.subscribe_fn(move |event| {
                    let mut binding = component.try_lock();
                    let _component = binding.as_mut().unwrap();
                    match event.as_ref() {
                        ComponentEvent::Ready => if !_component.is_ready() {
                            GraphComponent::set_up(component.clone()).expect("expected graph component to setup");
                        },
                        _ => {}
                    }
                });
            }
        }
        if _component.network.is_none() {
            return Ok(())
        }

        if let Ok(network) = _component.network.clone().unwrap().try_lock() {
            network.start()?;
            _component.starting = false;
        }

        Ok(())
    }

    pub fn tear_down(component: Arc<Mutex<Self>>)-> Result<(), String> {
        let binding = component.clone();
        let mut binding = binding.try_lock();
        let _component = binding.as_mut().unwrap();
        if _component.network.is_none() {
            return Ok(())
        }
        if let Ok(network) = _component.network.clone().unwrap().try_lock() {
            return network.stop();
        }

        Err("Faled to teardown graph component".to_owned())
    }
}
