
use std::any::Any;
use std::sync::{Arc, Mutex};

use fp_rust::handler::HandlerThread;
use log::{log, Level};
use serde::{Deserialize, Serialize};
use serde_json::{json,  Value};

use crate::component::Component;
use crate::ip::{IPOptions, IPType, IP};
use crate::port::{normalize_port_name, BasePort, InPort, InPorts, OutPorts};
use std::collections::{HashMap, VecDeque};
use std::fmt::Debug;

pub type ValidatorFn = Box<dyn (FnMut(IP) -> bool) + Send + Sync>;

#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct ProcessError(pub String);

#[repr(C)]
#[derive(Clone, Default)]
pub struct ProcessHandle {
    pub context: Arc<Mutex<ProcessContext>>,
    pub handler_thread: Arc<Mutex<HandlerThread>>,
    pub input: ProcessInput,
    pub output: ProcessOutput,
}

impl ProcessHandle {
    pub fn background_thread(&mut self) -> Arc<Mutex<HandlerThread>> {
        self.handler_thread.clone()
    }
    pub fn context(&mut self) -> Arc<Mutex<ProcessContext>> {
        self.context.clone()
    }

    pub fn input(&mut self) -> ProcessInput {
        self.input.clone()
    }

    pub fn output(&mut self) -> ProcessOutput {
        self.output.clone()
    }
}

pub type ProcessFunc =
    dyn (FnMut(Arc<Mutex<ProcessHandle>>) -> Result<ProcessResult, ProcessError>) + Sync + Send;

#[repr(C)]
#[derive(Clone, Default)]
pub struct ProcessResult {
    pub resolved: bool,
    pub bracket_closing_before: Vec<Arc<Mutex<ProcessContext>>>,
    pub bracket_closing_after: Vec<Arc<Mutex<ProcessContext>>>,
    pub bracket_context: Option<HashMap<String, Vec<Arc<Mutex<ProcessContext>>>>>,
    pub data: Value,
    pub outputs: HashMap<String, HashMap<String, Vec<IP>>>,
}

impl Debug for ProcessResult {
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

#[derive(Clone, Default)]
pub struct ProcessInput {
    pub in_ports: InPorts,
    pub context: Arc<Mutex<ProcessContext>>,
    pub component: Arc<Mutex<Component>>,
    pub ip: IP,
    pub result: Arc<Mutex<ProcessResult>>,
    pub scope: Option<String>,
    pub port: InPort,
}

impl Debug for ProcessInput {
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

impl ProcessInput {
 
    /// When preconditions are met, set component state to `activated`
    pub fn activate(&mut self) {
        let activated =  self.context.clone().try_lock().unwrap().activated;
        if activated {
            return;
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
                    self.ip
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
    pub fn has(&mut self, port: &str, validator: Option<ValidatorFn>) -> bool {
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
            log!(Level::Error, "zflow::component => Unknown port");
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
                        if let Ok(component) = self.component.clone().try_lock().as_mut() {
                            if let Some(contexts) = component.get_bracket_context(
                                "in",
                                name.clone(),
                                self.scope.clone().unwrap(),
                                idx,
                            ) {
                                if let Some(context) = contexts.pop() {
                                    context.clone().try_lock().unwrap().close_ip = Some(ip.clone());
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

#[derive(Clone, Default)]
pub struct ProcessOutput {
    pub out_ports: OutPorts,
    pub context: Arc<Mutex<ProcessContext>>,
    pub component: Arc<Mutex<Component>>,
    pub ip: IP,
    pub result: Arc<Mutex<ProcessResult>>,
    pub scope: Option<String>,
}
impl Debug for ProcessOutput {
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

impl ProcessOutput {

    /// Sends an error object
    pub fn error(&mut self, err: &dyn Any) -> Result<(), ProcessError> {
        let errors = if let Some(err) = err.downcast_ref::<ProcessError>() {
            vec![err.clone()]
        } else if let Some(err) = err.downcast_ref::<Vec<ProcessError>>() {
            err.clone()
        } else {
            return Err(ProcessError(format!("Invalid error value")));
        };

        if self.out_ports.ports.contains_key("error") {
            if let Some(error_port) = self.out_ports.ports.get("error") {
                if error_port.is_attached(None) || !error_port.is_required() {
                    if errors.len() > 1 {
                        self.send_ip(
                            "error",
                            &IP::new(IPType::OpenBracket(Value::Null), IPOptions::default()),
                        )?;
                    }
                    errors
                        .iter()
                        .for_each(|err| { self.send_ip("error", &Value::String(err.0.clone())).expect("expected process to send value"); });
                    if errors.len() > 1 {
                        self.send_ip(
                            "error",
                            &IP::new(IPType::CloseBracket(Value::Null), IPOptions::default()),
                        )?;
                    }
                }
            }
        }

        Err(ProcessError(format!(
            "errors {}",
            errors
                .iter()
                .map(|e| e.0.clone())
                .collect::<Vec<String>>()
                .join(",\n")
        )))
    }
    /// Sends a single IP object to a port.
    ///
    /// `packet` can be type of `IP` or `Value`
    pub fn send_ip(&mut self, port: &str, packet: &dyn Any) -> Result<(), ProcessError> {
        
        let mut ip = if let Some(ip) = packet.downcast_ref::<IP>() {
            ip.clone()
        } else if let Some(data) = packet.downcast_ref::<IPType>() {
            IP::new(data.clone(), IPOptions::default())
        } else if let Some(data) = packet.downcast_ref::<Value>() {
            if let Ok(ip) = IP::deserialize(data) {
                ip
            } else {
                IP::new(IPType::Data(data.clone()), IPOptions::default())
            }
        } else if let Some(res) = packet.downcast_ref::<ProcessResult>() {
            IP::new(IPType::Data(res.clone().data), IPOptions::default())
        } else {
                return Ok(());
        };

        if self.scope.is_some() && ip.scope.is_empty() {
            ip.scope = self.scope.clone().unwrap();
        }
        
       
        if let Ok(component) = self.component.clone().try_lock().as_mut() {
            if !component.get_outports().ports.contains_key(port) {
                return Err(ProcessError(format!(
                    "Node {} does not have outport {}",
                    component.get_node_id(),
                    port
                )));
            }
            if let Some(port_impl) = self.out_ports.ports.get_mut(port) {
                if port_impl.is_addressable() && ip.index.is_none() {
                    return Err(ProcessError(format!(
                        "Sending packets to addressable port {} {} requires specifying index",
                        component.get_node_id(),
                        port
                    )));
                }
                if component.is_ordered() {
                    component.add_to_result(self.result.clone(), port.to_string(), &mut ip, false);
                    return Ok(());
                }
                if !port_impl.options.scoped {
                    ip.scope = "".to_string();
                }
                _ = port_impl.send_ip(&ip, ip.index, true);
            }
        }
        Ok(())
    }

    /// Sends the argument via `send()` and marks activation as `done()`
    pub fn send_done(&mut self, packet: &dyn Any)-> Result<(), ProcessError> {
        self.send(packet)?;
        self.done(None);
        Ok(())
    }
    /// Sends packets for each port as a key in a map, tupple,
    /// or sends Error or a list of Errors if passed such
    ///
    /// Accepts either `ProcessError`, `Vec<ProcessError>`, tupple `(&str, Value)`, tupple `(&str, IPType)`  or an output map `Map<String, Value>`
    pub fn send(&mut self, packet: &dyn Any) -> Result<(), ProcessError>  {
        if let Some(err) = packet.downcast_ref::<ProcessError>() {
            return self.error(err);
        } else if let Some(err) = packet.downcast_ref::<Vec<ProcessError>>() {
            return self.error(err);
        }

        let component_ports = &mut vec![];
        let mut maps_in_ports = false;
        let out_ports = self.out_ports.clone();
        if let Some(v) = packet.downcast_ref::<Value>() {
            if let Some(ports) = v.as_object() {
                out_ports.ports.keys().for_each(|port| {
                    if (port != "error") && (port != "ports") {
                        component_ports.push(port);
                    }
                    if !maps_in_ports && ports.contains_key(port.clone().as_str()) {
                        maps_in_ports = true;
                    }
                });
    
                if (component_ports.len() == 1) && !maps_in_ports {
                    return self.send_ip(component_ports.clone()[0], &json!(ports.clone()));
                }
                if (component_ports.len() > 1) && !maps_in_ports {
                    return Err(ProcessError(format!("Port must be specified for sending output")));
                }
                for port in out_ports.ports.keys() {
                    let key = port.clone();
                    let packet = ports.get(&key).unwrap();
                    self.send_ip(port, packet).expect(format!("expected to send IP to port {}", port).as_str());
                }
            }
            return Ok(())
        }
       
        
        if let Some(ports) = packet.downcast_ref::<HashMap<&str, Value>>() {
            out_ports.ports.keys().for_each(|port| {
                if (port != "error") && (port != "ports") {
                    component_ports.push(port);
                }
                if !maps_in_ports && ports.contains_key(port.clone().as_str()) {
                    maps_in_ports = true;
                }
            });

            if (component_ports.len() == 1) && !maps_in_ports {
                return self.send_ip(component_ports.clone()[0], &json!(ports.clone()));
            }
            if (component_ports.len() > 1) && !maps_in_ports {
                return Err(ProcessError(format!("Port must be specified for sending output")));
            }
            for port in out_ports.ports.keys() {
                let key = port.clone();
                let packet = ports.get(key.as_str()).unwrap();
                self.send_ip(port, packet).expect(format!("expected to send IP to port {}", port).as_str());
            }
            return Ok(())
        }

        if let Some(ports) = packet.downcast_ref::<HashMap<String, Value>>() {
            out_ports.ports.keys().for_each(|port| {
                if (port != "error") && (port != "ports") {
                    component_ports.push(port);
                }
                if !maps_in_ports && ports.contains_key(port.clone().as_str()) {
                    maps_in_ports = true;
                }
            });

            if (component_ports.len() == 1) && !maps_in_ports {
                return self.send_ip(component_ports.clone()[0], &json!(ports.clone()));
            }
            if (component_ports.len() > 1) && !maps_in_ports {
                return Err(ProcessError(format!("Port must be specified for sending output")));
            }
            for port in out_ports.ports.keys() {
                let key = port.clone();
                let packet = ports.get(key.as_str()).unwrap();
                self.send_ip(port, packet).expect(format!("expected to send IP to port {}", port).as_str());
            }
            return Ok(())
        }
        
        if let Some((port, data)) = packet.downcast_ref::<(&str, Value)>() {
            return self.send_ip(*port, data);
        }
        if let Some((port, data)) = packet.downcast_ref::<(&str, _)>() {
            return self.send_ip(*port, *data);
        }
        if let Some((port, data)) = packet.downcast_ref::<(&str, IPType)>() {
            return self.send_ip(*port, &IP::new(data.clone(), IPOptions::default()));
        }
        for port in self.out_ports.clone().ports.keys() {
            self.send_ip(port, packet)?;
        }
        Ok(())
    }

    /// Sends data you don't want to serialize as JSON values. Uses Bincode to encode data
    pub fn send_buffer<T>(&mut self, port:&str, packet: T) -> Result<(), ProcessError> where T:Serialize {
        match bincode::serialize(&packet) {
            Ok(v) => {
                let ip = IP::new(IPType::Buffer(v), IPOptions::default());
                self.send_ip(port, &ip)
            }
            Err(x) =>{
                Err(ProcessError(format!("{:?}", x)))
            }
        }
    }

    /// Finishes process activation gracefully
    pub fn done(&mut self, err: Option<&dyn Any>) {
        self.result.clone().try_lock().unwrap().resolved = true;
        if let Ok(component) = self.component.clone().try_lock().as_mut() {
            component.activate(self.context.clone());
        }
        if let Some(err) = err {
            if let Some(err) = err.downcast_ref::<ProcessError>() {
                let _ = self.error(err);
            } else if let Some(err) = err.downcast_ref::<Vec<ProcessError>>() {
                let _ = self.error(err);
            }
        }

        if let Ok(component) = self.component.clone().try_lock().as_mut() {
            let mut _component = component.clone();
            let mut is_last = || {
                // We only care about real output sets with processing data
                let results_only = _component
                    .get_output_queue_mut()
                    .iter()
                    .filter(|q| {
                        if let Ok(q) = q.try_lock().as_mut() {
                            if !q.resolved {
                                return true;
                            }

                            if !q.bracket_closing_after.is_empty() {
                                return false;
                            }
                        }
                        false
                    })
                    .collect::<VecDeque<&Arc<Mutex<ProcessResult>>>>();
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

            log!(
                Level::Debug,
                "zflow::component {} finished processing {}",
                component.get_node_id(),
                component.get_load()
            );
          
            component.deactivate(self.context.clone());
        }
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct ProcessContext {
    pub close_ip: Option<IP>,
    pub ip: IP,
    pub component: Arc<Mutex<Component>>,
    pub ports: Vec<String>,
    pub source: String,
    pub data: Value,
    pub result: Arc<Mutex<ProcessResult>>,
    pub activated: bool,
    pub deactivated: bool,
    pub scope: Option<String>,
    pub load: usize,
}

impl Default for ProcessContext {
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
            load: Default::default()
        }
    }
}

impl Debug for ProcessContext {
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

impl ProcessContext {

    pub fn activate(_context: Arc<Mutex<ProcessContext>>, _component: Arc<Mutex<Component>>) {
        if let Ok(context) = _context.clone().try_lock().as_mut() {
            if let Ok(component) = _component.clone().try_lock().as_mut() {
                if context.result.clone().try_lock().unwrap().resolved
                    || component
                        .get_output_queue()
                        .iter()
                        .find(|ctx| Arc::ptr_eq(ctx, &context.result))
                        .is_some()
                {
                    context.result = Arc::new(Mutex::new(ProcessResult::default()));
                }
                component.activate(_context)
            }
        }
    }

    pub fn deactivate(_context: Arc<Mutex<ProcessContext>>, _component: Arc<Mutex<Component>>) {
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
