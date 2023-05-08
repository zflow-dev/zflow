use std::{
    any::Any,
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex},
};

use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};

use foreach::ForEach;
use fp_rust::{common::SubscriptionFunc, publisher::Publisher};
use futures::{executor::block_on, Future};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt::Debug;

use crate::{
    errors::async_transform,
    ip::{IPOptions, IPType, IP},
    process::ValidatorFn,
    sockets::{InternalSocket, SocketEvent}, component::Component,
};

// #[dyn_cast(PortTrait, BasePort)]
pub trait PortTrait {}

pub trait BasePort: PortTrait + Debug {
    fn attach(&mut self, socket: Arc<Mutex<InternalSocket>>, index: Option<usize>);
    fn attach_socket(&mut self, socket: Arc<Mutex<InternalSocket>>, idx: usize);
    fn detach(&mut self, socket_id: usize);
    fn is_attached(&self, socket_id: Option<usize>) -> bool;
    fn list_attached(&self) -> Vec<usize>;
    fn is_addressable(&self) -> bool;
    fn is_buffered(&self) -> bool;
    fn is_required(&self) -> bool;
    fn is_connected(&self, socket_id: Option<usize>) -> Result<bool, String>;
    fn can_attach(&self) -> bool;
    fn get_id(&self) -> String;
    fn get_schema(&self) -> String;
    fn get_description(&self) -> String;
    fn get_data_type(&self) -> IPType;
    fn to_dyn(&self) -> &dyn Any;
    fn find_socket(&self, socket_id: usize) -> Option<&Arc<Mutex<InternalSocket>>>;
}

pub trait PortsTrait {
    type Target;
    fn add(&mut self, name: &str, port: &dyn Any) -> Result<(), String>;
    fn get(&self, name: &str) -> Option<&Self::Target>;
    fn get_mut(&mut self, name: &str) -> Option<&mut Self::Target>;
    fn remove(&mut self, name: &str);
}

const fn _default_true() -> bool { true }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortOptions {
    #[serde(default)]
    pub addressable: bool,
    #[serde(default)]
    pub buffered: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub caching: bool,
    #[serde(default)]
    pub data_type: IPType,
    #[serde(default)]
    pub user_data: Value,
    #[serde(default)]
    pub control: bool,
    #[serde(default = "_default_true")]
    pub triggering: bool,
    #[serde(default)]
    pub scoped: bool,
}

impl Default for PortOptions {
    fn default() -> Self {
        Self {
            addressable: false,
            buffered: Default::default(),
            required: false,
            schema: Default::default(),
            description: Default::default(),
            caching: false,
            data_type: IPType::All(Value::default()),
            user_data: Value::default(),
            control: false,
            triggering: true,
            scoped: false,
        }
    }
}

#[derive(Clone, Default)]
pub struct InPort {
    pub node: String,
    pub (crate) node_instance: Option<Arc<Mutex<Component>>>,
    pub name: String,
    pub options: PortOptions,
    pub sockets: Vec<Arc<Mutex<InternalSocket>>>,
    pub buffer: Arc<Mutex<Vec<IP>>>,
    pub indexed_buffer: Arc<Mutex<HashMap<usize, Arc<Mutex<Vec<IP>>>>>>,
    pub scoped_buffer: Arc<Mutex<HashMap<String, Arc<Mutex<Vec<IP>>>>>>,
    pub indexed_scoped_buffer: Arc<Mutex<HashMap<String, HashMap<usize, Arc<Mutex<Vec<IP>>>>>>>,
    pub indexed_iip_buffer: Arc<Mutex<HashMap<usize, Arc<Mutex<Vec<IP>>>>>>,
    pub iip_buffer: Arc<Mutex<Vec<IP>>>,
    pub(crate) bus: Arc<Mutex<Publisher<SocketEvent>>>,
    pub(crate) subscribers: Vec<Arc<SubscriptionFunc<SocketEvent>>>,
}

impl Debug for InPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut binding = f.debug_struct("OutPort");
        binding
            .field("node", &self.node)
            .field("name", &self.name)
            .field("options", &self.options)
            .field("sockets", &self.sockets)
            .field("buffer", &self.buffer)
            .field("scoped_buffer", &self.scoped_buffer)
            .field("iip_buffer", &self.iip_buffer)
            .field("node_instance", &"[Component]");

        binding.finish()
    }
}

// #[dyn_cast(PortTrait, BasePort)]
impl PortTrait for InPort {}

impl BasePort for InPort {
    fn attach(&mut self, socket: Arc<Mutex<InternalSocket>>, index: Option<usize>) {
        let mut idx = index;
        if !self.is_addressable() || index.is_none() {
            idx = Some(self.sockets.len());
        }

        if let Ok(socket) = socket.clone().try_lock().as_mut().as_mut() {
            socket.index = idx.unwrap();
            let _ = socket.connect();
            let bus = self.bus.clone();
            let buffer = self.buffer.clone();
            let scoped_buffer = self.scoped_buffer.clone();
            let iip_buffer = self.iip_buffer.clone();
            let is_addressable = self.is_addressable();
            let options = self.options.clone();
            let node = self.node.clone();
            socket.on(move |event| match event.clone().as_ref() {
                SocketEvent::IP(ip, index) => {
                    let mut ip = ip.clone();
                    match ip.datatype {
                        IPType::Data(_) => {}
                        _ => {
                            if options.control {
                                return;
                            }
                        }
                    }

                    ip.owner = Some(node.clone());

                    if is_addressable {
                        ip.index = index.clone();
                    }

                    if !options.schema.is_empty() && ip.schema.is_empty() {
                        ip.schema = options.clone().schema;
                    }

                    if ip.initial {
                        if is_addressable {
                            iip_buffer
                                .clone()
                                .try_lock()
                                .expect("expected iip buffer instance")
                                .insert(
                                    index.expect("expected index for addressable port"),
                                    ip.clone(),
                                );
                        } else {
                            iip_buffer
                                .clone()
                                .try_lock()
                                .expect("expected iip buffer instance")
                                .push(ip.clone());
                        }
                        match ip.datatype {
                            IPType::Data(_) => {}
                            _ => {
                                if options.control
                                    && iip_buffer
                                        .clone()
                                        .try_lock()
                                        .expect("expected iip buffer instance")
                                        .len()
                                        > 1
                                {
                                    iip_buffer
                                        .clone()
                                        .try_lock()
                                        .expect("expected iip buffer instance")
                                        .remove(0);
                                }
                            }
                        }
                        bus.clone()
                            .try_lock()
                            .expect("expected inport bus instance")
                            .publish(SocketEvent::IP(ip.clone(), *index));
                        return;
                    }
                    if !ip.clone().scope.is_empty() {
                        if let Ok(scoped_buffer) = scoped_buffer.clone().try_lock().as_mut() {
                            if let Some(scopes) = scoped_buffer.get_mut(&ip.scope) {
                                if let Ok(scopes) = scopes.clone().try_lock().as_mut() {
                                    if is_addressable {
                                        scopes.insert(
                                            index.expect("expected index for addressable port"),
                                            ip.clone(),
                                        );
                                    } else {
                                        scopes.push(ip.clone());
                                    }
                                    match ip.datatype {
                                        IPType::Data(_) => {}
                                        _ => {
                                            if options.control && scopes.len() > 1 {
                                                scopes.remove(0);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        bus.clone()
                            .try_lock()
                            .expect("expected inport bus instance")
                            .publish(SocketEvent::IP(ip.clone(), *index));
                        return;
                    }
                    if is_addressable {
                        buffer
                            .clone()
                            .try_lock()
                            .expect("expected publisher")
                            .insert(
                                index.expect("expected index for addressable port"),
                                ip.clone(),
                            );
                    } else {
                        buffer
                            .clone()
                            .try_lock()
                            .expect("expected publisher")
                            .push(ip.clone());
                    }

                    match ip.datatype {
                        IPType::Data(_) => {}
                        _ => {
                            if options.control
                                && buffer.clone().try_lock().expect("expected publisher").len() > 1
                            {
                                buffer
                                    .clone()
                                    .try_lock()
                                    .expect("expected publisher")
                                    .remove(0);
                            }
                        }
                    }
                    bus.clone()
                        .try_lock()
                        .expect("expected inport bus instance")
                        .publish(SocketEvent::IP(ip.clone(), *index));
                }
                _ => bus
                    .clone()
                    .try_lock()
                    .expect("expected inport bus instance")
                    .publish(event.as_ref().clone()),
            });
        }

        self.attach_socket(socket.clone(), idx.unwrap());
        if self.is_addressable() {
            self.emit(SocketEvent::Attach(idx));
            return;
        }
        self.emit(SocketEvent::Attach(None));
    }

    fn attach_socket(&mut self, socket: Arc<Mutex<InternalSocket>>, idx: usize) {
        // has default value
        if self.has_default() {
            let default_value = match self.options.data_type.clone() {
                IPType::All(data) | IPType::Data(data) => data,
                _ => Value::Null,
            };
            socket
                .try_lock()
                .as_mut()
                .expect("expected socket instance")
                .set_data_delegate(move || {
                    IP::new(IPType::Data(default_value.clone()), IPOptions::default())
                });
        }
        self.sockets.insert(idx, socket.clone());
    }

    fn is_addressable(&self) -> bool {
        return self.options.addressable;
    }

    fn detach(&mut self, socket_id: usize) {
        self.sockets
            .clone()
            .iter_mut()
            .enumerate()
            .for_each(|(i, _)| {
                if i == socket_id {
                    self.sockets.remove(socket_id);
                    self.bus
                        .clone()
                        .try_lock()
                        .as_mut()
                        .expect("expected instance of publisher")
                        .publish(SocketEvent::Detach(socket_id));
                }
            });
    }

    fn is_buffered(&self) -> bool {
        self.options.buffered
    }

    fn is_required(&self) -> bool {
        self.options.required
    }

    fn can_attach(&self) -> bool {
        true
    }

    fn find_socket(&self, socket_id: usize) -> Option<&Arc<Mutex<InternalSocket>>> {
        self.sockets.get(
            self.sockets
                .clone()
                .iter()
                .position(|sock| {
                    sock.clone()
                        .try_lock()
                        .expect("Cannot find socket instance")
                        .index
                        == socket_id
                })
                .expect("Cannot find socket at index"),
        )
    }

    fn is_connected(&self, socket_id: Option<usize>) -> Result<bool, String> {
        if self.is_addressable() {
            if socket_id.is_none() {
                return Err(format!("{}: Socket ID required", self.get_id()));
            }
            if let Some(socket) = self.find_socket(socket_id.unwrap()) {
                return Ok(socket
                    .clone()
                    .try_lock()
                    .expect("expected socket instance")
                    .is_connected());
            } else {
                return Err(format!(
                    "{}: Socket ID {} is not available",
                    self.get_id(),
                    socket_id.unwrap()
                ));
            }
        }

        let mut connected = false;
        self.sockets.iter().foreach(|socket, _| {
            if socket
                .try_lock()
                .expect("expected socket instance")
                .is_connected()
            {
                connected = true;
            }
        });
        return Ok(connected);
    }

    fn get_id(&self) -> String {
        if self.node.is_empty() || self.name.is_empty() {
            return "Port".to_string();
        }
        format!("{} {}", self.node, self.name.to_uppercase())
    }

    fn get_schema(&self) -> String {
        self.options.schema.clone()
    }

    fn get_description(&self) -> String {
        self.options.description.clone()
    }

    fn is_attached(&self, socket_id: Option<usize>) -> bool {
        if self.is_addressable() && socket_id.is_some() {
            if self.find_socket(socket_id.unwrap()).is_some() {
                return true;
            }
            return false;
        }
        if self.sockets.is_empty() {
            return false;
        }

        true
    }
    fn get_data_type(&self) -> IPType {
        self.options.data_type.clone()
    }

    fn list_attached(&self) -> Vec<usize> {
        let mut attached = Vec::new();
        for socket in self.sockets.clone() {
            attached.push(socket.clone().try_lock().unwrap().index);
        }

        attached
    }

    fn to_dyn(&self) -> &dyn Any {
        self as &dyn Any
    }
}

impl InPort {
    pub fn new(options: PortOptions) -> Self {
        Self {
            node: "".to_string(),
            node_instance: None,
            name: "".to_string(),
            options,
            sockets: Vec::new(),
            buffer: Arc::new(Mutex::new(Vec::new())),
            bus: Arc::new(Mutex::new(Publisher::new())),
            scoped_buffer: Arc::new(Mutex::new(HashMap::new())),
            indexed_scoped_buffer: Arc::new(Mutex::new(HashMap::new())),
            iip_buffer: Arc::new(Mutex::new(Vec::new())),
            indexed_iip_buffer: Arc::new(Mutex::new(HashMap::new())),
            indexed_buffer: Arc::new(Mutex::new(HashMap::new())),
            subscribers: Vec::new(),
        }
    }
    pub fn has_default(&self) -> bool {
        match self.options.data_type.clone() {
            IPType::Data(data) | IPType::All(data) => {
                if data.is_null() {
                    return false;
                }
                true
            }
            _ => false,
        }
    }

    pub fn on(&mut self, fun: impl FnMut(Arc<SocketEvent>) -> () + 'static + Send + Sync) {
        self.subscribers.push(
            self.bus
                .clone()
                .try_lock()
                .expect("expected inport bus instance")
                .subscribe_fn(fun),
        );
    }

    pub fn emit(&mut self, event: SocketEvent) {
        match event.clone() {
            SocketEvent::IP(ip, idx) => {
                self.handle_ip(ip, idx);
            }
            _ => self
                .bus
                .clone()
                .try_lock()
                .expect("expected inport bus instance")
                .publish(event.clone()),
        }
    }

    pub fn handle_ip(&mut self, mut ip: IP, index: Option<usize>) {
        match ip.datatype {
            IPType::Data(_) => {}
            _ => {
                if self.options.control {
                    return;
                }
            }
        }

        ip.owner = Some(self.node.clone());

        if self.is_addressable() {
            ip.index = index;
        }

        if !self.get_schema().is_empty() && ip.schema.is_empty() {
            ip.schema = self.get_schema();
        }

        if ip.initial {
            if self.is_addressable() {
                self.iip_buffer
                    .clone()
                    .try_lock()
                    .expect("expected iip buffer instance")
                    .insert(
                        index.expect("expected index for addressable port"),
                        ip.clone(),
                    );
            } else {
                self.iip_buffer
                    .clone()
                    .try_lock()
                    .expect("expected iip buffer instance")
                    .push(ip.clone());
            }
            match ip.datatype {
                IPType::Data(_) => {}
                _ => {
                    if self
                        .iip_buffer
                        .clone()
                        .try_lock()
                        .expect("expected iip buffer instance")
                        .len()
                        > 1
                    {
                        self.iip_buffer
                            .clone()
                            .try_lock()
                            .expect("expected iip buffer instance")
                            .remove(0);
                    }
                }
            }
            self.bus
                .clone()
                .try_lock()
                .expect("expected inport bus instance")
                .publish(SocketEvent::IP(ip.clone(), index));
            return;
        }
        if !ip.clone().scope.is_empty() {
            if let Ok(scoped_buffer) = self.scoped_buffer.clone().try_lock().as_mut() {
                if let Some(scopes) = scoped_buffer.get_mut(&ip.scope) {
                    if let Ok(scopes) = scopes.clone().try_lock().as_mut() {
                        if self.is_addressable() {
                            scopes.insert(
                                index.expect("expected index for addressable port"),
                                ip.clone(),
                            );
                        } else {
                            scopes.push(ip.clone());
                        }
                        match ip.datatype {
                            IPType::Data(_) => {}
                            _ => {
                                if scopes.len() > 1 {
                                    scopes.remove(0);
                                }
                            }
                        }
                    }
                }
            }
            self.bus
                .clone()
                .try_lock()
                .expect("expected inport bus instance")
                .publish(SocketEvent::IP(ip.clone(), index));
            return;
        }
        if self.is_addressable() {
            self.buffer
                .clone()
                .try_lock()
                .expect("expected publisher")
                .insert(
                    index.expect("expected index for addressable port"),
                    ip.clone(),
                );
        } else {
            self.buffer
                .clone()
                .try_lock()
                .expect("expected publisher")
                .push(ip.clone());
        }

        match ip.datatype {
            IPType::Data(_) => {}
            _ => {
                if self.options.control
                    && self
                        .buffer
                        .clone()
                        .try_lock()
                        .expect("expected publisher")
                        .len()
                        > 1
                {
                    self.buffer
                        .clone()
                        .try_lock()
                        .expect("expected publisher")
                        .remove(0);
                }
            }
        }

        self.bus
            .clone()
            .try_lock()
            .expect("expected inport bus instance")
            .publish(SocketEvent::IP(ip.clone(), index));
    }

    pub fn get_buffer(
        &mut self,
        scope: Option<String>,
        index: Option<usize>,
        initial: bool,
    ) -> Option<Arc<Mutex<Vec<IP>>>> {
        if self.is_addressable() {
            if self.options.scoped {
                if let Some(scope) = scope.as_ref() {
                    if !self
                        .indexed_scoped_buffer
                        .clone()
                        .try_lock()
                        .unwrap()
                        .contains_key(scope)
                    {
                        return None;
                    }

                    if let Some(index) = index.as_ref() {
                        if self
                            .indexed_scoped_buffer
                            .clone()
                            .try_lock()
                            .unwrap()
                            .get(scope)
                            .unwrap()
                            .get(index)
                            .is_none()
                        {
                            return None;
                        }
                    }

                    let v = self
                        .indexed_scoped_buffer
                        .clone()
                        .try_lock()
                        .unwrap()
                        .get(scope)
                        .unwrap()
                        .get(&index.unwrap())
                        .cloned();
                    return v;
                }
            }
            if initial {
                if let Some(index) = index.as_ref() {
                    if let Some(indexed_iips) = self
                        .indexed_iip_buffer
                        .clone()
                        .try_lock()
                        .unwrap()
                        .get(index)
                    {
                        return Some(indexed_iips.clone());
                    }
                }
            }

            if let Some(index) = index.as_ref() {
                if let Some(indexed_buffer) =
                    self.indexed_buffer.clone().try_lock().unwrap().get(index)
                {
                    return Some(indexed_buffer.clone());
                }
            }
        }

        if self.options.scoped {
            if let Some(scope) = scope.as_ref() {
                if let Some(scope_buffer) =
                    self.scoped_buffer.clone().try_lock().unwrap().get(scope)
                {
                    return Some(scope_buffer.clone());
                }
            }
        }

        if initial {
            return Some(self.iip_buffer.clone());
        }
        Some(self.buffer.clone())
    }

    fn get_from_buffer(
        &mut self,
        scope: Option<String>,
        index: Option<usize>,
        initial: bool,
    ) -> Option<IP> {
        let buf = self.get_buffer(scope, index, initial);
        if let Some(buf) = buf.clone() {
            if let Ok(buf) = buf.clone().try_lock().as_mut() {
                if self.options.control && buf.len() > 0 {
                    return buf.get(buf.len() - 1).cloned();
                }
                if buf.len() > 0 {
                    return Some(buf.remove(0));
                }
                return None;
            }
        }
        None
    }

    /// Fetches a packet from the port
    pub fn get(&mut self, scope: Option<String>, index: Option<usize>) -> Option<IP> {
        let res = self.get_from_buffer(scope, index, false);
        if res.is_none() {
            return self.get_from_buffer(None, index, true);
        }
        res
    }

    pub(crate) fn has_iip_in_buffer(
        &mut self,
        scope: Option<String>,
        index: Option<usize>,
        validator: &mut ValidatorFn,
        initial: bool,
    ) -> bool {
        if let Some(buf) = self.get_buffer(scope, index, initial) {
            if let Ok(buf) = buf.clone().try_lock() {
                for i in 0..buf.len() {
                    if (validator)(buf[i].clone()) {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub(crate) fn has_iip(&mut self, index: Option<usize>, validator: &mut ValidatorFn) -> bool {
        self.has_iip_in_buffer(None, index, validator, true)
    }

    pub fn has(
        &mut self,
        scope: Option<String>,
        index: Option<usize>,
        mut validator: Option<ValidatorFn>,
    ) -> bool {
        if let Some(validator) = validator.as_mut() {
            if self.has_iip_in_buffer(scope, index, validator, false) {
                return true;
            } else if self.has_iip(index, validator) {
                return true;
            }
        }

        false
    }
}

#[derive(Clone, Default)]
pub struct InPorts {
    pub ports: HashMap<String, InPort>,
    pub(crate) bus: Arc<Mutex<Publisher<(String, Value)>>>,
}

impl Debug for InPorts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InPorts")
            .field("ports", &self.ports)
            .field("bus", &"[signal_bus]")
            .finish()
    }
}

#[derive(Clone, Debug, Default)]
pub struct InPortsOptions {
    pub ports: HashMap<String, InPort>,
}

impl InPorts {
    pub fn new(options: InPortsOptions) -> Self {
        Self {
            ports: options.ports,
            bus: Arc::new(Mutex::new(Publisher::new())),
        }
    }
}

impl PortsTrait for InPorts {
    fn add(&mut self, name: &str, port: &dyn Any) -> Result<(), String> {
        let re = Regex::new(r"^[a-z0-9_\.//]+$").unwrap();
        if !re.is_match(name) {
            return Err(format!("Port names can only contain lowercase alphanumeric characters and underscores. '{}' not allowed", name));
        }

        // Remove previous implementation
        if self.ports.contains_key(name) {
            self.ports.remove(name);
        }

        if let Some(inport) = port.downcast_ref::<InPort>() {
            self.ports.insert(name.to_string(), inport.clone());
            self.bus
                .clone()
                .try_lock()
                .unwrap()
                .publish(("add".to_string(), json!(name)));
        }

        if let Some(options) = port.downcast_ref::<PortOptions>() {
            let new_port = InPort::new(options.clone());
            self.ports.insert(name.to_string(), new_port);
            self.bus
                .clone()
                .try_lock()
                .unwrap()
                .publish(("add".to_string(), json!(name)));
        }

        Ok(())
    }

    fn remove(&mut self, name: &str) {
        self.ports.remove(name);
        self.bus
            .clone()
            .try_lock()
            .unwrap()
            .publish(("remove".to_string(), json!(name)));
    }

    fn get(&self, name: &str) -> Option<&Self::Target> {
        self.ports.get(name)
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Self::Target> {
        self.ports.get_mut(name)
    }

    type Target = InPort;
}

#[derive(Clone, Default)]
pub struct OutPort {
    pub node: String,
    pub (crate) node_instance: Option<Arc<Mutex<Component>>>,
    pub name: String,
    pub options: PortOptions,
    pub sockets: Vec<Arc<Mutex<InternalSocket>>>,
    pub cache: HashMap<String, IP>,
    pub(crate) bus: Arc<Mutex<Publisher<SocketEvent>>>,
    pub(crate) subscribers: Vec<Arc<SubscriptionFunc<SocketEvent>>>,
}
impl Debug for OutPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut binding = f.debug_struct("OutPort");
        binding
            .field("node", &self.node)
            .field("name", &self.name)
            .field("options", &self.options)
            .field("sockets", &self.sockets)
            .field("listeners", &"[listeners]")
            .field("cache", &self.cache)
            .field("node_instance", &"[Component]");

        binding.finish()
    }
}

// #[dyn_cast(PortTrait, BasePort)]
impl PortTrait for OutPort {}
impl BasePort for OutPort {
    fn attach(&mut self, socket: Arc<Mutex<InternalSocket>>, index: Option<usize>) {
        let mut idx = index;
        if !self.is_addressable() || index.is_none() {
            idx = Some(self.sockets.len());
        }

        if let Ok(socket) = socket.clone().try_lock().as_mut() {
            socket.index = idx.unwrap();
            let _ = socket.connect().expect("expected to connect");
        }

        self.sockets.insert(idx.unwrap(), socket.clone());

        if let Ok(publisher) = self.bus.clone().try_lock().as_mut() {
            if self.is_addressable() {
                (*publisher).publish(SocketEvent::Attach(idx));
            } else {
                (*publisher).publish(SocketEvent::Attach(None));
            }
        }

        if self.is_caching() && self.cache.contains_key(&format!("{:?}", index)) {
            block_on(self.send(
                self.cache.clone().get(&format!("{:?}", index)).unwrap(),
                index,
            ))
            .expect("expected to send data");
        }
    }

    fn is_addressable(&self) -> bool {
        self.options.addressable
    }

    fn detach(&mut self, socket_id: usize) {
        self.sockets.clone().iter().enumerate().find(|(i, socket)| {
            if socket.clone().try_lock().unwrap().index == socket_id {
                self.sockets.remove(*i);
                self.bus
                    .clone()
                    .try_lock()
                    .as_mut()
                    .expect("expected instance of publisher")
                    .publish(SocketEvent::Detach(socket_id));
                return true;
            }
            false
        });
    }

    fn is_buffered(&self) -> bool {
        self.options.buffered
    }

    fn attach_socket(&mut self, __: Arc<Mutex<InternalSocket>>, _: usize) {
        // not needed for outport
    }

    fn is_required(&self) -> bool {
        self.options.required
    }

    fn can_attach(&self) -> bool {
        true
    }

    fn find_socket(&self, socket_id: usize) -> Option<&Arc<Mutex<InternalSocket>>> {
        self.sockets.get(
            self.sockets
                .clone()
                .iter()
                .position(|sock| {
                    sock.clone()
                        .try_lock()
                        .expect("Cannot find socket instance")
                        .index
                        == socket_id
                })
                .expect("Cannot find socket at index"),
        )
    }

    fn is_connected(&self, socket_id: Option<usize>) -> Result<bool, String> {
        if self.is_addressable() {
            if socket_id.is_none() {
                return Err(format!("{}: Socket ID required", self.get_id()));
            }
            if self.find_socket(socket_id.unwrap()).is_none() {}
            if let Some(socket) = self.find_socket(socket_id.unwrap()) {
                return Ok(socket
                    .try_lock()
                    .expect(&format!("expected socket of id {}", socket_id.unwrap()))
                    .is_connected());
            } else {
                return Err(format!(
                    "{}: Socket ID {} is not available",
                    self.get_id(),
                    socket_id.unwrap()
                ));
            }
        }

        let mut connected = false;
        self.sockets.iter().foreach(|socket, _| {
            if let Ok(socket) = socket.try_lock() {
                if socket.is_connected() {
                    connected = true;
                }
            }
        });
        return Ok(connected);
    }

    fn get_id(&self) -> String {
        if self.node.is_empty() || self.name.is_empty() {
            return "Port".to_string();
        }
        format!("{} {}", self.node, self.name.to_uppercase())
    }
    fn get_schema(&self) -> String {
        self.options.schema.clone()
    }

    fn get_description(&self) -> String {
        self.options.description.clone()
    }

    fn is_attached(&self, socket_id: Option<usize>) -> bool {
        if self.is_addressable() && socket_id.is_some() {
            if self.find_socket(socket_id.unwrap()).is_some() {
                return true;
            }
            return false;
        }
        if self.sockets.is_empty() {
            return false;
        }

        true
    }

    fn get_data_type(&self) -> IPType {
        self.options.data_type.clone()
    }

    fn list_attached(&self) -> Vec<usize> {
        let mut attached = Vec::new();

        for socket in self.sockets.clone() {
            println!("{}", socket.clone().try_lock().unwrap().index);
            attached.push(socket.clone().try_lock().unwrap().index);
        }

        attached
    }

    fn to_dyn(&self) -> &dyn Any {
        self as &dyn Any
    }
}

/// Outport are the way a node component sends Information Packets.
impl OutPort {
    pub fn new(options: PortOptions) -> Self {
        Self {
            node: "".to_string(),
            node_instance: None,
            name: "".to_string(),
            bus: Arc::new(Mutex::new(Publisher::new())),
            options,
            sockets: Vec::new(),
            cache: HashMap::new(),
            subscribers: Vec::new(),
        }
    }

    pub async fn connect(&mut self, index: Option<usize>) -> Result<(), String> {
        let mut futures: Vec<Pin<Box<dyn Future<Output = Result<(), String>>>>> = Vec::new();
        let sockets = self.get_sockets(index).expect("expected list of sockets");
        self.check_required()?;
        sockets.iter().for_each(|socket| {
            let socket = socket.clone();
            futures.push(Box::pin(async move {
                if let Ok(socket) = socket.try_lock().as_mut() {
                    socket.connect()?;
                }
                Ok(())
            }));
        });

        async_transform(futures).await
    }

    pub async fn send(&mut self, data: &dyn Any, index: Option<usize>) -> Result<(), String> {
        let ip = if let Some(ip) = data.downcast_ref::<IP>() {
            ip.clone()
        } else if let Some(data) = data.downcast_ref::<IPType>() {
            IP::new(data.clone(), IPOptions::default())
        } else if let Some(data) = data.downcast_ref::<Value>() {
            if let Ok(ip) = IP::deserialize(data) {
                ip
            } else {
                IP::new(IPType::Data(data.clone()), IPOptions::default())
            }
        } else {
            panic!("packet type should be either IP or Value");
        };

        let mut futures: Vec<Pin<Box<dyn Future<Output = Result<(), String>>>>> = Vec::new();
        let sockets = if let Ok(sockets) = self.get_sockets(index) {
            sockets
        } else {
            return Err("Expected list of sockets".to_string());
        };
        self.check_required()?;
        if self.is_caching() {
            if let Some(_data) = self.cache.get(&format!("{:?}", index)) {
                if !assert_json_matches_no_panic(&ip, _data, Config::new(CompareMode::Strict))
                    .is_ok()
                {
                    self.cache.insert(format!("{:?}", index), ip.clone());
                }
            } else {
                self.cache.insert(format!("{:?}", index), ip.clone());
            }
        }
        sockets.clone().iter().for_each(|socket| {
            let socket = socket.clone();
            let ip = ip.clone();
            futures.push(Box::pin(async move {
                if let Ok(socket) = socket.try_lock().as_mut() {
                    if let Ok(_) = socket.send(Some(&ip)).await {
                    } else {
                        return Err("Socket Send".to_string());
                    }
                }
                Ok(())
            }));
        });

        async_transform(futures).await
    }

    pub async fn begin_group(&mut self, group: Value, index: Option<usize>) -> Result<(), String> {
        let mut futures: Vec<Pin<Box<dyn Future<Output = Result<(), String>>>>> = Vec::new();
        let sockets = self.get_sockets(index).expect("expected list of sockets");
        self.check_required()?;
        sockets.iter().foreach(|socket, _| {
            let socket = socket.clone();
            let group = group.clone();
            futures.push(Box::pin(async move {
                if let Ok(socket) = socket.try_lock().as_mut() {
                    socket.begin_group(group)?;
                }
                Ok(())
            }));
        });

        async_transform(futures).await
    }
    pub async fn end_group(&mut self, index: Option<usize>) -> Result<(), String> {
        let mut futures: Vec<Pin<Box<dyn Future<Output = Result<(), String>>>>> = Vec::new();
        let sockets = self.get_sockets(index).expect("expected list of sockets");
        self.check_required()?;
        sockets.iter().foreach(|socket, _| {
            let socket = socket.clone();
            futures.push(Box::pin(async move {
                if let Ok(socket) = socket.try_lock().as_mut() {
                    socket.end_group()?;
                }
                Ok(())
            }))
        });

        async_transform(futures).await
    }

    pub async fn disconnect(&mut self, index: Option<usize>) -> Result<(), String> {
        let mut futures: Vec<Pin<Box<dyn Future<Output = Result<(), String>>>>> = Vec::new();
        let sockets = self.get_sockets(index).expect("expected list of sockets");
        self.check_required()?;
        sockets.iter().for_each(|socket| {
            let socket = socket.clone();
            futures.push(Box::pin(async move {
                if let Ok(socket) = socket.clone().try_lock().as_mut() {
                    socket.disconnect()?;
                }
                Ok(())
            }));
        });

        async_transform(futures).await
    }

    fn get_data_type(&self) -> IPType {
        self.options.data_type.clone()
    }

    pub fn send_ip(
        &mut self,
        value: &dyn Any,
        index: Option<usize>,
        auto_connect: bool,
    ) -> &mut Self {
        let mut ip = if let Some(ip) = value.downcast_ref::<IP>() {
            ip.clone()
        } else if let Some(data) = value.downcast_ref::<IPType>() {
            IP::new(data.clone(), IPOptions::default())
        } else if let Some(data) = value.downcast_ref::<Value>() {
            if let Ok(ip) = IP::deserialize(data) {
                ip
            } else {
                IP::new(IPType::Data(data.clone()), IPOptions::default())
            }
        } else {
            panic!("packet type should be either IP or Value");
        };
        let idx = index;
        ip.index = idx;

        let sockets = self.get_sockets(idx).expect("expected list of sockets");
        self.check_required().expect("check failed");

        if !self.get_schema().is_empty() && ip.schema.is_empty() {
            // Stamp non-specific IP objects with port schema
            ip.schema = self.get_schema();
        }

        if let Some(cached_data) = self.cache.get(&format!("{:?}", idx)) {
            if self.is_caching()
                && !assert_json_matches_no_panic(cached_data, &ip, Config::new(CompareMode::Strict))
                    .is_ok()
            {
                self.cache.insert(format!("{:?}", idx), ip.clone());
            }
        }

        let mut pristine = true;

        sockets.iter().for_each(move |socket| {
            if let Ok(socket) = socket.clone().try_lock().as_mut() {
                if pristine {
                    let _ = socket.post(Some(ip.clone()), auto_connect);
                    pristine = false;
                } else {
                    if ip.can_fake_clone() {
                        let _ = socket.post(Some(ip.clone()), auto_connect);
                    }
                }
            }
        });

        self
    }

    pub fn open_bracket(
        &mut self,
        data: Value,
        options: IPOptions,
        index: Option<usize>,
    ) -> &mut Self {
        self.send_ip(&IP::new(IPType::OpenBracket(data), options), index, true)
    }

    pub fn close_bracket(
        &mut self,
        data: Value,
        options: IPOptions,
        index: Option<usize>,
    ) -> &mut Self {
        self.send_ip(&IP::new(IPType::CloseBracket(data), options), index, true)
    }

    pub fn data(&mut self, data: Value, options: IPOptions, index: Option<usize>) -> &mut Self {
        self.send_ip(&IP::new(IPType::Data(data), options), index, true)
    }

    pub fn get_sockets(
        &self,
        index: Option<usize>,
    ) -> Result<Vec<Arc<Mutex<InternalSocket>>>, String> {
        // Addressable sockets affect only one connection at time
        if self.is_addressable() {
            if index.is_none() {
                return Err(format!("{} Socket ID required", self.get_id()));
            }
            let idx = index.unwrap();
            if self.find_socket(idx).is_none() {
                return Ok(Vec::new());
            }
            return Ok(vec![self.find_socket(idx).unwrap().clone()]);
        }
        if !self.is_addressable() && index.is_some() {
            return Err(format!("Not addressable"));
        }
        // Regular sockets affect all outbound connections
        Ok(self.sockets.clone())
    }

    fn is_caching(&self) -> bool {
        self.options.caching
    }

    fn check_required(&self) -> Result<(), String> {
        if self.sockets.is_empty() && self.is_required() {
            return Err(format!("{}: No connections available", self.get_id()));
        }
        Ok(())
    }

    pub fn on(&mut self, rec: impl FnMut(Arc<SocketEvent>) -> () + 'static + Send + Sync) {
        self.subscribers.push(
            self.bus
                .clone()
                .try_lock()
                .expect("expected outport event bus instance")
                .subscribe_fn(rec),
        );
    }
}

#[derive(Clone, Default)]
pub struct OutPorts {
    pub ports: HashMap<String, OutPort>,
    pub(crate) bus: Arc<Mutex<Publisher<(String, Value)>>>,
}

impl Debug for OutPorts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutPorts")
            .field("ports", &self.ports)
            .field("bus", &"[signal_bus]")
            .finish()
    }
}

#[derive(Clone, Default, Debug)]
pub struct OutPortsOptions {
    pub ports: HashMap<String, OutPort>,
}

impl OutPorts {
    pub fn new(options: OutPortsOptions) -> Self {
        options.ports.iter().for_each(|(name, _)| {
            if let Err(err) = validate_port_name(name.clone()) {
                panic!("{}", err);
            }
        });
        Self {
            ports: options.ports,
            bus: Arc::new(Mutex::new(Publisher::new()))
        }
    }

    pub async fn connect(&mut self, name: &str, socket_id: Option<usize>) -> Result<(), String> {
        let port = self
            .ports
            .get_mut(name)
            .expect(format!("Port {} not available", name).as_str());
        port.connect(socket_id).await
    }

    pub async fn disconnect(&mut self, name: &str, socket_id: Option<usize>) -> Result<(), String> {
        let port = self
            .ports
            .get_mut(name)
            .expect(format!("Port {} not available", name).as_str());
        port.disconnect(socket_id).await
    }

    pub async fn begin_group(
        &mut self,
        name: &str,
        group: Value,
        socket_id: Option<usize>,
    ) -> Result<(), String> {
        let port = self
            .ports
            .get_mut(name)
            .expect(format!("Port {} not available", name).as_str());
        port.begin_group(group, socket_id).await
    }
    pub async fn end_group(&mut self, name: &str, socket_id: Option<usize>) -> Result<(), String> {
        let port = self
            .ports
            .get_mut(name)
            .expect(format!("Port {} not available", name).as_str());
        port.end_group(socket_id).await
    }

    pub async fn send(
        &mut self,
        name: &str,
        data: &dyn Any,
        socket_id: Option<usize>,
    ) -> Result<(), String> {
        let port = self
            .ports
            .get_mut(name)
            .expect(format!("Port {} not available", name).as_str());
        let ip = if let Some(ip) = data.downcast_ref::<IP>() {
            ip.clone()
        } else if let Some(data) = data.downcast_ref::<IPType>() {
            IP::new(data.clone(), IPOptions::default())
        } else if let Some(data) = data.downcast_ref::<Value>() {
            IP::new(IPType::Data(data.clone()), IPOptions::default())
        } else {
            panic!("packet type should be either IP or Value");
        };
        port.send(&ip, socket_id).await
    }
}

impl PortsTrait for OutPorts {
    fn add(&mut self, name: &str, port: &dyn Any) -> Result<(), String> {
        let re = Regex::new(r"^[a-z0-9_\.//]+$").unwrap();
        if !re.is_match(name) {
            return Err(format!("Port names can only contain lowercase alphanumeric characters and underscores. '{}' not allowed", name));
        }

        // Remove previous implementation
        if self.ports.contains_key(name) {
            self.ports.remove(name);
        }

        if let Some(port) = port.downcast_ref::<OutPort>() {
            self.ports.insert(name.to_string(), port.clone());
            self.bus
                .clone()
                .try_lock()
                .unwrap()
                .publish(("add".to_string(), json!(name)));
        }

        if let Some(options) = port.downcast_ref::<PortOptions>() {
            let new_port = OutPort::new(options.clone());
            self.ports.insert(name.to_string(), new_port);
            self.bus
                .clone()
                .try_lock()
                .unwrap()
                .publish(("add".to_string(), json!(name)));
        }

        Ok(())
    }

    fn remove(&mut self, name: &str) {
        self.ports.remove(name);
        self.bus
            .clone()
            .try_lock()
            .unwrap()
            .publish(("remove".to_string(), json!(name)));
    }

    type Target = OutPort;

    fn get(&self, name: &str) -> Option<&Self::Target> {
        self.ports.get(name)
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Self::Target> {
        self.ports.get_mut(name)
    }
}

pub struct NormalizedPortName {
    pub name: String,
    pub index: Option<String>,
}
pub fn normalize_port_name(name: String) -> NormalizedPortName {
    let mut port = NormalizedPortName {
        name: name.clone(),
        index: None,
    };

    // Regular port
    if !name.contains("[") {
        return port;
    }

    // Addressable port with index
    let re = Regex::new(r"(.*)\[([0-9]+)\]").unwrap();
    if re.is_match(&name) {
        let captured = re
            .captures(&name)
            .expect("expected captured values from port name");
        port.name = captured[1].to_string();
        port.index = Some(captured[2].to_string());

        return port;
    }

    port
}

pub fn validate_port_name(name: String) -> Result<(), String> {
    let re = Regex::new(r"^[a-z0-9_\.//]+$").unwrap();
    if !re.is_match(&name) {
        return Err(format!("Port names can only contain lowercase alphanumeric characters and underscores. '{}' not allowed", name));
    }

    Ok(())
}
