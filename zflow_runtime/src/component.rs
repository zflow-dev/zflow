use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    fmt::Debug,
    sync::{Arc, Mutex},
};

use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
use dyn_clone::DynClone;
use fp_rust::{
    common::SubscriptionFunc,
    handler::{Handler, HandlerThread},
    publisher::Publisher,
};

use log::{debug, log, Level};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use zflow_graph::types::GraphJson;
use zflow_graph::Graph;

use crate::{
    ip::{IPOptions, IPType, IP},
    // loader::ComponentLoader,
    network::{BaseNetwork, Network, NetworkEvent, NetworkProcess},
    port::{
        normalize_port_name, BasePort, InPort, InPorts, InPortsOptions, OutPort, OutPorts,
        OutPortsOptions, PortsTrait,
    },
    process::{
        ProcessContext, ProcessError, ProcessFunc, ProcessHandle, ProcessInput, ProcessOutput,
        ProcessResult,
    },
    // registry::RemoteComponent,
    sockets::SocketEvent,
};

#[derive(Debug, Clone, Default)]
pub struct BracketContext {
    pub r#in: HashMap<String, HashMap<String, Vec<Arc<Mutex<ProcessContext>>>>>,
    pub out: HashMap<String, HashMap<String, Vec<Arc<Mutex<ProcessContext>>>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentEvent {
    Activate(usize),
    Deactivate(usize),
    Start,
    Ready,
    End,
    Icon(Value),
}

pub trait ModuleComponent: DynClone + Send + Sync + 'static {
    fn as_component(&self) -> Result<Component, String>;
}

pub trait GraphDefinition: DynClone + Send + Sync + Debug + 'static {
    fn to_any(&self) -> &dyn Any;
}

impl GraphDefinition for Component {
    fn to_any(&self) -> &dyn Any {
        Box::leak(Box::new(self.clone())) as &dyn Any
    }
}

// impl GraphDefinition for RemoteComponent {
//     fn to_any(&self) -> &dyn Any {
//         Box::leak(Box::new(self.clone())) as &dyn Any
//     }
// }

impl GraphDefinition for Graph {
    fn to_any(&self) -> &dyn Any {
        Box::leak(Box::new(self.clone())) as &dyn Any
    }
}
impl GraphDefinition for GraphJson {
    fn to_any(&self) -> &dyn Any {
        Box::leak(Box::new(self.clone())) as &dyn Any
    }
}

impl GraphDefinition for String {
    fn to_any(&self) -> &dyn Any {
        Box::leak(Box::new(self.clone())) as &dyn Any
    }
}

/// ZFlow Process Component
// #[derive(Clone)]
pub struct Component {
    // pub loader: Option<ComponentLoader>,
    pub network: Option<*mut Network>, //Option<*mut (dyn BaseNetwork + Send + Sync + 'static)>,
    pub in_ports: InPorts,
    pub out_ports: OutPorts,
    /// Set the default component description
    pub description: String,
    /// Set the default component icon
    pub icon: String,
    /// Whether the component should keep send packets
    /// out in the order they were received
    pub ordered: bool,
    /// Whether the component should activate when it receives packets
    pub activate_on_input: bool,
    /// Bracket forwarding rules. By default we forward
    pub forward_brackets: HashMap<String, Vec<String>>,
    pub bracket_context: BracketContext,
    pub component_name: Option<String>,
    pub node_id: String,
    /// Queue for handling ordered output packets
    pub output_q: VecDeque<Arc<Mutex<ProcessResult>>>,
    pub base_dir: String,
    /// Initially the component is not started
    pub started: bool,
    pub starting: bool,
    pub load: usize,
    pub(crate) handle: Option<Arc<Mutex<ProcessFunc>>>,
    pub(crate) bus: Arc<Mutex<Publisher<ComponentEvent>>>,
    pub(crate) internal_thread: Arc<Mutex<HandlerThread>>,
    pub auto_ordering: Option<bool>,
    setup_fn:
        Option<Arc<Mutex<dyn FnMut(*mut Self) -> Result<(), String> + Send + Sync + 'static>>>,
    teardown_fn:
        Option<Arc<Mutex<dyn FnMut(*mut Self) -> Result<(), String> + Send + Sync + 'static>>>,
    tracked_signals: Vec<Arc<SubscriptionFunc<ComponentEvent>>>,
    pub(crate) graph: Option<Box<dyn GraphDefinition>>,
    pub(crate) ready: bool,
    pub metadata: Option<Map<String, Value>>,
}

impl Clone for Component {
    fn clone(&self) -> Self {
        let mut s = Self {
            // loader: self.loader.clone(),
            network: self.network.clone(),
            in_ports: self.in_ports.clone(),
            out_ports: self.out_ports.clone(),
            description: self.description.clone(),
            icon: self.icon.clone(),
            ordered: self.ordered.clone(),
            activate_on_input: self.activate_on_input.clone(),
            forward_brackets: self.forward_brackets.clone(),
            bracket_context: self.bracket_context.clone(),
            component_name: self.component_name.clone(),
            node_id: self.node_id.clone(),
            output_q: self.output_q.clone(),
            base_dir: self.base_dir.clone(),
            started: self.started.clone(),
            starting: self.starting.clone(),
            load: self.load.clone(),
            handle: self.handle.clone(),
            bus: self.bus.clone(),
            internal_thread: self.internal_thread.clone(),
            auto_ordering: self.auto_ordering.clone(),
            setup_fn: self.setup_fn.clone(),
            teardown_fn: self.teardown_fn.clone(),
            tracked_signals: self.tracked_signals.clone(),
            graph: None,
            ready: self.ready.clone(),
            metadata: self.metadata.clone(),
        };

        if let Some(graph) = &self.graph {
            s.graph = Some(dyn_clone::clone_box(&**graph));
        }

        s
    }
}

unsafe impl Send for Component {}
unsafe impl Sync for Component {}

impl Default for Component {
    fn default() -> Self {
        Self {
            in_ports: Default::default(),
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
            load: Default::default(),
            handle: None,
            bus: Default::default(),
            internal_thread: Arc::new(Mutex::new(HandlerThread::default())),
            auto_ordering: Default::default(),
            setup_fn: Default::default(),
            teardown_fn: Default::default(),
            tracked_signals: Default::default(),
            graph: Default::default(),
            ready: false,
            network: Default::default(),
            // loader: Default::default(),
            starting: Default::default(),
            metadata: Default::default(),
        }
    }
}

impl Debug for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Component")
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
            .field("handle", &"[process_func]")
            .field("bus", &"[signal]")
            .field("auto_ordering", &self.auto_ordering)
            .field("setup_fn", &"[setup_fn]")
            .field("teardown_fn", &"[teardown_fn]")
            // .field("tracked_signals", &self.tracked_signals)
            .finish()
    }
}

impl Component {
    pub fn to_static(&mut self) -> &'static Self {
        Box::leak(Box::new(self.clone()))
    }
    pub fn new(options: ComponentOptions) -> Self {
        let bus_handle = HandlerThread::new_with_mutex();

        let mut s = Self {
            out_ports: OutPorts::new(OutPortsOptions {
                ports: options.out_ports,
                ..OutPortsOptions::default()
            }),
            in_ports: InPorts::new(InPortsOptions {
                ports: options.in_ports,
                ..InPortsOptions::default()
            }),
            description: options.description,
            icon: options.icon,
            ordered: options.ordered,
            activate_on_input: options.activate_on_input,
            forward_brackets: options.forward_brackets,
            component_name: None,
            output_q: VecDeque::new(),
            bracket_context: BracketContext::default(),
            base_dir: "/".to_owned(),
            started: false,
            load: 0,
            handle: None,
            auto_ordering: None,
            node_id: String::from(""),
            bus: Arc::new(Mutex::new(Publisher::new())),
            internal_thread: bus_handle.clone(),
            setup_fn: None,
            teardown_fn: None,
            tracked_signals: Vec::new(),
            graph: options.graph,
            ready: false,
            network: None,
            starting: false,
            metadata: options.metadata,
        };

        if let Some(process) = options.process {
            s.handle = Some(Arc::new(Mutex::new(Box::leak(process))));
        }

        s
    }
    pub(crate) fn from_instance(component: Component) -> Arc<Mutex<Self>> {
        let component = Arc::new(Mutex::new(component));
        let _ = Self::link_process(component.clone());
        let has_graph = component.clone().try_lock().unwrap().graph.is_some();
        if has_graph {
            Component::init_graph(component.clone());
        }
        component.clone()
    }
    pub fn init(options: ComponentOptions) -> Arc<Mutex<Self>> {
        let component = Arc::new(Mutex::new(Self::new(options)));
        let _ = Self::link_process(component.clone());
        let has_graph = component.clone().try_lock().unwrap().graph.is_some();
        if has_graph {
            Component::init_graph(component.clone());
        }
        component.clone()
    }

    pub(crate) fn init_graph(component: Arc<Mutex<Self>>) {
        component
            .clone()
            .try_lock()
            .as_mut()
            .unwrap()
            .in_ports
            .ports
            .get_mut("graph")
            .map(|graph_port| {
                let _s = component.clone();
                graph_port.on(move |event| {
                    if let SocketEvent::IP(ip, _) = event.as_ref() {
                        if let IPType::Data(graph) = &ip.datatype {
                            if let Ok(graph_json) = GraphJson::deserialize(graph) {
                                let _ = Self::setup_graph(_s.clone(), Some(&graph_json));
                            }
                        }
                    }
                });
            });

        let _component = component.clone();
        component.clone().try_lock().as_mut().unwrap().setup_fn =
            Some(Arc::new(Mutex::new(move |this| {
                let _s = _component.clone();
                if let Ok(s) = _s.try_lock().as_mut() {
                    let setup_f = s.setup_fn.as_mut();
                    if let Some(setup_fn) = setup_f.cloned() {
                        if let Ok(setup_fn) = setup_fn.clone().try_lock().as_mut() {
                            (setup_fn)(this)?;
                        }
                    }
                }

                let mut this = unsafe { this.read() };
                return this.graph_set_up();
            })));
        let _component = component.clone();
        component.clone().try_lock().as_mut().unwrap().teardown_fn =
            Some(Arc::new(Mutex::new(move |this| {
                let _s = _component.clone();
                if let Ok(s) = _s.try_lock().as_mut() {
                    let teardown_f = s.teardown_fn.as_mut();
                    if let Some(tear_down_fn) = teardown_f.cloned() {
                        if let Ok(tear_down_fn) = tear_down_fn.try_lock().as_mut() {
                            (tear_down_fn)(this)?;
                        }
                    }
                }
                let mut this = unsafe { this.read() };
                return this.graph_tear_down();
            })));
    }

    /// ### Start
    ///
    /// Called when network starts. This calls the setUp
    /// method and sets the component to a started state.
    pub fn start(&mut self) -> Result<(), String> {
        if self.is_started() {
            return Ok(());
        }
        self.set_started(true);
        self.get_publisher()
            .clone()
            .try_lock()
            .unwrap()
            .publish(ComponentEvent::Start);

        if let Some(setup_fn) = self.get_setup_function() {
            if let Ok(setup_fn) = setup_fn.clone().try_lock().as_mut() {
                (setup_fn)(self)?;
            }
        }
        Ok(())
    }

    /// ### Shutdown
    ///
    /// Called when network is shut down. This calls the
    /// teardown function and sets the component back to a
    /// non-started state.
    pub fn shutdown(&mut self) -> Result<(), String> {
        // Tell the component that it is time to shut down
        if let Some(teardown_fn) = self.get_teardown_function() {
            if let Ok(teardown_fn) = teardown_fn.try_lock().as_mut() {
                (teardown_fn)(self)?;
            }
        }

        // thread::spawn(move || {
        //     // if let Ok(component) = _component.clone().try_lock().as_mut() {
        //     //     while component.get_load() > 0
        //     //         || component
        //     //             .get_handler_thread()
        //     //             .try_lock()
        //     //             .unwrap()
        //     //             .is_alive()
        //     //     {
        //     //         // Some in-flight processes, wait for them to finish
        //     //     }
        //     //     component.get_subscribers().iter().for_each(|signal| {
        //     //         component
        //     //             .get_publisher()
        //     //             .clone()
        //     //             .try_lock()
        //     //             .unwrap()
        //     //             .unsubscribe(signal.clone());
        //     //     });
        //     //     component.reset();
        //     // }
        // });

        while self.get_load() > 0 || self.get_handler_thread().try_lock().unwrap().is_alive() {
            // Some in-flight processes, wait for them to finish
        }
        self.get_subscribers().iter().for_each(|signal| {
            self.get_publisher()
                .clone()
                .try_lock()
                .unwrap()
                .unsubscribe(signal.clone());
        });
        self.reset();

        Ok(())
    }

    /// Sets process handler function
    pub(crate) fn link_process(component: Arc<Mutex<Component>>) {
        let binding = component.clone();
        let mut binding = binding.try_lock();
        let _component = binding.as_mut().expect("expected component instance");

        _component.prepare_forwarding();
        _component.get_inports().ports.keys().for_each(move |name| {
            let port = _component.get_inports_mut().ports.get_mut(name).unwrap();
            if port.name.is_empty() {
                (*port).name = name.to_string();
            }

            let name = name.clone();
            let port = port.clone();
            let component = component.clone();
            _component
                .get_inports_mut()
                .ports
                .get_mut(&name)
                .map(|inport| {
                    let component = component.clone();
                    inport.on(move |event| match event.as_ref() {
                        SocketEvent::IP(ip, None) => {
                            Component::handle_ip(component.clone(), ip.clone(), port.clone());
                        }
                        _ => {}
                    });
                });
        });
    }

    /// ### Handling IP objects
    ///
    /// The component has received an Information Packet. Call the
    /// processing function so that firing pattern preconditions can
    /// be checked and component can do processing as needed.
    pub fn handle_ip(_component: Arc<Mutex<Self>>, ip: IP, mut port: InPort) {
        // Initialize the result object for situations where output needs
        // to be queued to be kept in order
        let mut result = Arc::new(Mutex::new(ProcessResult::default()));
        let mut inports = InPorts::default();
        let mut outports = OutPorts::default();
        let mut handle = None;
        let mut handler_thread = None;

        let mut node_id = "".to_owned();

        if let Ok(component) = _component.clone().try_lock().as_mut() {
            node_id = component.get_node_id();
            if !port.options.triggering {
                // If port is non-triggering, we can skip the process function call
                return;
            }
            // component_name = component.get_name();
            match ip.datatype {
                IPType::OpenBracket(_) => {
                    if component.get_auto_ordering().is_none() && !component.get_ordered() {
                        // Switch component to ordered mode when receiving a stream unless
                        // auto-ordering is disabled
                        debug!(
                            target: "Component::handle_ip",
                            "zflow::component => {} port '{}' entered auto-ordering mode",
                            component.get_node_id(),
                            port.clone().name
                        );
                        component.set_auto_ordering(true);
                    }
                }
                _ => {}
            }

            if component.is_forwarding_inport(&port.clone()) {
                // For bracket-forwarding inports we need to initialize a bracket context
                // so that brackets can be sent as part of the output, and closed after.
                match ip.datatype {
                    IPType::OpenBracket(_) => {
                        // For forwarding ports openBrackets don't fire
                        return;
                    }
                    IPType::CloseBracket(_) => {
                        // For forwarding ports closeBrackets don't fire
                        // However, we need to handle several different scenarios:
                        // A. There are closeBrackets in queue before current packet
                        // B. There are closeBrackets in queue after current packet
                        // C. We've queued the results from all in-flight processes and
                        //    new closeBracket arrives

                        if let Some(buf) = port.get_buffer(Some(ip.clone().scope), ip.index, false)
                        {
                            if let Ok(buf) = buf.clone().try_lock() {
                                let data_packets = buf
                                    .iter()
                                    .filter(|b| match b.datatype {
                                        IPType::Data(_) => true,
                                        _ => false,
                                    })
                                    .collect::<Vec<&IP>>();

                                if (component.get_output_queue().len() >= component.get_load())
                                    && data_packets.is_empty()
                                {
                                    if let Some(first) = buf.first() {
                                        if !assert_json_matches_no_panic(
                                            first,
                                            &json!(ip.clone()),
                                            Config::new(CompareMode::Strict),
                                        )
                                        .is_ok()
                                        {
                                            return;
                                        }
                                    }

                                    if port.clone().name.is_empty() {
                                        return;
                                    }
                                    let port_name = port.clone().name;
                                    // Remove from buffer
                                    if let Some(_bracket_ctx) = component
                                        .get_bracket_context(
                                            "in",
                                            port_name,
                                            ip.clone().scope,
                                            ip.clone().index,
                                        )
                                        .expect("expected bracket contexts")
                                        .pop()
                                        .as_mut()
                                    {
                                        if let Ok(bracket_ctx) = _bracket_ctx.try_lock().as_mut() {
                                            bracket_ctx.close_ip = Some(ip.clone());
                                            log!(
                                        Level::Debug,
                                        "zflow::component::bracket => {} closeBracket-C from '{}' to {:?}: '{:?}'",
                                        component.get_node_id(),
                                        bracket_ctx.source,
                                        bracket_ctx.ports,
                                        ip.datatype
                                    );

                                            result = Arc::new(Mutex::new(ProcessResult {
                                                resolved: true,
                                                bracket_closing_after: vec![_bracket_ctx.clone()],
                                                ..ProcessResult::default()
                                            }));
                                        }

                                        component.get_output_queue_mut().push_back(result.clone());
                                        component.process_output_queue();
                                    }
                                }
                                // Check if buffer contains data IPs. If it does, we want to allow
                                // firing
                                if !data_packets.is_empty() {
                                    return;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            // has_load = component.get_load() > 0;
            inports = component.get_inports();
            outports = component.get_outports();
            handle = component.get_handle();
            handler_thread = Some(component.get_handler_thread());
        }

        // We have to free the component's mutex, so that we can use it in the process function

        let context = Arc::new(Mutex::new(ProcessContext {
            ip: ip.clone(),
            result: result.clone(),
            ports: vec![port.clone().name],
            close_ip: None,
            source: "".to_owned(),
            data: Value::Null,
            scope: None,
            component: _component.clone(),
            activated: false,
            ..Default::default()
        }));
        if handle.is_none() {
            return;
        }

        // Call the processing function

        // Prepare the input/output pair
        let input = ProcessInput {
            in_ports: inports,
            context: context.clone(),
            component: _component.clone(),
            ip: ip.clone(),
            port: port.clone(),
            scope: None,
            result: result.clone(),
        };
        let output = ProcessOutput {
            out_ports: outports,
            context: context.clone(),
            component: _component.clone(),
            ip: ip.clone(),
            scope: None,
            result: result.clone(),
        };

        let _context = context.clone();
        if let Ok(handle_binding) = handle.unwrap().try_lock().as_mut() {
            let res = (handle_binding)(Arc::new(Mutex::new(ProcessHandle {
                context: _context.clone(),
                handler_thread: handler_thread.clone().unwrap(),
                input,
                output: output.clone(),
            })));
            if res.is_ok() {
                let data = res.ok().unwrap();

                if data.resolved {
                    if let Err(x) = output.clone().send_done(&data) {
                        output.clone().done(Some(&x));
                        let _component = _component.clone();
                        if let Ok(component) = _component.clone().try_lock().as_mut() {
                            component.deactivate(_context.clone());
                        }
                    }
                }
            } else {
                if res.clone().err().is_some() {
                    output.clone().done(Some(res.err().as_ref().unwrap()));
                } else {
                    output.clone().done(None);
                }
                let _component = _component.clone();
                if let Ok(component) = _component.clone().try_lock().as_mut() {
                    component.deactivate(_context.clone());
                }
            }
        }

        if let Ok(ctx) = context.clone().try_lock() {
            if ctx.activated {
                return;
            }
        }

        // If receiving an IP object didn't cause the component to
        // activate, log that input conditions were not met
        if port.clone().is_addressable() {
            log!(
                Level::Debug,
                "zflow::component => {} packet on '{}[{}]' didn't match preconditions: {:?}",
                node_id,
                port.clone().name,
                ip.clone().index.unwrap(),
                ip.clone().datatype
            );
            return;
        }
        log!(
            Level::Debug,
            "zflow::component => {} packet on '{}' didn't match preconditions: {:?}",
            node_id,
            port.clone().name,
            ip.clone().datatype
        );
    }

    /// ### Setup
    /// Provide the setUp function to be called for component-specific initialization.
    /// Called at network start-up.
    pub fn setup(
        &mut self,
        setup_fn: impl FnMut(*mut Self) -> Result<(), String> + Send + Sync + 'static,
    ) {
        self.setup_fn = Some(Arc::new(Mutex::new(setup_fn)));
    }

    /// ### Teardown
    /// Provide the setUp function to be called for component-specific cleanup.
    /// Called at network shutdown.
    pub fn teardown(
        &mut self,
        teardown_fn: impl FnMut(*mut Self) -> Result<(), String> + Send + Sync + 'static,
    ) {
        self.teardown_fn = Some(Arc::new(Mutex::new(teardown_fn)));
    }

    /// Subscribe to component's events
    pub fn on(&mut self, callback: impl FnMut(Arc<ComponentEvent>) -> () + Send + Sync + 'static) {
        let f = self.bus.clone().try_lock().unwrap().subscribe_fn(callback);
        self.tracked_signals.push(f.clone());
    }

    pub fn prepare_forwarding(&mut self) {
        self.get_forward_brackets().keys().for_each(|inport| {
            if let Some(out_ports) = self.get_forward_brackets().get(inport) {
                if !self.get_inports().ports.contains_key(inport) {
                    self.get_forward_brackets_mut().remove(inport);
                }

                let mut temp: Vec<String> = Vec::new();
                out_ports.iter().for_each(|outport| {
                    if self.get_outports().ports.contains_key(outport) {
                        temp.push(outport.clone());
                    }
                });
                if temp.is_empty() {
                    self.get_forward_brackets_mut().remove(inport);
                } else {
                    self.get_forward_brackets_mut().insert(inport.clone(), temp);
                }
            }
        });
    }

    pub fn is_forwarding_outport(&self, inport: &dyn Any, outport: &dyn Any) -> bool {
        let mut inport_name = None;
        let mut outport_name = None;
        if let Some(inport) = inport.downcast_ref::<String>() {
            inport_name = Some(inport.clone());
        }
        if let Some(inport) = inport.downcast_ref::<InPort>() {
            inport_name = Some((*inport).name.clone());
        }
        if let Some(outport) = outport.downcast_ref::<String>() {
            outport_name = Some(outport.clone());
        }
        if let Some(outport) = outport.downcast_ref::<OutPort>() {
            outport_name = Some((*outport).name.clone());
        }

        if inport_name.is_none() || outport_name.is_none() {
            return false;
        }

        if !self
            .get_forward_brackets()
            .contains_key(&inport_name.clone().unwrap())
        {
            return false;
        }
        if self.get_forward_brackets()[&inport_name.unwrap()].contains(&outport_name.unwrap()) {
            return true;
        }
        false
    }
    pub fn is_forwarding_inport(&self, port: &dyn Any) -> bool {
        let mut port_name = "";
        if let Some(port) = port.downcast_ref::<InPort>() {
            port_name = port.name.as_str();
        }
        if let Some(port) = port.downcast_ref::<String>() {
            port_name = port;
        }

        if !port_name.is_empty() && self.get_forward_brackets().contains_key(port_name) {
            return true;
        }

        return false;
    }

    /// Get the current bracket forwarding context for an IP object
    pub fn get_bracket_context(
        &mut self,
        _type: &str,
        port: String,
        scope: String,
        idx: Option<usize>,
    ) -> Option<&mut Vec<Arc<Mutex<ProcessContext>>>> {
        let normalized = normalize_port_name(port.clone());
        let mut name = normalized.name;
        let mut index = normalized.index;
        if idx.is_some() {
            index = Some(format!("{}", idx.unwrap()));
        }

        if _type == "in" {
            if self
                .get_inports()
                .ports
                .get(&name)
                .expect("expected to access port from InPorts by name")
                .is_addressable()
            {
                name = format!("{}[{}]", name, index.unwrap());
            } else {
                name = port.clone();
            };
        } else if _type == "out" {
            if self
                .get_outports()
                .get(&name)
                .expect("expected to access port from OutPorts by name")
                .is_addressable()
            {
                name = format!("{}[{}]", name, index.unwrap());
            } else {
                name = port.clone();
            };
        }

        // Ensure we have a bracket context for the current scope
        if _type == "in" {
            if !self
                .get_bracket_context_val()
                .r#in
                .contains_key(&name.clone())
            {
                self.get_bracket_context_val_mut()
                    .r#in
                    .insert(name.clone(), HashMap::new());
            }
            if let Some(obj) = self
                .get_bracket_context_val_mut()
                .r#in
                .get_mut(&name.clone())
            {
                if !obj.contains_key(&scope.clone()) {
                    (*obj).insert(scope.clone(), Vec::new());
                }
            }

            return self
                .get_bracket_context_val_mut()
                .r#in
                .get_mut(&name)
                .unwrap()
                .get_mut(&scope);
        } else if _type == "out" {
            if !self
                .get_bracket_context_val()
                .out
                .contains_key(&name.clone())
            {
                self.get_bracket_context_val_mut()
                    .out
                    .insert(name.clone(), HashMap::new());
            }
            if let Some(obj) = self
                .get_bracket_context_val_mut()
                .out
                .get_mut(&name.clone())
            {
                if !obj.contains_key(&scope.clone()) {
                    (*obj).insert(scope.clone(), Vec::new());
                }
            }

            return self
                .get_bracket_context_val_mut()
                .out
                .get_mut(&name)
                .unwrap()
                .get_mut(&scope);
        }
        None
    }
    /// Get contexts that can be forwarded with this in/outport
    /// pair.
    pub fn get_forwardable_contexts(
        &mut self,
        inport: String,
        outport: String,
        contexts: Vec<Arc<Mutex<ProcessContext>>>,
    ) -> Vec<Arc<Mutex<ProcessContext>>> {
        let normalized = normalize_port_name(outport.clone());
        let name = normalized.name;
        let index = normalized.index;

        let mut forwardable = vec![];

        contexts.iter().enumerate().for_each(|(idx, _ctx)| {
            // No forwarding to this outport
            if !self.is_forwarding_outport(&inport, &name) {
                return;
            }
            if let Ok(ctx) = _ctx.clone().try_lock() {
                // We have already forwarded this context to this outport
                if !ctx.ports.contains(&outport) {
                    return;
                }
                // See if we have already forwarded the same bracket from another
                // inport

                let mut out_context =
                    self.get_bracket_context("out", name.clone(), ctx.ip.scope.clone(), None);

                if index.is_some() {
                    out_context = self.get_bracket_context(
                        "out",
                        name.clone(),
                        ctx.ip.scope.clone(),
                        index.clone().unwrap().parse().ok(),
                    );
                }
                if let Some(out_context) = out_context {
                    if let Some(out_context) = out_context.get_mut(idx) {
                        match &ctx.ip.datatype {
                            IPType::All(data)
                            | IPType::OpenBracket(data)
                            | IPType::CloseBracket(data)
                            | IPType::Data(data) => {
                                match &out_context.clone().try_lock().unwrap().ip.datatype {
                                    IPType::All(data2)
                                    | IPType::OpenBracket(data2)
                                    | IPType::CloseBracket(data2)
                                    | IPType::Data(data2) => {
                                        if assert_json_matches_no_panic(
                                            data,
                                            data2,
                                            Config::new(CompareMode::Strict),
                                        )
                                        .is_ok()
                                            && out_context
                                                .clone()
                                                .try_lock()
                                                .unwrap()
                                                .ports
                                                .contains(&outport)
                                        {
                                            return;
                                        }
                                    }

                                    _ => {}
                                }
                            }

                            _ => {}
                        }
                    }
                }

                forwardable.push(_ctx.clone());
            }
        });

        forwardable
    }
    pub fn add_bracket_forwards(&mut self, result: Arc<Mutex<ProcessResult>>) {
        let result = result.clone();
        if !result
            .clone()
            .try_lock()
            .unwrap()
            .bracket_closing_before
            .is_empty()
        {
            result
                .clone()
                .try_lock().unwrap()
                .bracket_closing_before.clone()
                .iter()
                .for_each(|context| {
                    if let Ok(context) = context.clone().try_lock() {
                        log!(
                            Level::Debug,
                            "zflow::component::bracket => {} closeBracket-A from '{}' to {:?}: '{:?}'",
                            self.get_node_id(),
                            context.source.clone(),
                            context.ports.clone(),
                            context
                                .close_ip
                                .clone()
                                .expect("expected close IP")
                                .datatype
                        );
                        if context.ports.is_empty() {
                            return;
                        }
                        context.ports.iter().for_each(|port| {
                            let mut ip_clone = context
                                .close_ip
                                .clone()
                                .expect("expected close IP")
                                .fake_clone();
                            self.add_to_result(result.clone(), port.clone(), &mut ip_clone, true);
                            if let Some(contexts) =
                                self.get_bracket_context("out", port.clone(), ip_clone.scope, None)
                            {
                                (*contexts).pop();
                            }
                        });
                    }
                });
        }
        let binding = result.clone();
        if let Some(bracket_context) = &binding.try_lock().unwrap().bracket_context {
            // First see if there are any brackets to forward. We need to reverse
            // the keys so that they get added in correct order
            let mut reversed_ctx_keys = (*bracket_context)
                .clone()
                .into_keys()
                .collect::<Vec<String>>();
            reversed_ctx_keys.reverse();
            reversed_ctx_keys.iter().for_each(|inport| {
                if let Some(context) = (*bracket_context).get(inport) {
                    if (*context).is_empty() {
                        return;
                    }

                    result.clone().try_lock().unwrap().outputs.keys().for_each(|outport| {
                        let ips = result.clone().try_lock().unwrap().outputs.get(outport).unwrap().clone();
                        let mut datas: Vec<IP> = vec![];
                        let mut forwarded_opens: Vec<IP> = vec![];
                        let mut unforwarded: Vec<Arc<Mutex<ProcessContext>>> = vec![];
                        if self.get_outports().ports[outport].is_addressable() {
                            ips.keys().for_each(|idx| {
                                // Don't register indexes we're only sending brackets to
                                let idx_ips = ips.get(idx).unwrap();
                                datas = idx_ips
                                    .iter()
                                    .filter_map(|ip| match (*ip).datatype {
                                        IPType::Data(_) => Some(ip.clone()),
                                        _ => None,
                                    })
                                    .collect::<Vec<IP>>();
                                if datas.is_empty() {
                                    return;
                                }
                                let port_identifier = format!("{}[{}]", outport.clone(), idx);
                                unforwarded = self.get_forwardable_contexts(
                                    inport.clone(),
                                    outport.clone(),
                                    context.clone(),
                                );
                                if unforwarded.is_empty() {
                                    return;
                                }

                                forwarded_opens.clear();

                                unforwarded.iter().for_each(|_ctx| {
                                    if let Ok(ctx) = _ctx.clone().try_lock().as_mut() {
                                        log!(
                                            Level::Debug,
                                            "zflow::component::bracket => {} openBracket from '{}' to '{}': '{:?}'",
                                            self.get_node_id(),
                                            inport.clone(),
                                            port_identifier.clone(),
                                            ctx.ip.datatype
                                        );
                                        let mut ip_clone = ctx.ip.fake_clone();
                                        ip_clone.index = idx.parse::<usize>().ok();
                                        forwarded_opens.push(ip_clone.clone());
                                        ctx.ports.push(port_identifier.clone());
                                        if let Some(b_ctx) = self.get_bracket_context(
                                            "out",
                                            outport.clone(),
                                            ctx.ip.scope.clone(),
                                            ctx.ip.index,
                                        ) {
                                            b_ctx.push(_ctx.clone());
                                        }
                                    }
                                });
                                forwarded_opens.reverse();
                                forwarded_opens.iter_mut().for_each(|ip| {
                                    self.add_to_result(result.clone(), outport.clone(), ip, true);
                                });
                            });
                            return;
                        }
                        // Don't register ports we're only sending brackets to
                        datas = ips
                            .values()
                            .filter_map(|v| Some(v.clone()))
                            .collect::<Vec<Vec<IP>>>()
                            .iter()
                            .flatten()
                            .filter_map(|ip| match (*ip).datatype {
                                IPType::Data(_) => Some(ip.clone()),
                                _ => None,
                            })
                            .collect();
                        if datas.is_empty() {
                            return;
                        }

                        unforwarded = self.get_forwardable_contexts(
                            inport.clone(),
                            outport.clone(),
                            context.clone(),
                        );
                        if unforwarded.is_empty() {
                            return;
                        }
                        unforwarded.iter().for_each(|_ctx| {
                            if let Ok(ctx) = _ctx.clone().try_lock().as_mut() {
                                log!(
                                    Level::Debug,
                                    "zflow::component::bracket => {} openBracket from '{}' to '{}': '{:?}'",
                                    self.get_node_id(),
                                    inport.clone(),
                                    outport.clone(),
                                    ctx.ip.datatype
                                );

                                forwarded_opens.push(ctx.ip.clone());
                                ctx.ports.push(outport.clone());
                                if let Some(b_ctx) = self.get_bracket_context(
                                    "out",
                                    outport.clone(),
                                    ctx.ip.scope.clone(),
                                    None,
                                ) {
                                    b_ctx.push(_ctx.clone());
                                }
                            }
                        });
                        forwarded_opens.reverse();
                        forwarded_opens.iter_mut().for_each(|ip| {
                            self.add_to_result(result.clone(), outport.clone(), ip, true);
                        });
                    });
                }
            });
        }
        if !result
            .clone()
            .try_lock()
            .unwrap()
            .bracket_closing_after
            .is_empty()
        {
            result
                .clone()
                .try_lock()
                .unwrap()
                .bracket_closing_after
                .iter()
                .for_each(|context| {
                    if let Ok(context) = context.clone().try_lock() {
                        log!(
                        Level::Debug,
                        "zflow::component::bracket => {} closeBracket-B from '{}' to {:?}: '{:?}'",
                        self.get_node_id(),
                        context.source.clone(),
                        context.ports.clone(),
                        context
                            .close_ip
                            .clone()
                            .expect("expected close IP")
                            .datatype
                    );
                        if context.ports.is_empty() {
                            return;
                        }

                        context.ports.iter().for_each(|port| {
                            let mut ip_clone = context
                                .close_ip
                                .clone()
                                .expect("expected close IP")
                                .fake_clone();
                            self.add_to_result(result.clone(), port.clone(), &mut ip_clone, true);
                            if let Some(contexts) =
                                self.get_bracket_context("out", port.clone(), ip_clone.scope, None)
                            {
                                (*contexts).pop();
                            }
                        });
                    }
                });
        }
        result
            .clone()
            .try_lock()
            .unwrap()
            .bracket_closing_before
            .clear();
        result.clone().try_lock().unwrap().bracket_context = None;
        result
            .clone()
            .try_lock()
            .unwrap()
            .bracket_closing_after
            .clear();
    }
    /// Add an IP object to the list of results to be sent in
    /// order
    pub fn add_to_result(
        &mut self,
        result: Arc<Mutex<ProcessResult>>,
        port: String,
        ip: &mut IP,
        before: bool,
    ) {
        let normalized = normalize_port_name(port.clone());
        let name = normalized.name;
        let index = normalized.index;

        if let Ok(result) = result.clone().try_lock().as_mut() {
            if self.get_outports().ports[&name].is_addressable() {
                let idx = if let Some(idx) = &index {
                    idx.parse::<usize>().expect("expected port index")
                } else {
                    ip.index.expect("expected port index")
                };

                if !result.outputs.contains_key(&name) {
                    result.outputs.insert(name.clone(), HashMap::new());
                }
                if !result.outputs[&name].contains_key(format!("{}", idx).as_str()) {
                    result
                        .outputs
                        .get_mut(&name)
                        .unwrap()
                        .insert(format!("{}", idx), vec![]);
                }
                ip.index = Some(idx);
                if before {
                    result
                        .outputs
                        .get_mut(&name)
                        .unwrap()
                        .get_mut(format!("{}", idx).as_str())
                        .unwrap()
                        .insert(0, ip.clone());
                } else {
                    result
                        .outputs
                        .get_mut(&name)
                        .unwrap()
                        .get_mut(format!("{}", idx).as_str())
                        .unwrap()
                        .push(ip.clone());
                }
            } else {
                if !result.outputs.contains_key(&name) {
                    result.outputs.insert(name.clone(), HashMap::new());
                }
                if result.outputs.get_mut(&name).unwrap().is_empty() {
                    result
                        .outputs
                        .get_mut(&name)
                        .unwrap()
                        .insert("0".to_owned(), Vec::new());
                }
                if before {
                    result
                        .outputs
                        .get_mut(&name)
                        .unwrap()
                        .iter_mut()
                        .for_each(|(_, v)| {
                            v.insert(0, ip.clone());
                        });
                } else {
                    result
                        .outputs
                        .get_mut(&name)
                        .unwrap()
                        .get_mut("0")
                        .unwrap()
                        .push(ip.clone());
                }
            }
        }
    }
    /// Whenever an execution context finishes, send all resolved
    /// output from the queue in the order it is in.
    pub fn process_output_queue(&mut self) {
        while !self.get_output_queue().is_empty() {
            if !&self.get_output_queue()[0]
                .clone()
                .try_lock()
                .unwrap()
                .resolved
            {
                break;
            }

            if let Some(result) = self.get_output_queue_mut().pop_front().as_mut() {
                self.add_bracket_forwards(result.clone());
                if let Ok(result) = result.clone().try_lock() {
                    result.outputs.keys().for_each(|port| {
                        let mut port_identifier: Option<String> = None;
                        let ips = &result.outputs[port];
                        if self.get_outports().ports[port].is_addressable() {
                            ips.keys().for_each(|index| {
                                let mut idx_ips = (*ips)[index].clone();
                                if let Ok(idx) = index.parse::<usize>() {
                                    if !self.get_outports().ports[port].is_attached(Some(idx)) {
                                        return;
                                    }
                                }

                                idx_ips.iter_mut().for_each(|ip| {
                                    port_identifier = Some(format!("{}{:?}", port, (*ip).index));

                                    match &(*ip).datatype {
                                        IPType::OpenBracket(data) => {
                                            log!(
                                                Level::Debug,
                                                "zflow::component::send => {} sending {} < '{:?}'",
                                                self.get_node_id(),
                                                port_identifier.clone().unwrap(),
                                                data
                                            );
                                        }
                                        IPType::CloseBracket(data) => {
                                            log!(
                                                Level::Debug,
                                                "zflow::component::send => {} sending {} > '{:?}'",
                                                self.get_node_id(),
                                                port_identifier.clone().unwrap(),
                                                data
                                            );
                                        }
                                        _ => {
                                            log!(
                                                Level::Debug,
                                                "zflow::component::send => {} sending {} DATA",
                                                self.get_node_id(),
                                                port_identifier.clone().unwrap()
                                            );
                                        }
                                    }
                                    self.get_outports_mut()
                                        .ports
                                        .get_mut(port)
                                        .unwrap()
                                        .send_ip(ip, (*ip).index, true);
                                });
                            });
                            return;
                        }
                        if !self.get_outports().ports[port].is_attached(None) {
                            return;
                        }
                        ips.iter().for_each(|(_, ips)| {
                            ips.iter().for_each(|ip| {
                                port_identifier = Some(port.clone().to_string());
                                match &(*ip).datatype {
                                    IPType::OpenBracket(data) => {
                                        log!(
                                            Level::Debug,
                                            "zflow::component::send => {} sending {} < '{:?}'",
                                            self.get_node_id(),
                                            port_identifier.clone().unwrap(),
                                            data
                                        );
                                    }
                                    IPType::CloseBracket(data) => {
                                        log!(
                                            Level::Debug,
                                            "zflow::component::send => {} sending {} > '{:?}'",
                                            self.get_node_id(),
                                            port_identifier.clone().unwrap(),
                                            data
                                        );
                                    }
                                    _ => {
                                        log!(
                                            Level::Debug,
                                            "zflow::component::send => {} sending {} DATA",
                                            self.get_node_id(),
                                            port_identifier.clone().unwrap()
                                        );
                                    }
                                }
                                self.get_outports_mut()
                                    .ports
                                    .get_mut(port)
                                    .unwrap()
                                    .send_ip(ip, None, true);
                            });
                        });
                    });
                }
            }
        }
    }
    /// Signal that component has deactivated. There may be multiple
    /// deactivated contexts at the same time
    pub fn deactivate(&mut self, context: Arc<Mutex<ProcessContext>>) {
        let context = context.clone();
        let mut context = context.try_lock();
        let context = context.as_mut().unwrap();

        if context.deactivated {
            return;
        }

        context.activated = false;
        context.deactivated = true;

        if self.is_ordered() {
            self.process_output_queue();
        }

        self.load -= 1;
        context.load -= 1;

        self.get_publisher()
            .clone()
            .try_lock()
            .unwrap()
            .publish(ComponentEvent::Deactivate(context.load));
    }
    /// Signal that component has activated. There may be multiple
    /// activated contexts at the same time
    pub fn activate(&mut self, context: Arc<Mutex<ProcessContext>>) {
        let context = context.clone();
        let mut context = context.try_lock();
        let context = context.as_mut().unwrap();

        if context.activated {
            return;
        }

        context.activated = true;
        context.deactivated = false;

        self.load += 1;
        context.load += 1;

        self.get_publisher()
            .try_lock()
            .unwrap()
            .publish(ComponentEvent::Activate(context.load));

        if self.is_ordered() || self.get_auto_ordering().is_some() {
            self.get_output_queue_mut()
                .push_back(context.result.clone());
        }
    }

    /// Housekeeping function to reset this component;
    pub fn reset(&mut self) {
        // Clear contents of inport buffers
        self.get_inports_mut().ports.clear();
        self.clear_bracket_context();
        if !self.is_started() {
            return;
        }
        self.set_started(false);
        self.get_publisher()
            .clone()
            .try_lock()
            .unwrap()
            .publish(ComponentEvent::End);
    }

    /// Component error manager
    pub fn error(
        &mut self,
        e: ProcessError,
        groups: Vec<String>,
        mut error_port: Option<&str>,
        scope: Option<String>,
    ) -> Result<(), ProcessError> {
        if error_port.is_none() {
            error_port = Some("error")
        }

        if let Some(outport) = self.get_outports().ports.get_mut(error_port.unwrap()) {
            if outport.is_attached(None) || !outport.is_required() {
                groups.iter().for_each(|group| {
                    outport.open_bracket(
                        json!(group),
                        IPOptions {
                            scope: scope.clone().or_else(|| Some("".to_owned())).unwrap(),
                            ..IPOptions::default()
                        },
                        None,
                    );
                });
                outport.data(
                    json!(e.clone().0),
                    IPOptions {
                        scope: scope.clone().or_else(|| Some("".to_owned())).unwrap(),
                        ..IPOptions::default()
                    },
                    None,
                );
                groups.iter().for_each(|group| {
                    outport.close_bracket(
                        json!(group),
                        IPOptions {
                            scope: scope.clone().or_else(|| Some("".to_owned())).unwrap(),
                            ..IPOptions::default()
                        },
                        None,
                    );
                });
                return Ok(());
            }
        }

        log!(Level::Error, "{:?}", e);
        Err(e)
    }

    pub(crate) fn set_started(&mut self, started: bool) {
        self.started = started;
    }

    pub(crate) fn is_started(&self) -> bool {
        self.started
    }
    /// Method for checking whether the component sends packets
    /// in the same order they were received.
    pub fn is_ordered(&self) -> bool {
        if self.ordered || self.auto_ordering == Some(true) {
            return true;
        };
        return false;
    }
    pub fn is_subgraph(&self) -> bool {
        self.graph.is_some()
    }
    pub fn is_ready(&self) -> bool {
        return self.ready;
    }
    pub fn get_description(&self) -> String {
        self.description.clone()
    }

    pub fn get_name(&self) -> Option<String> {
        self.component_name.clone()
    }

    pub fn get_node_id(&self) -> String {
        self.node_id.clone()
    }

    pub fn set_node_id(&mut self, id: String) {
        self.node_id = id;
    }

    pub fn get_handle(&self) -> Option<Arc<Mutex<ProcessFunc>>> {
        self.handle.clone()
    }

    pub fn set_handle(&mut self, handle: Box<ProcessFunc>) {
        self.handle = Some(Arc::new(Mutex::new(handle)));
    }

    pub fn get_inports(&self) -> InPorts {
        self.in_ports.clone()
    }

    pub fn get_inports_mut(&mut self) -> &mut InPorts {
        &mut self.in_ports
    }

    pub fn get_outports(&self) -> OutPorts {
        self.out_ports.clone()
    }

    pub fn get_outports_mut(&mut self) -> &mut OutPorts {
        &mut self.out_ports
    }

    pub fn get_auto_ordering(&self) -> Option<bool> {
        self.auto_ordering
    }

    pub fn set_auto_ordering(&mut self, auto_ordering: bool) {
        self.auto_ordering = Some(auto_ordering);
    }

    pub fn get_ordered(&self) -> bool {
        self.ordered
    }

    pub fn get_output_queue(&self) -> VecDeque<Arc<Mutex<ProcessResult>>> {
        self.output_q.clone()
    }

    pub fn get_output_queue_mut(&mut self) -> &mut VecDeque<Arc<Mutex<ProcessResult>>> {
        &mut self.output_q
    }

    pub fn get_load(&self) -> usize {
        self.load
    }

    pub fn set_load(&mut self, load: usize) {
        self.load = load
    }

    pub fn get_icon(&self) -> String {
        self.icon.clone()
    }

    pub fn get_teardown_function(
        &self,
    ) -> Option<Arc<Mutex<dyn FnMut(*mut Self) -> Result<(), String> + Send + Sync + 'static>>>
    {
        self.teardown_fn.clone()
    }

    pub fn get_setup_function(
        &self,
    ) -> Option<Arc<Mutex<dyn FnMut(*mut Self) -> Result<(), String> + Send + Sync + 'static>>>
    {
        self.setup_fn.clone()
    }

    /// Convenience method to find all the event subscribers for this component: Used internally
    pub(crate) fn get_subscribers(&self) -> Vec<Arc<SubscriptionFunc<ComponentEvent>>> {
        self.tracked_signals.clone()
    }

    pub(crate) fn get_subscribers_mut(
        &mut self,
    ) -> &mut Vec<Arc<SubscriptionFunc<ComponentEvent>>> {
        &mut self.tracked_signals
    }
    /// Convenience method to access the internal event publisher for this component: Used internally
    pub(crate) fn get_publisher(&self) -> Arc<Mutex<Publisher<ComponentEvent>>> {
        self.bus.clone()
    }

    pub fn clear_bracket_context(&mut self) {
        self.bracket_context.r#in.clear();
        self.bracket_context.out.clear();
    }

    pub fn set_icon(&mut self, icon: &str) {
        self.icon = icon.to_string();
        self.bus
            .clone()
            .try_lock()
            .unwrap()
            .publish(ComponentEvent::Icon(json!({
                "id": self.node_id,
                "icon": icon
            })));
    }

    pub fn get_bracket_context_val(&self) -> BracketContext {
        self.bracket_context.clone()
    }
    pub fn get_bracket_context_val_mut(&mut self) -> &mut BracketContext {
        &mut self.bracket_context
    }

    pub fn get_forward_brackets(&self) -> HashMap<String, Vec<String>> {
        self.forward_brackets.clone()
    }

    pub fn get_forward_brackets_mut(&mut self) -> &mut HashMap<String, Vec<String>> {
        &mut self.forward_brackets
    }

    pub fn get_base_dir(&self) -> String {
        self.base_dir.clone()
    }

    pub fn set_base_dir(&mut self, dir: String) {
        self.base_dir = dir;
    }
    pub fn set_name(&mut self, name: String) {
        self.component_name = Some(name);
    }

    pub fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
        self.get_publisher()
            .try_lock()
            .expect("expected publisher")
            .publish(ComponentEvent::Ready);
    }

    pub fn set_description(&mut self, desc: String) {
        self.description = desc;
    }

    pub fn get_handler_thread(&self) -> Arc<Mutex<HandlerThread>> {
        self.internal_thread.clone()
    }

    // pub fn get_network(&self) -> Option<Arc<Mutex<dyn BaseNetwork + Send + Sync>>> {
    //     self.network.clone()
    // }

    // pub fn set_loader(&mut self, loader: ComponentLoader) {
    //     // self.loader = Some(loader);
    // }

    pub fn setup_graph(
        component: Arc<Mutex<Component>>,
        graph: Option<&dyn GraphDefinition>,
    ) -> Result<(), String> {
        if let Ok(component) = component.clone().try_lock().as_mut() {
            component.set_ready(false);

            if let Some(g) = graph {
                if let Some(graph) = g.to_any().downcast_ref::<Graph>() {
                    return component.create_network(graph.clone());
                }
                if let Some(graph_json) = g.to_any().downcast_ref::<GraphJson>() {
                    return component.create_network(Graph::from_json(graph_json.clone(), None));
                }
            }
            if let Some(g) = component.clone().graph.as_mut() {
                if let Some(graph) = g.to_any().downcast_ref::<Graph>() {
                    return component.create_network(graph.clone());
                }
                if let Some(graph_json) = g.to_any().downcast_ref::<GraphJson>() {
                    return component.create_network(Graph::from_json(graph_json.clone(), None));
                }
            }
        }
        Err(format!("Could not setup graph component"))
    }

    pub fn create_network(&mut self, mut graph: Graph) -> Result<(), String> {
        if let Some(description) = graph.properties.get("description") {
            self.set_description(
                description
                    .as_str()
                    .expect("expected description string")
                    .to_owned(),
            );
        }

        if let Some(icon) = graph.properties.get("icon") {
            self.set_icon(icon.as_str().expect("expected icon string"));
        }

        if graph.name.is_empty() && !self.get_node_id().is_empty() {
            graph.name = self.get_node_id();
        }

        // Todo: call create-network factory

        Ok(())
    }

    pub fn subscribe_network(
        &mut self,
        network: &mut Network,
    ) {
        let bus = self.get_publisher();
        network
                .get_publisher()
                .try_lock()
                .expect("expected network publisher")
                .subscribe_fn(move |event| {
                    match event.as_ref() {
                        NetworkEvent::Start(_) => {
                            // activate
                            bus.clone().try_lock().unwrap().publish(ComponentEvent::Activate(0));
                        }
                        NetworkEvent::End(_) => {
                            // deactivate
                            bus.clone().try_lock().unwrap().publish(ComponentEvent::Deactivate(0))
                        }
                        _ => {}
                    }
                });
    }

    pub fn is_exported_inport(
        &mut self,
        _port: InPort,
        name: &str,
        port_name: &str,
    ) -> Option<String> {
        if self.network.is_none() {
            return None;
        }
        // First we check disambiguated exported ports
        if let Some(network) = self.network {
            let network = unsafe { network.read() };
            if let Ok(graph) = network.get_graph().try_lock() {
                for key in graph.inports.keys() {
                    if let Some(priv_port) = graph.inports.get(key) {
                        if priv_port.process == name && priv_port.port == port_name {
                            return Some(key.clone());
                        }
                    }
                }
            }
        }
        // Component has exported ports and this isn't one of them
        None
    }

    pub fn is_exported_outport(
        &mut self,
        _port: OutPort,
        name: &str,
        port_name: &str,
    ) -> Option<String> {
        if self.network.is_none() {
            return None;
        }
        // First we check disambiguated exported ports
        if let Some(network) = self.network {
            let network = unsafe { network.read() };
            if let Ok(graph) = network.get_graph().try_lock() {
                for key in graph.outports.keys() {
                    if let Some(priv_port) = graph.outports.get(key) {
                        if priv_port.process == name && priv_port.port == port_name {
                            return Some(key.clone());
                        }
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
        process: NetworkProcess,
    ) -> bool {
        if process.component.is_none() {
            return false;
        }
        let binding = process.component.clone().unwrap();
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
                        //let p_binding = binding.clone();
                        let mut binding = binding.try_lock();
                        match event.as_ref() {
                            SocketEvent::Connect(_) => {
                                if let Ok(graph_component) = binding.as_mut() {
                                    // Start the network implicitly if we're starting to get data
                                    if graph_component.starting || graph_component.network.is_none()
                                    {
                                        return;
                                    }
                                    if let Some(network) = graph_component.network {
                                        let mut network = unsafe { network.read() };
                                        if let Ok(manager) = network.manager.clone().try_lock() {
                                            if manager.is_started() {
                                                return;
                                            }
                                        }

                                        if network.get_startup_time().is_some() {
                                            // Network was started, but did finish. Re-start simply
                                            network.set_started(true);
                                            return;
                                        }

                                        // Network was never started, start properly
                                        let _ = graph_component.graph_set_up();
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

    pub(crate) fn graph_set_up(&mut self) -> Result<(), String> {
        self.starting = true;

        if !self.is_ready() {
            if let Ok(publisher) = self.bus.clone().try_lock().as_mut() {
                publisher.subscribe_fn(move |event| {

                    // match event.as_ref() {
                    //     ComponentEvent::Ready => {
                    //         if !_component.is_ready() {
                    //             Component::graph_set_up(component.clone())
                    //                 .expect("expected graph component to setup");
                    //         }
                    //     }
                    //     _ => {}
                    // }
                });
            }
        }
        if self.network.is_none() {
            return Ok(());
        }

        let mut network = unsafe { self.network.unwrap().read() };

        network.start()?;
        self.starting = false;

        Ok(())
    }

    pub fn graph_tear_down(&mut self) -> Result<(), String> {
        if self.network.is_none() {
            return Ok(());
        }

        let mut network = unsafe { self.network.unwrap().read() };
        return network.stop();
    }
}

pub struct ComponentOptions {
    pub in_ports: HashMap<String, InPort>,
    pub out_ports: HashMap<String, OutPort>,
    /// Set the default component description
    pub description: String,
    /// Set the default component icon
    pub icon: String,
    /// Whether the component should keep send packets
    /// out in the order they were received
    pub ordered: bool,
    /// Whether the component should activate when it receives packets
    pub activate_on_input: bool,
    /// Bracket forwarding rules. By default we forward
    pub forward_brackets: HashMap<String, Vec<String>>,
    pub graph: Option<Box<dyn GraphDefinition>>,
    pub process: Option<Box<ProcessFunc>>,
    pub metadata: Option<Map<String, Value>>,
}

impl Clone for ComponentOptions {
    fn clone(&self) -> Self {
        let mut s = Self {
            in_ports: self.in_ports.clone(),
            out_ports: self.out_ports.clone(),
            description: self.description.clone(),
            icon: self.icon.clone(),
            ordered: self.ordered.clone(),
            activate_on_input: self.activate_on_input.clone(),
            forward_brackets: self.forward_brackets.clone(),
            graph: None,
            process: None,
            metadata: self.metadata.clone(),
        };
        if let Some(graph) = &self.graph {
            s.graph = Some(dyn_clone::clone_box(&**graph));
        }

        s
    }
}

impl Default for ComponentOptions {
    fn default() -> Self {
        Self {
            in_ports: Default::default(),
            out_ports: Default::default(),
            description: Default::default(),
            icon: Default::default(),
            ordered: Default::default(),
            activate_on_input: Default::default(),
            forward_brackets: HashMap::from([(
                "in".to_string(),
                vec!["out".to_string(), "error".to_string()],
            )]),
            process: None,
            graph: None,
            metadata: None,
        }
    }
}

impl Debug for ComponentOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentOptions")
            .field("in_ports", &self.in_ports)
            .field("out_ports", &self.out_ports)
            .field("icon", &self.icon)
            .field("ordered", &self.ordered)
            .field("description", &self.description)
            .field("activate_on_input", &self.activate_on_input)
            .field("forward_brackets", &self.forward_brackets)
            .field("process", &"[ProcessFunc]")
            .finish()
    }
}

// impl Serialize for ComponentOptions {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: serde::Serializer {
//         let mut options = serializer.serialize_struct("ComponentOptions", 2)?;
//         options.serialize_field("in_ports", &self.in_ports)?;
//         options.serialize_field("out_ports", &self.out_ports)?;
//         options.end()
//     }
// }
