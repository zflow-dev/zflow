use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    fmt::{Debug, Error},
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
};

use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
use fp_rust::{common::SubscriptionFunc, publisher::Publisher};
use log::{log, Level};
use serde_json::{json, Value};

use crate::{
    ip::{IPOptions, IPType, IP},
    port::{
        normalize_port_name, BasePort, InPort, InPorts, InPortsOptions, OutPort, OutPorts,
        OutPortsOptions, PortsTrait,
    },
    process::{
        ProcessContext, ProcessError, ProcessFunc, ProcessInput, ProcessOutput, ProcessResult,
    },
    sockets::SocketEvent,
};

#[derive(Debug, Clone, Default)]
pub struct BracketContext<T: ComponentTrait> {
    pub r#in: HashMap<String, HashMap<String, Vec<Arc<Mutex<ProcessContext<T>>>>>>,
    pub out: HashMap<String, HashMap<String, Vec<Arc<Mutex<ProcessContext<T>>>>>>,
}

#[derive(Debug)]
pub enum ComponentEvent {
    Activate(usize),
    Deactivate(usize),
    Start,
    End,
}

pub trait ComponentCallbacks {
    /// ### Setup
    /// Provide the setUp function to be called for component-specific initialization.
    /// Called at network start-up.
    fn setup(&mut self, setup_fn: impl FnMut() -> Result<(), String> + Send + Sync + 'static);

    /// ### Teardown
    /// Provide the setUp function to be called for component-specific cleanup.
    /// Called at network shutdown.
    fn teardown(&mut self, teardown_fn: impl FnMut() -> Result<(), String> + Send + Sync + 'static);

    fn on(
        &self,
        callback: impl FnMut(Arc<ComponentEvent>) -> () + Send + Sync + 'static,
    ) -> Arc<SubscriptionFunc<ComponentEvent>>;
}

/// Base Component trait that provides a way to instantiate and extend ZFLow components
pub trait BaseComponentTrait
where
    Self::Comp: ComponentTrait,
{
    type Handle;
    type Comp;

    fn get_name(&self) -> Option<String>;
    fn get_node_id(&self) -> String;
    fn set_node_id(&mut self, id: String);
    fn get_description(&self) -> String;
    fn get_icon(&self) -> String;
    fn set_icon(&mut self, icon: String);
    fn get_handle(&self) -> Self::Handle;
    fn set_handle(
        &mut self,
        handle: impl FnMut(
                Arc<Mutex<ProcessContext<Self::Comp>>>,
                Arc<Mutex<ProcessInput<Self::Comp>>>,
                Arc<Mutex<ProcessOutput<Self::Comp>>>,
            ) -> Result<ProcessResult<Self::Comp>, ProcessError>
            + Sync
            + Send
            + 'static,
    );

    fn prepare_forwarding(&mut self) {
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
    fn get_inports(&self) -> InPorts;
    fn get_inports_mut(&mut self) -> &mut InPorts;
    fn get_outports(&self) -> OutPorts;
    fn get_outports_mut(&mut self) -> &mut OutPorts;
    /// Method for checking whether the component sends packets
    /// in the same order they were received.
    fn is_ordered(&self) -> bool;
    fn get_auto_ordering(&self) -> bool;
    fn set_auto_ordering(&mut self, auto_ordering: bool);
    fn get_ordered(&self) -> bool;
    fn is_forwarding_outport(&self, inport: &dyn Any, outport: &dyn Any) -> bool {
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
    fn is_forwarding_inport(&self, port: &dyn Any) -> bool {
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
    fn get_output_queue(&self) -> VecDeque<Arc<Mutex<ProcessResult<Self::Comp>>>>;
    fn get_output_queue_mut(&mut self) -> &mut VecDeque<Arc<Mutex<ProcessResult<Self::Comp>>>>;
    fn get_load(&self) -> usize;
    fn set_load(&mut self, load: usize);
    /// Get the current bracket forwarding context for an IP object
    fn get_bracket_context(
        &mut self,
        _type: &str,
        port: String,
        scope: String,
        idx: Option<usize>,
    ) -> Option<&mut Vec<Arc<Mutex<ProcessContext<Self::Comp>>>>> {
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
            .get_bracket_context_val().r#in.contains_key(&name.clone()) {
                self
                .get_bracket_context_val_mut()
                    .r#in
                    .insert(name.clone(), HashMap::new());
            }
            if let Some(obj) = self
            .get_bracket_context_val_mut().r#in.get_mut(&name.clone()) {
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
            .get_bracket_context_val().out.contains_key(&name.clone()) {
                self
                .get_bracket_context_val_mut()
                    .out
                    .insert(name.clone(), HashMap::new());
            }
            if let Some(obj) = self
            .get_bracket_context_val_mut().out.get_mut(&name.clone()) {
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
    fn get_bracket_context_val(&self) -> BracketContext<Self::Comp>;
    fn get_bracket_context_val_mut(&mut self) -> &mut BracketContext<Self::Comp>;
    /// Get contexts that can be forwarded with this in/outport
    /// pair.
    fn get_forwardable_contexts(
        &mut self,
        inport: String,
        outport: String,
        contexts: Vec<Arc<Mutex<ProcessContext<Self::Comp>>>>,
    ) -> Vec<Arc<Mutex<ProcessContext<Self::Comp>>>> {
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

    fn get_forward_brackets(&self) -> HashMap<String, Vec<String>>;
    fn get_forward_brackets_mut(&mut self) -> &mut HashMap<String, Vec<String>>;

    fn add_bracket_forwards(&mut self, result: Arc<Mutex<ProcessResult<Self::Comp>>>) {
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
                    if let Some(context) = context.clone().try_lock().ok() {
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
                        let mut unforwarded: Vec<Arc<Mutex<ProcessContext<Self::Comp>>>> = vec![];
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
                    if let Some(context) = context.clone().try_lock().ok() {
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
    fn add_to_result(
        &mut self,
        result: Arc<Mutex<ProcessResult<Self::Comp>>>,
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
    fn process_output_queue(&mut self) {
        while !self.get_output_queue().is_empty() {
            if !&self.get_output_queue()[0].clone().try_lock().unwrap().resolved {
                break;
            }

            if let Some(result) = self.get_output_queue_mut().pop_front().as_mut() {
                self.add_bracket_forwards(result.clone());
                if let Ok(result) = result.clone().try_lock() {
                    result.outputs
                    .keys()
                    .for_each(|port| {
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
                                    self.get_outports_mut().ports.get_mut(port).unwrap().send_ip(
                                        ip,
                                        (*ip).index,
                                        true,
                                    );
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
                                self.get_outports_mut().ports.get_mut(port).unwrap().send_ip(
                                    ip,
                                    None,
                                    true,
                                );
                            });
                        });
                    });
                }
            }
        }
    }
    /// Signal that component has deactivated. There may be multiple
    /// deactivated contexts at the same time
    fn deactivate(&mut self, context: Arc<Mutex<ProcessContext<Self::Comp>>>) {
        if let Ok(context) = context.clone().try_lock().as_mut() {
            if context.deactivated {
                return;
            }

            context.activated = false;
            context.deactivated = true;

            self.set_load(self.get_load() - 1);

            if self.is_ordered() {
                self.process_output_queue();
            }

            self.get_publisher()
                .clone()
                .try_lock()
                .unwrap()
                .publish(ComponentEvent::Deactivate(self.get_load()));
        }
    }
    /// Signal that component has activated. There may be multiple
    /// activated contexts at the same time
    fn activate(&mut self, context: Arc<Mutex<ProcessContext<Self::Comp>>>) {
        if let Ok(context) = context.clone().try_lock().as_mut() {
            if context.activated {
                return;
            }

            context.activated = true;
            context.deactivated = false;

            self.set_load(self.get_load() + 1);

            self.get_publisher()
                .try_lock()
                .unwrap()
                .publish(ComponentEvent::Activate(self.get_load()));

            if self.is_ordered() || self.get_auto_ordering() {
                self.get_output_queue_mut().push_back(context.result.clone());
            }
        }
    }

    /// ### Start
    ///
    /// Called when network starts. This calls the setUp
    /// method and sets the component to a started state.
    fn start(&mut self) -> Result<(), String> {
        if let Some(setup_fn) = &self.get_setup_function() {
            if let Ok(setup_fn) = setup_fn.clone().try_lock().as_mut() {
                (setup_fn)()?;
            }
        }
        self.set_started(true);
        self.get_publisher()
            .clone()
            .try_lock()
            .unwrap()
            .publish(ComponentEvent::Start);
        Ok(())
    }
    fn set_started(&mut self, started:bool);
    fn is_started(&self) -> bool;
    fn is_subgraph(&self) -> bool;
    fn is_ready(&self) -> bool;

    fn get_teardown_function(
        &self,
    ) -> Option<Arc<Mutex<dyn FnMut() -> Result<(), String> + Send + Sync + 'static>>>;

    fn get_setup_function(
        &self,
    ) -> Option<Arc<Mutex<dyn FnMut() -> Result<(), String> + Send + Sync + 'static>>>;

    /// Convenience method to find all the event subscribers for this component: Used internally
    fn get_subscribers(&self) -> Vec<Arc<SubscriptionFunc<ComponentEvent>>>;
    fn get_subscribers_mut(&mut self) -> &mut Vec<Arc<SubscriptionFunc<ComponentEvent>>>;
    /// Convenience method to access the internal event publisher for this component: Used internally
    fn get_publisher(&self) -> Arc<Mutex<Publisher<ComponentEvent>>>;

    /// Housekeeping function to reset this component;
    fn reset(&mut self) {
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

    /// Clear bracket context
    fn clear_bracket_context(&mut self);

    /// Component error manager
    fn error(
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

    fn to_dyn(&self) -> &dyn Any;
    fn to_dyn_mut(&mut self) -> &mut dyn Any;
}

pub trait ComponentTrait:
    BaseComponentTrait<Handle = Arc<Mutex<ProcessFunc<Self>>>, Comp = Self>
    + ComponentCallbacks
    + Sync
    + Send
    + Clone
    + Default
    + 'static
{
}

/// ZFlow Process Component
#[derive(Clone)]
pub struct Component {
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
    pub bracket_context: BracketContext<Self>,
    pub component_name: Option<String>,
    pub node_id: String,
    /// Queue for handling ordered output packets
    pub output_q: VecDeque<Arc<Mutex<ProcessResult<Self>>>>,
    pub base_dir: Option<String>,
    /// Initially the component is not started
    pub started: bool,
    pub load: usize,
    pub(crate) handle: Arc<Mutex<ProcessFunc<Self>>>,
    pub(crate) bus: Arc<Mutex<Publisher<ComponentEvent>>>,
    pub auto_ordering: bool,
    setup_fn: Option<Arc<Mutex<dyn FnMut() -> Result<(), String> + Send + Sync + 'static>>>,
    teardown_fn: Option<Arc<Mutex<dyn FnMut() -> Result<(), String> + Send + Sync + 'static>>>,
    tracked_signals: Vec<Arc<SubscriptionFunc<ComponentEvent>>>,
}

impl BaseComponentTrait for Component {
    type Handle = Arc<Mutex<ProcessFunc<Self>>>;
    type Comp = Component;

    fn set_started(&mut self, started:bool) {
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
        false
    }
    fn is_ready(&self) -> bool {
        return true;
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

    fn get_handle(&self) -> Self::Handle {
        self.handle.clone()
    }

    fn set_handle(
        &mut self,
        handle: impl FnMut(
                Arc<Mutex<ProcessContext<Self::Comp>>>,
                Arc<Mutex<ProcessInput<Self::Comp>>>,
                Arc<Mutex<ProcessOutput<Self::Comp>>>,
                // Arc<Mutex<ProcessContext<T>>>,
            ) -> Result<ProcessResult<Self::Comp>, ProcessError>
            + Sync
            + Send
            + 'static,
    ) {
        self.handle = Arc::new(Mutex::new(handle));
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

    fn to_dyn(&self) -> &dyn Any {
        self as &dyn Any
    }

    fn to_dyn_mut(&mut self) -> &mut dyn Any {
        self as &mut dyn Any
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
}

impl ComponentCallbacks for Component {
    /// ### Setup
    /// Provide the setUp function to be called for component-specific initialization.
    /// Called at network start-up.
    fn setup(&mut self, setup_fn: impl FnMut() -> Result<(), String> + Send + Sync + 'static) {
        self.setup_fn = Some(Arc::new(Mutex::new(setup_fn)));
    }

    /// ### Teardown
    /// Provide the setUp function to be called for component-specific cleanup.
    /// Called at network shutdown.
    fn teardown(
        &mut self,
        teardown_fn: impl FnMut() -> Result<(), String> + Send + Sync + 'static,
    ) {
        self.teardown_fn = Some(Arc::new(Mutex::new(teardown_fn)));
    }

    /// Subscribe to component's events
    fn on(
        &self,
        callback: impl FnMut(Arc<ComponentEvent>) -> () + Send + Sync + 'static,
    ) -> Arc<SubscriptionFunc<ComponentEvent>> {
        self.bus.clone().try_lock().unwrap().subscribe_fn(callback)
    }
}

impl ComponentTrait for Component {}

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
            handle: Arc::new(Mutex::new(|_, __, ___| return Ok(ProcessResult::default()))),
            bus: Default::default(),
            auto_ordering: Default::default(),
            setup_fn: Default::default(),
            teardown_fn: Default::default(),
            tracked_signals: Default::default(),
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
    pub fn new(options: ComponentOptions<Self>) -> Arc<Mutex<Self>> {
        let component = Arc::new(Mutex::new(Self {
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
            base_dir: None,
            started: false,
            load: 0,
            handle: Arc::new(Mutex::new(Box::new(|_, __, ___| {
                Ok(ProcessResult::default())
            }))),
            auto_ordering: false,
            node_id: String::from(""),
            bus: Arc::new(Mutex::new(Publisher::new())),
            setup_fn: None,
            teardown_fn: None,
            tracked_signals: Vec::new(),
        }));

        let _ = Self::link_process(component.clone(), options.process);

        component.clone()
    }

    /// ### Shutdown
    ///
    /// Called when network is shut down. This calls the
    /// teardown function and sets the component back to a
    /// non-started state.
    pub fn shutdown<T>(_component: Arc<Mutex<T>>) -> Result<(), String>
    where
        T: ComponentTrait + 'static,
    {
        if let Ok(component) = _component.clone().try_lock().as_mut() {
            // Tell the component that it is time to shut down
            if let Some(teardown_fn) = component.get_teardown_function().clone() {
                if let Ok(teardown_fn) = teardown_fn.clone().try_lock().as_mut() {
                    (teardown_fn)()?;
                }
            }
            thread::spawn(move || {
                if let Ok(component) = _component.clone().try_lock().as_mut() {
                    while component.get_load() > 0 {
                        // Some in-flight processes, wait for them to finish
                    }
                    component.get_subscribers().iter().for_each(|signal| {
                        component
                            .get_publisher()
                            .try_lock()
                            .unwrap()
                            .unsubscribe(signal.clone());
                    });
                    component.reset();
                }
            });
        }
        Ok(())
    }

    /// Sets process handler function
    pub fn link_process<T>(
        component: Arc<Mutex<T>>,
        mut process_func: impl FnMut(
                Arc<Mutex<ProcessContext<T>>>,
                Arc<Mutex<ProcessInput<T>>>,
                Arc<Mutex<ProcessOutput<T>>>,
            ) -> Result<ProcessResult<T>, ProcessError>
            + Sync
            + Send
            + 'static,
    ) where
        T: ComponentTrait + Debug,
    {
        if let Ok(_component) = component.clone().try_lock().as_mut() {
            _component.prepare_forwarding();
            _component.set_handle(move |c, i, o| (process_func)(c.clone(), i.clone(), o.clone()));
            _component.get_inports().ports.keys().for_each(move |name| {
                let mut port = _component.get_inports_mut().ports.get_mut(name).unwrap();
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
    }

    /// ### Handling IP objects
    ///
    /// The component has received an Information Packet. Call the
    /// processing function so that firing pattern preconditions can
    /// be checked and component can do processing as needed.
    pub fn handle_ip<T>(_component: Arc<Mutex<T>>, ip: IP, mut port: InPort)
    where
        T: ComponentTrait + Debug,
    {
        // Initialize the result object for situations where output needs
        // to be queued to be kept in order
        let mut result = Arc::new(Mutex::new(ProcessResult::<T>::default()));
        let mut inports = InPorts::default();
        let mut outports = OutPorts::default();

        if let Ok(component) = _component.clone().try_lock().as_mut() {
            if !port.options.triggering {
                // If port is non-triggering, we can skip the process function call
                return;
            }

            match ip.datatype {
                IPType::OpenBracket(_) => {
                    if !component.get_auto_ordering() && !component.get_ordered() {
                        // Switch component to ordered mode when receiving a stream unless
                        // auto-ordering is disabled
                        log!(
                            Level::Debug,
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
                                        if let Some(bracket_ctx) =
                                            _bracket_ctx.try_lock().ok().as_mut()
                                        {
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

            inports = component.get_inports();
            outports = component.get_outports();
        }

        // We have to free the component's mutex, so that we can use it in the process function

        // Prepare the input/output pair
        let context = Arc::new(Mutex::new(ProcessContext::<T> {
            ip: ip.clone(),
            result: result.clone(),
            activated: false,
            deactivated: true,
            ports: vec![port.clone().name],
            close_ip: None,
            source: "".to_owned(),
            data: Value::Null,
            scope: None,
            component: _component.clone(),
        }));
        let input = Arc::new(Mutex::new(ProcessInput::<T> {
            in_ports: inports,
            context: context.clone(),
            component: _component.clone(),
            ip: ip.clone(),
            port: port.clone(),
            scope: None,
            result: result.clone(),
        }));
        let output = Arc::new(Mutex::new(ProcessOutput::<T> {
            out_ports: outports,
            context: context.clone(),
            component: _component.clone(),
            ip: ip.clone(),
            scope: None,
            result: result.clone(),
        }));

        // Call the processing function
        let handle_binding = _component
            .clone()
            .try_lock()
            .as_mut()
            .expect("expected component instance")
            .get_handle()
            .clone();
        let mut handle_binding = handle_binding.try_lock();
        let handle = handle_binding
            .as_mut()
            .expect("Processing function is not defined");
        let res = (handle)(context.clone(), input.clone(), output.clone());
        if res.is_ok() {
            output.clone().try_lock().unwrap().send_done(&res.ok());
        } else {
            if res.clone().err().is_some() {
                output
                    .clone()
                    .try_lock()
                    .unwrap()
                    .done(Some(res.err().as_ref().unwrap()));
            } else {
                output.clone().try_lock().unwrap().done(None);
            }
        }
        if let Ok(component) = _component.clone().try_lock().as_mut() {
            if context.clone().try_lock().unwrap().activated {
                return;
            }
            // If receiving an IP object didn't cause the component to
            // activate, log that input conditions were not met
            if port.clone().is_addressable() {
                log!(
                    Level::Debug,
                    "zflow::component => {} packet on '{}[{}]' didn't match preconditions: {:?}",
                    component.get_node_id(),
                    port.clone().name,
                    ip.clone().index.unwrap(),
                    ip.clone().datatype
                );
                return;
            }
            log!(
                Level::Debug,
                "zflow::component => {} packet on '{}' didn't match preconditions: {:?}",
                component.get_node_id(),
                port.clone().name,
                ip.clone().datatype
            );
        }
    }
    //  pub fn once(&mut self, event: &str, callback: EventCallback) {}
}

pub struct ComponentOptions<T: ComponentTrait> {
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
    pub process: Box<ProcessFunc<T>>,
}

impl<T> Default for ComponentOptions<T>
where
    T: ComponentTrait,
{
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
            process: Box::new(|_, __, ___| Ok(ProcessResult::default())),
        }
    }
}

impl<T> Debug for ComponentOptions<T>
where
    T: ComponentTrait,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentOptions")
            .field("in_ports", &self.in_ports)
            .field("out_ports", &self.out_ports)
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
