use std::{
    any::Any,
    sync::{Arc, Mutex},
};

use fp_rust::publisher::Publisher;
use futures::executor::block_on;
use serde::Deserialize;
use serde_json::{Map, Value};

use std::fmt::Debug;
use std::marker::Send;

use crate::{ip::{IPOptions, IPType, IP}, network::NetworkProcess};

#[derive(Debug, Clone)]
pub struct SocketConnection {
    pub process: NetworkProcess,
    pub port: String,
    pub index: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub enum SocketEvent {
    Connect(Option<usize>),
    Disconnect(Option<usize>),
    Attach(Option<usize>),
    Detach(usize),
    Data(IP, Option<usize>),
    BeginGroup(Value, Option<usize>),
    EndGroup(Option<usize>),
    IP(IP, Option<usize>),
    Error(String, Option<usize>),
    #[default]
    Undefined,
}

/// ## Internal Sockets
///
/// The default communications mechanism between ZFlow processes is
/// an _internal socket_, which is responsible for accepting information
/// packets sent from processes' outports, and emitting corresponding
/// events so that the packets can be caught to the inport of the
/// connected process.

#[derive(Clone)]
pub struct InternalSocket {
    pub index: usize,
    pub connected: bool,
    pub to: Option<SocketConnection>,
    pub from: Option<SocketConnection>,
    pub metadata: Option<Map<String, Value>>,
    pub data_delegate: Option<Arc<Mutex<dyn (FnMut() -> IP) + Sync + Send>>>,
    pub(crate) bus: Arc<Mutex<Publisher<SocketEvent>>>,
    pub brackets: Vec<Value>,
    pub errors: Vec<String>,
    debug: bool
}

impl Debug for InternalSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InternalSocket")
            .field("index", &self.index)
            .field("connected", &self.connected)
            .field("to", &self.to)
            .field("from", &self.from)
            .field("metadata", &self.metadata)
            .field("data_delegate", &"dyn (FnMut() -> Value")
            .field("brackets", &self.brackets)
            .field("errors", &self.errors)
            .field("listeners", &"[listeners]")
            .field("debug", &self.debug)
            .finish()
    }
}

impl InternalSocket {
    fn new(metadata: Option<Map<String, Value>>) -> Self {
        Self {
            to: None,
            from: None,
            bus: Arc::new(Mutex::new(Publisher::new())),
            index: 0,
            metadata,
            data_delegate: None,
            connected: false,
            brackets: Vec::new(),
            errors: Vec::new(),
            debug: false
        }
    }
    pub fn create(metadata: Option<Map<String, Value>>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new(metadata)))
    }

    pub fn set_debug(&mut self, active: bool){
        self.debug = active;
    }

    pub fn emit_event(&mut self, event: SocketEvent) {
        self.bus
            .clone()
            .try_lock()
            .expect("expected event bus")
            .publish(event);
    }

    pub fn on(&mut self, rec: impl FnMut(Arc<SocketEvent>) -> () + 'static + Send + Sync) {
        self.bus
            .clone()
            .try_lock()
            .expect("expected event bus")
            .subscribe_fn(rec);
    }

    pub fn get_id(&self) -> String {
        "".to_string()
    }
    /// ## Socket data delegation
    ///
    /// Sockets have the option to receive data from a delegate function
    /// should the `send` method receive undefined for `data`.  This
    ///  helps in the case of defaulting values.
    pub fn set_data_delegate<K>(&mut self, fun: K)
    where
        K: (FnMut() -> IP) + 'static + Sync + Send,
    {
        self.data_delegate = Some(Arc::new(Mutex::new(fun)))
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }
    /// ## Socket connections
    ///
    /// Sockets that are attached to the ports of processes may be
    /// either connected or disconnected. The semantical meaning of
    /// a connection is that the outport is in the process of sending
    /// data. Disconnecting means an end of transmission.
    ///
    /// This can be used for example to signal the beginning and end
    /// of information packets resulting from the reading of a single
    /// file or a database query.
    ///
    pub fn connect(&mut self) -> Result<(), String> {
        if self.is_connected() {
            return Ok(());
        }
        self.connected = true;
        self.emit_event(SocketEvent::Connect(None));
        Ok(())
    }
    pub fn disconnect(&mut self) -> Result<(), String> {
        if !self.is_connected() {
            return Ok(());
        }
        self.connected = false;
        self.emit_event(SocketEvent::Disconnect(None));
        Ok(())
    }
    /// ## Sending information packets
    ///
    /// The _send_ method is used by a processe's outport to
    /// send information packets. The actual packet contents are
    /// not defined by ZFlow, and may be any valid Json value.
    pub async fn send(&mut self, data: Option<&dyn Any>) -> Result<(), String> {
        if let Some(data_delegate) = self.data_delegate.clone() {
            if data.is_none() {
                let _data = data_delegate.try_lock().expect("expect data delegate")();
                return self.handle_socket_event(SocketEvent::Data(_data, None), true);
            }
        }

        let ip = if let Some(ip) = data.unwrap().downcast_ref::<IP>() {
            ip.clone()
        } else if let Some(data) = data.unwrap().downcast_ref::<IPType>() {
            IP::new(data.clone(), IPOptions::default())
        } else if let Some(data) = data.unwrap().downcast_ref::<Value>() {
            IP::new(IPType::Data(data.clone()), IPOptions::default())
        } else {
            panic!("packet type should be either IP or Value");
        };

        self.handle_socket_event(SocketEvent::Data(ip, None), true)
    }

    pub async fn send_defaults(&mut self) -> Result<(), String> {
        return self.send(None).await;
    }

    /// ## Sending information packets without open bracket
    ///
    /// As _connect_ event is considered as open bracket, it needs to be followed
    /// by a _disconnect_ event or a closing bracket. In the new simplified
    /// sending semantics single IP objects can be sent without open/close brackets.
    pub fn post(&mut self, mut packet: Option<IP>, auto_discconnect: bool) -> Result<(), String> {
        if packet.is_none() {
            if let Some(data_delegate) = self.data_delegate.clone() {
                packet = Some((data_delegate.try_lock().expect("expects data delegate"))());
            }
        }
        // Send legacy connect/disconnect if needed
        if !self.is_connected() && self.brackets.is_empty() {
            self.connect()?;
        }

        self.handle_socket_event(SocketEvent::IP(packet.unwrap(), None), false)?;

        if auto_discconnect && self.is_connected() && self.brackets.is_empty() {
            self.disconnect()?;
        }
        Ok(())
    }

    /// ## Information Packet grouping
    ///
    /// Processes sending data to sockets may also group the packets
    /// when necessary. This allows transmitting tree structures as
    /// a stream of packets.
    ///
    /// For example, an object could be split into multiple packets
    /// where each property is identified by a separate grouping:
    ///
    ///     # Group by object ID
    ///     out_ports.get("out").unwrap().begin_group(object.id);
    ///
    ///     for (property, value) in &object{
    ///       out_ports.get("out").unwrap().begin_group(property);
    ///       out_ports.get("out").unwrap().send(value);
    ///       out_ports.get("out").unwrap().end_group();
    ///     }
    ///     out_ports.get("out").unwrap().end_group();
    ///     
    ///
    /// This would cause a tree structure to be sent to the receiving
    /// process as a stream of packets. So, an article object may be
    /// as packets like:
    ///
    /// * `/<article id>/title/Lorem ipsum`
    /// * `/<article id>/author/Henri Bergius`
    ///
    /// Components are free to ignore groupings, but are recommended
    /// to pass received groupings onward if the data structures remain
    /// intact through the component's processing.
    pub fn begin_group(&mut self, group: Value) -> Result<(), String> {
        self.handle_socket_event(SocketEvent::BeginGroup(group, None), true)
    }
    pub fn end_group(&mut self) -> Result<(), String> {
        self.handle_socket_event(SocketEvent::EndGroup(None), true)
    }

    fn handle_socket_event(&mut self, event: SocketEvent, autoconnect: bool) -> Result<(), String> {
        if !self.is_connected() && autoconnect && self.brackets.is_empty() {
            // connect before sending
            self.connect()?;
        }

        let mut ip: Option<IP> = None;

        match event.clone() {
            SocketEvent::EndGroup(_) => {
                // Prevent closing already closed groups
                if self.brackets.is_empty() {
                    return Ok(());
                }
                // Add group name to bracket
                ip = Some(IP::new(
                    IPType::Data(self.brackets.pop().expect("expected data")),
                    IPOptions::default(),
                ));
            }
            SocketEvent::BeginGroup(data, _) => {
                self.brackets.push(data);
            }
            SocketEvent::IP(_ip, _) => {
                let mut _ip = _ip;
                let datatype = _ip.clone().datatype;
                match datatype {
                    IPType::OpenBracket(data) => {
                        self.brackets.push(data);
                    }
                    IPType::CloseBracket(_) => {
                        // Prevent closing already closed brackets
                        if self.brackets.is_empty() {
                            return Ok(());
                        }
                        self.brackets.pop().unwrap();
                    }
                    _ => {}
                }
                ip = Some(_ip);
            }
            SocketEvent::Data(data, _) => {
                ip = Some(data);
            }
            SocketEvent::Connect(_) => self.connected = true,
            SocketEvent::Disconnect(_) => self.connected = false,
            SocketEvent::Error(err, i) => {
                if let Some(idx) = i {
                    self.errors.insert(idx, err);
                } else {
                    self.errors.insert(0, err);
                }
            }
            _ => {}
        }

        // Emit the IP Object
        if let Some(ip) = ip {
            self.emit_event(SocketEvent::IP(ip.clone(), ip.index));
            return Ok(());
        }

        self.emit_event(event);
        Ok(())
    }
}
