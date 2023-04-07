use core::panic;
use std::any::Any;
use std::pin::Pin;
use std::sync::{Arc, Mutex};


use futures::Future;
use log::{log, Level};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::component::ComponentTrait;
use crate::ip::{IPOptions, IPType, IP};
use crate::port::{normalize_port_name, BasePort, InPort, InPorts, OutPorts};
use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;

pub type ValidatorFn = Box<dyn (FnMut(IP) -> bool) + Send + Sync>;

#[derive(Debug, Clone, Default)]
pub struct ProcessError(pub String);

pub type ProcessFunc<T> = dyn (FnMut(
    Arc<Mutex<ProcessContext<T>>>,
    ProcessInput<T>,
    ProcessOutput<T>,
) ->Result<ProcessResult<T>, ProcessError>)
+ Sync + Send;

#[derive(Clone, Default)]
pub struct ProcessResult<T: ComponentTrait> {
    pub resolved: bool,
    pub bracket_closing_before: Vec<Arc<Mutex<ProcessContext<T>>>>,
    pub bracket_closing_after: Vec<Arc<Mutex<ProcessContext<T>>>>,
    pub bracket_context: Option<HashMap<String, Vec<Arc<Mutex<ProcessContext<T>>>>>>,
    pub data: Value,
    pub outputs: HashMap<String, HashMap<String, Vec<IP>>>,
}

impl<T> Debug for ProcessResult<T>
where
    T: ComponentTrait,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessResult")
            .field("resolved", &self.resolved)
            .field("bracket_closing_before", &self.bracket_closing_before)
            .field("bracket_closing_after", &self.bracket_closing_after)
            .field("bracket_context", &self.bracket_context)
            .field("data", &self.data)
            .field("outputs", &self.outputs)
            .finish()
    }
}

#[derive(Clone)]
pub struct ProcessInput<T: ComponentTrait> {
    pub in_ports: InPorts,
    pub context: Arc<Mutex<ProcessContext<T>>>,
    pub component: Arc<Mutex<T>>,
    pub ip:IP,
    pub result:Arc<Mutex<ProcessResult<T>>>,
    pub scope:Option<String>,
    pub port:InPort
}

impl<T> Debug for ProcessInput<T>
where
    T: ComponentTrait,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessInput")
            .field("in_ports", &self.in_ports)
            .field("context", &self.context.clone())
            .field("component", &"[Component]")
            .field("ip", &self.ip)
            .field("result", &self.result)
            .field("scope", &self.scope)
            .finish()
    }
}

impl<T> ProcessInput<T>
where
    T: ComponentTrait,
{
    // pub fn new(
    //     in_ports: InPorts,
    //     context: Arc<Mutex<ProcessContext<T>>>,
    //     component: Arc<Mutex<T>>,
    // ) -> Arc<Mutex<Self>> {
    //     Arc::new(Mutex::new(Self {
    //         in_ports,
    //         context: context.clone(),
    //         component: component.clone(),
    //     }))
    // }

    /// When preconditions are met, set component state to `activated`
    pub fn activate(&mut self) {
        if let Ok(context) = self.context.clone().try_lock() {
            if context.activated {
                return;
            }
        }

        if let Ok(component) = self.component.clone().try_lock().as_mut() {
                if component.is_ordered() {
                    // We're handling packets in order. Set the result as non-resolved
                    // so that it can be send when the order comes up
                    self.result.clone().try_lock().unwrap().resolved = false;
                }
                component.activate(self.context.clone());

                
                    if self.port.is_addressable() {
                        log!(
                            Level::Debug,
                            "zflow::component => {} packet on '{}[{}]' caused activation {}: {:?}",
                            component.get_node_id(),
                            self.port.name,
                            self
                                .ip
                                .clone()
                                .index
                                .expect("expected port index for addressable port"),
                            component.get_load(),
                            self.ip.clone().datatype
                        );
                    } else {
                        log!(
                            Level::Debug,
                            "zflow::component => {} packet on '{}' caused activation {}: {:?}",
                            component.get_node_id(),
                            self.port.name,
                            component.get_load(),
                            self.ip.clone().datatype
                        );
                    }
            
        }
    }

    /// ## Connection listing
    /// This allows components to check which input ports are attached. This is
    /// useful mainly for addressable ports
    pub fn attached(&mut self, ports: Vec<&str>) -> Vec<Vec<usize>> {
        let mut attached = vec![];
        for port in ports {
            if let Some(port_impl) = self.in_ports.ports.get(port) {
                attached.push(port_impl.list_attached());
            }
        }

        attached
    }

    /// ## Input preconditions
    /// When the processing function is called, it can check if input buffers
    /// contain the packets needed for the process to fire.
    /// This precondition handling is done via the `has` and `has_stream` methods.

    /// Returns true if a port has a new IP
    /// Passing a validation callback as a last argument allows more selective
    /// checking of packets.
    pub fn has(
        &mut self,
        port: &str,
        validator: Option<ValidatorFn>,
    ) -> bool {
        let normalize = normalize_port_name(port.to_string());
        let name = normalize.name;
        let idx = normalize.index;
        if let Some(port_impl) = self.in_ports.ports.get_mut(&name) {
            if port_impl.is_addressable() {
                if idx.is_none() {
                    log!(
                        Level::Error,
                        "zflow::component => Index must be set for addressable port"
                    );
                    return false;
                }
                let idx = idx.unwrap().parse::<usize>().ok();
                return port_impl.has(self.scope.clone(), idx, validator);
            } else {
                return port_impl.has(self.scope.clone(), None, validator);
            }
        } else {
            log!(
                Level::Error,
                "zflow::component => Unknown port"
            );
        }
        false
    }

    ///  Returns true if the ports contain data packets
    pub fn has_data(&mut self, port: &str) -> bool {
        return self.has(
            port,
            Some(Box::new(|ip: IP| match ip.datatype {
                IPType::Data(_) => true,
                _ => false,
            })),
        );
    }

    ///  Returns true if a port has a complete stream in its input buffer.
    pub fn has_stream(&mut self, port: &str) -> bool {
        return self.has(
            port,
            Some(Box::new(|ip: IP| match ip.datatype {
                IPType::OpenBracket(_) | IPType::CloseBracket(_) => true,
                IPType::Data(_) => false,
                _ => false,
            })),
        );
    }
    /// ## Input processing
    ///
    /// Once preconditions have been met, the processing function can read from
    /// the input buffers. Reading packets sets the component as "activated".
    ///
    /// Fetches IP object for port
    pub fn get(&mut self, port: &str) -> Option<IP> {
        self.activate();
        let normalize = normalize_port_name(port.to_string());
        let port_name = normalize.name;
        let idx = normalize.index;

        if let Some(port) = self.in_ports.ports.get_mut(port) {
            if port.is_addressable() {
                if idx.is_none() {
                    let msg = format!("For addressable ports, access must be with index");
                    log!(Level::Error, "zflow::component => {}", msg);
                    return None;
                }

                let idx = idx.unwrap().parse::<usize>().ok();
                if let Ok(component) = self.component.clone().try_lock().as_mut() {
                    if component.is_forwarding_inport(&port_name) {
                        return self.get_for_forwarding(port_name, idx);
                    } else {
                        return port.get(self.scope.clone(), idx);
                    }
                }
            }
        }

        if let Ok(component) = self.component.clone().try_lock().as_mut() {
            if component.is_forwarding_inport(&port_name) {
                return self.get_for_forwarding(port_name, None);
            }
        }
        return self
                .in_ports
                .clone()
                .ports
                .get_mut(port)
                .map(|p| p.get(self.scope.clone(), None))
                .unwrap();

    }

    /// Fetches Data object for port
    pub fn get_data(&mut self, port: &str) -> Option<Value> {
        if self.has_data(port) {
            if let Some(packet) = self.get(port) {
                match &packet.datatype {
                    IPType::Data(packet) => {
                        return Some(packet.clone());
                    }
                    _ => {}
                }
            }
            return None;
        }

        None
    }

    /// ## Input processing
    ///
    /// Once preconditions have been met, the processing function can read from
    /// the input buffers. Reading packets sets the component as "activated".
    ///
    /// Fetches IP objects for ports
    pub fn get_many(&mut self, ports: Vec<&str>) -> Vec<Option<IP>> {
        self.activate();
        let mut res = vec![];

        for port in ports {
            res.push(self.get(port));
        }

        res
    }

    fn get_for_forwarding(&mut self, name: String, idx: Option<usize>) -> Option<IP> {
        let mut prefix: Vec<IP> = vec![];
        let mut data_ip = None;
        // Read IPs until we hit data
        
            loop {
                if let Some(port_impl) = self.in_ports.ports.get_mut(&name) {
                    let ip = port_impl.get(self.scope.clone(), idx);
                    // Stop at the end of the buffer
                    if ip.is_none() {
                        break;
                    }
                    match ip.clone().unwrap().datatype {
                        IPType::Data(_) => {
                            // Hit the data IP, stop here
                            data_ip = ip;
                            break;
                        }
                        _ => {}
                    }
                    // Keep track of bracket closings and openings before
                    prefix.push(ip.unwrap());
                }
            }
            // Forwarding brackets that came before data packet need to manipulate context
            // and be added to result so they can be forwarded correctly to ports that
            // need them
            prefix.iter().for_each(|ip| {
                match ip.clone().datatype {
                    IPType::OpenBracket(_) => {
                        // Bracket openings need to go to bracket context
                        if let Ok(component) = self.component.clone().try_lock().as_mut() {
                            if let Some(contexts) = component.get_bracket_context(
                                "in",
                                name.clone(),
                                self.scope.clone().unwrap(),
                                idx,
                            ) {
                                contexts.push(Arc::new(Mutex::new(ProcessContext {
                                    ip: ip.clone(),
                                    ports: vec![],
                                    source: name.clone(),
                                    ..ProcessContext::default()
                                })))
                            }
                        }
                    }
                    IPType::CloseBracket(_) => {
                        // Bracket closings before data should remove bracket context
                        if let Ok(res) = self.result.clone().try_lock().as_mut() {
                            if let Ok(component) = self.component.clone().try_lock().as_mut()
                            {
                                if let Some(contexts) = component.get_bracket_context(
                                    "in",
                                    name.clone(),
                                    self.scope.clone().unwrap(),
                                    idx,
                                ) {
                                    if let Some(context) = contexts.pop() {
                                        context.clone().try_lock().unwrap().close_ip =
                                            Some(ip.clone());
                                        res.bracket_closing_before.push(context.clone());
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            });
            // Add current bracket context to the result so that when we send
            // to ports we can also add the surrounding brackets
            if let Ok(res) = self.result.clone().try_lock().as_mut() {
                if let Ok(component) = self.component.clone().try_lock().as_mut() {
                    if let Some(contexts) = component.get_bracket_context(
                        "in",
                        name.clone(),
                        self.scope.clone().unwrap(),
                        idx,
                    ) {
                        if res.bracket_context.is_none() {
                            res.bracket_context = Some(HashMap::new());
                        }

                        let contexts = contexts.clone();
                        res.bracket_context
                            .as_mut()
                            .unwrap()
                            .insert(name.clone(), contexts);
                    }
                }
            }
        // Bracket closings that were in buffer after the data packet need to
        // be added to result for done() to read them from
        data_ip
    }
}

#[derive(Clone)]
pub struct ProcessOutput<T: ComponentTrait> {
    pub out_ports: OutPorts,
    pub context: Arc<Mutex<ProcessContext<T>>>,
    pub component: Arc<Mutex<T>>,
    pub ip:IP,
    pub result:Arc<Mutex<ProcessResult<T>>>,
    pub scope:Option<String>
}
impl<T> Debug for ProcessOutput<T>
where
    T: ComponentTrait,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessOutput")
            .field("out_ports", &self.out_ports)
            .field("context", &self.context.clone())
            .field("component", &"[Component]")
            .field("ip", &self.ip)
            .field("result", &self.result)
            .field("scope", &self.scope)
            .finish()
    }
}

impl<T> ProcessOutput<T>
where
    T: ComponentTrait,
{
    // pub fn new(
    //     out_ports: OutPorts,
    //     context: Arc<Mutex<ProcessContext<T>>>,
    //     component: Arc<Mutex<T>>,
    // ) -> Arc<Mutex<Self>> {
    //     Arc::new(Mutex::new(Self {
    //         out_ports,
    //         context: context.clone(),
    //         component: component.clone(),
    //     }))
    // }

    /// Sends an error object
    pub fn error(&mut self, err: &dyn Any) -> Result<(), String> {
        let errors = if let Some(err) = err.downcast_ref::<ProcessError>() {
            vec![err.clone()]
        } else if let Some(err) = err.downcast_ref::<Vec<ProcessError>>() {
            err.clone()
        } else {
            return Err(format!("Invalid error value"));
        };

        if self.out_ports.ports.contains_key("error") {
            if let Some(error_port) = self.out_ports.ports.get("error") {
                if error_port.is_attached(None) || !error_port.is_required() {
                    if errors.len() > 1 {
                        self.send_ip(
                            "error",
                            &IP::new(IPType::OpenBracket(Value::Null), IPOptions::default()),
                        );
                    }
                    errors
                        .iter()
                        .for_each(|err| self.send_ip("error", &Value::String(err.0.clone())));
                    if errors.len() > 1 {
                        self.send_ip(
                            "error",
                            &IP::new(IPType::CloseBracket(Value::Null), IPOptions::default()),
                        );
                    }
                }
            }
        }

        Err(format!(
            "errors {}",
            errors
                .iter()
                .map(|e| e.0.clone())
                .collect::<Vec<String>>()
                .join(",\n")
        ))
    }
    /// Sends a single IP object to a port.
    ///
    /// `packet` can be type of `IP` or `Value`
    pub fn send_ip(&mut self, port: &str, packet: &dyn Any) {
        let mut ip = if let Some(ip) = packet.downcast_ref::<IP>() {
            ip.clone()
        }  else if let Some(data) = packet.downcast_ref::<IPType>() {
            IP::new(data.clone(), IPOptions::default())
        } else if let Some(data) = packet.downcast_ref::<Value>() {
            if let Ok(ip) = IP::deserialize(data) {
                ip
            } else {
                IP::new(IPType::Data(data.clone()), IPOptions::default())
            }
        } else if let Some(res) = packet.downcast_ref::<ProcessResult<T>>() {
            IP::new(IPType::Data(res.clone().data), IPOptions::default())
        } else {
            return;
        };

        if self.scope.is_some() && ip.scope.is_empty() {
            ip.scope = self.scope.clone().unwrap();
        }

        if let Ok(component) = self.component.clone().try_lock().as_mut() {
            if !component.get_outports().ports.contains_key(port) {
                panic!(
                    "Node {} does not have outport {}",
                    component.get_node_id(),
                    port
                );
            }
            if let Some(port_impl) = self.out_ports.ports.get_mut(port) {
                if port_impl.is_addressable() && ip.index.is_none() {
                    panic!(
                        "Sending packets to addressable port {} {} requires specifying index",
                        component.get_node_id(),
                        port
                    );
                }
                if component.is_ordered() {
                    component.add_to_result(
                        self.result.clone(),
                        port.to_string(),
                        &mut ip,
                        false,
                    );
                    return;
                }
                if !port_impl.options.scoped {
                    ip.scope = "".to_string();
                }

                port_impl.send_ip(&ip, ip.index, true);
            }
        }
    }

    /// Sends the argument via `send()` and marks activation as `done()`
    pub fn send_done(&mut self, packet: &dyn Any) {
        self.send(packet);
        self.done(None);
    }
    /// Sends packets for each port as a key in a map, tupple, 
    /// or sends Error or a list of Errors if passed such
    ///
    /// Accepts either `ProcessError`, `Vec<ProcessError>`, tupple `(&str, Value)`, tupple `(&str, IPType)`  or an output map `Map<String, Value>`
    pub fn send(&mut self, packet: &dyn Any) {
        if let Some(err) = packet.downcast_ref::<ProcessError>() {
            let _ = self.error(err);
            return;
        } else if let Some(err) = packet.downcast_ref::<Vec<ProcessError>>() {
            let _ = self.error(err);
            return;
        }

        let component_ports = &mut vec![];
        let mut maps_in_ports = false;
        if let Some(ports) = packet.downcast_ref::<HashMap<&str, Value>>() {
            let outports = self.out_ports.clone();
            for port in outports.ports.keys() {
                if (port != "error") && (port != "ports") {
                    component_ports.push(port);
                }
                if !maps_in_ports && ports.contains_key(port.as_str()) {
                    maps_in_ports = true;
                }
            }

            if (component_ports.len() == 1) && !maps_in_ports {
                self.send_ip(component_ports.clone()[0], &json!(ports.clone()));
                return;
            }

            ports.iter().for_each(|(port, packet)| {
                self.send_ip(port, packet);
            });

            return;
        }
        if let Some((port, data)) = packet.downcast_ref::<(&str, Value)>() {
            self.send_ip(*port, data);
            return;
        }
        if let Some((port, data)) = packet.downcast_ref::<(&str, IPType)>() {
            self.send_ip(*port, &IP::new(data.clone(), IPOptions::default()));
            return;
        }
        for port in self.out_ports.clone().ports.keys() {
            self.send_ip(port, packet);
        }
    }

    /// Finishes process activation gracefully
    pub fn done(&mut self, err: Option<&dyn Any>) {
        if let Some(err) = err {
            if let Some(err) = err.downcast_ref::<ProcessError>() {
                let _ = self.error(err);
                return;
            } else if let Some(err) = err.downcast_ref::<Vec<ProcessError>>() {
                let _ = self.error(err);
                return;
            }
        }

        self.result.clone().try_lock().unwrap().resolved = true;

        if let Ok(component) = self.component.clone().try_lock().as_mut() {
            component.activate(self.context.clone());
            let mut _component = component.clone();
            let mut is_last = || {
                // We only care about real output sets with processing data
                let results_only = _component
                    .get_output_queue_mut()
                    .iter()
                    .filter(|q| {
                        if let Ok(q) = q.clone().try_lock().as_mut() {
                            if !q.resolved {
                                return true;
                            }

                            if !q.bracket_closing_after.is_empty() {
                                return false;
                            }
                        }
                        false
                    })
                    .collect::<VecDeque<&Arc<Mutex<ProcessResult<T>>>>>();
                let pos = {
                    let pos = results_only
                        .iter()
                        .position(|res| Arc::ptr_eq(*res, &self.result));

                    if pos.is_some() {
                        pos.unwrap() as i32
                    } else {
                        -1
                    }
                };
                
                let len = results_only.len();

                let load = _component.get_load();

                if pos == (len as i32 - 1) {
                    return true;
                }
                if (pos == -1) && (load == (len + 1)) {
                    return true;
                }
                if (len <= 1) && (load == 1) {
                    return true;
                }

                false
            };

            if component.is_ordered() && is_last() {
                // We're doing bracket forwarding. See if there are
                // dangling closeBrackets in buffer since we're the
                // last running process function.
                component
                    .get_bracket_context_val()
                    .r#in
                    .iter()
                    .for_each(|(port, _)| {
                        let contexts = &component.get_bracket_context_val().r#in[&port.clone()];
                        let scope = if let Some(scope) = self.scope.clone() {
                            scope.clone()
                        } else {
                            "".to_string()
                        };

                        if !contexts.clone().contains_key(&scope) {
                            return;
                        }
                        if let Some(node_context) = contexts.clone().get_mut(&scope) {
                            if node_context.is_empty() {
                                return;
                            }
                            let context = node_context.last();
                            if let Some(context) = context {
                                if let Some(in_ports) = component
                                    .get_inports()
                                    .ports
                                    .get_mut(&context.clone().try_lock().unwrap().source)
                                {
                                    let ip_scope = if !context
                                        .clone()
                                        .try_lock()
                                        .unwrap()
                                        .ip
                                        .scope
                                        .is_empty()
                                    {
                                        Some(context.clone().try_lock().unwrap().ip.scope.clone())
                                    } else {
                                        None
                                    };
                                    if let Some(buf) = in_ports.get_buffer(
                                        ip_scope.clone(),
                                        context.clone().try_lock().unwrap().ip.index,
                                        false,
                                    ) {
                                        if let Ok(buf) = buf.clone().try_lock() {
                                            while !buf.is_empty() {
                                                match &buf[0].datatype {
                                                    IPType::CloseBracket(_) => {
                                                        if let Some(ip) = in_ports.get(
                                                            ip_scope.clone(),
                                                            context
                                                                .clone()
                                                                .try_lock()
                                                                .unwrap()
                                                                .ip
                                                                .index,
                                                        ) {
                                                            if let Some(ctx) = node_context.clone().pop() {
                                                                let _ = ctx.clone().try_lock().as_mut().map(|ctx| ctx.close_ip = Some(ip.clone())).unwrap();
                                                                
                                                                if let Ok(result) = self.result.clone().try_lock().as_mut() {
                                                                    if result.bracket_closing_after.is_empty() {
                                                                        result.bracket_closing_after = vec![];
                                                                    }
                                                                    result.bracket_closing_after.push(ctx);
                                                                }
                                                            }
                                                        }
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });
            }
            
            log!(Level::Debug, "zflow::component {} finished processing {}", component.get_node_id(), component.get_load());
            component.deactivate(self.context.clone());
        }
    }
}

#[derive(Clone)]
pub struct ProcessContext<T: ComponentTrait> {
    pub close_ip: Option<IP>,
    pub ip: IP,
    pub component: Arc<Mutex<T>>,
    pub ports: Vec<String>,
    pub source: String,
    pub data: Value,
    pub result: Arc<Mutex<ProcessResult<T>>>,
    pub activated: bool,
    pub deactivated: bool,
    pub scope: Option<String>,
}

impl<T> Default for ProcessContext<T>
where
    T: ComponentTrait,
{
    fn default() -> Self {
        Self {
            close_ip: Default::default(),
            ip: Default::default(),
            component: Default::default(),
            ports: Default::default(),
            source: Default::default(),
            data: Default::default(),
            result: Default::default(),
            activated: Default::default(),
            deactivated: Default::default(),
            scope: Default::default(),
        }
    }
}

impl<T> Debug for ProcessContext<T>
where
    T: ComponentTrait,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessContext")
            .field("close_ip", &self.close_ip)
            .field("ip", &self.ip)
            .field("component", &"[Component]")
            .field("ports", &self.ports)
            .field("source", &self.source)
            .field("data", &self.data)
            .field("result", &self.result)
            .field("activated", &self.activated)
            .field("deactivated", &self.deactivated)
            .field("scope", &self.scope)
            .finish()
    }
}

impl<T> ProcessContext<T>
where
    T: ComponentTrait,
{
    // pub fn new(ip: IP, port: InPort<T>, result: Arc<Mutex<ProcessResult<T>>>) -> Arc<Mutex<Self>> {
    //     Arc::new(Mutex::new(Self {
    //         ip,
    //         component: None,
    //         result: result.clone(),
    //         activated: false,
    //         deactivated: true,
    //         ports: vec![port.name],
    //         close_ip: None,
    //         source: "".to_owned(),
    //         data: Value::Null,
    //         scope: None,
    //     }))
    // }

    pub fn activate(_context: Arc<Mutex<ProcessContext<T>>>, _component: Arc<Mutex<T>>) {
        if let Ok(context) = _context.clone().try_lock().as_mut() {
            if let Ok(component) = _component.clone().try_lock().as_mut() {
                if context.result.clone().try_lock().unwrap().resolved
                    || component
                        .get_output_queue()
                        .iter()
                        .find(|ctx| Arc::ptr_eq(ctx.clone(), &context.result))
                        .is_some()
                {
                    context.result = Arc::new(Mutex::new(ProcessResult::default()));
                }
                component.activate(_context)
            }
        }
    }

    pub fn deactivate(_context: Arc<Mutex<ProcessContext<T>>>, _component: Arc<Mutex<T>>) {
        if let Ok(context) = _context.clone().try_lock().as_mut() {
            if let Ok(component) = _component.clone().try_lock().as_mut() {
                if !context.result.clone().try_lock().unwrap().resolved {
                    context.result.clone().try_lock().unwrap().resolved = true;
                }
                component.deactivate(_context)
            }
        }
    }
}
