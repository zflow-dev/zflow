use std::{
    any::Any,
    borrow::BorrowMut,
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex}, ffi,
};

use is_url::is_url;
use once_cell::sync::OnceCell;
use ruwren::{
    create_module, get_slot_checked, BasicFileLoader, Class, FunctionSignature, Module,
    ModuleLibrary, ModuleScriptLoader, VMConfig, VMWrapper, VM,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{
    component::{Component, ComponentOptions, GraphDefinition, ModuleComponent},
    ip::IPType,
    port::{InPort, OutPort, PortOptions},
    process::{ProcessError, ProcessOutput, ProcessResult},
};

impl GraphDefinition for WrenComponent {
    fn to_any(&self) -> &dyn Any {
        Box::leak(Box::new(self.clone())) as &dyn Any
    }
}

fn default_base_dir() -> String {
    "/".to_string()
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct WrenComponent {
    pub name: String,
    pub inports: HashMap<String, PortOptions>,
    pub outports: HashMap<String, PortOptions>,
    #[serde(default)]
    /// Set the default component description
    pub description: String,
    #[serde(default)]
    /// Set the default component icon
    pub icon: String,
    #[serde(default)]
    /// Whether the component should keep send packets
    /// out in the order they were received
    pub ordered: bool,
    #[serde(default)]
    /// Whether the component should activate when it receives packets
    pub activate_on_input: bool,
    #[serde(default)]
    /// Bracket forwarding rules. By default we forward
    pub forward_brackets: HashMap<String, Vec<String>>,
    #[serde(default = "default_base_dir")]
    /// Base directory of wren packages
    pub base_dir: String,
    /// Path to wren source
    pub source: String,
    #[serde(default)]
    pub package_id: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl WrenComponent {
    pub fn from_metadata(meta: Value) -> Option<WrenComponent> {
        WrenComponent::deserialize(meta).ok()
    }

    pub fn with_metadata(&mut self, meta: Value) -> WrenComponent {
        if let Some(meta) = WrenComponent::from_metadata(meta.clone()) {
            self.inports.extend(meta.inports);
            self.outports.extend(meta.outports);
            if !meta.description.is_empty() {
                self.description = meta.description;
            }
            if !meta.icon.is_empty() {
                self.icon = meta.icon;
            }
            self.forward_brackets.extend(meta.forward_brackets);
            if !meta.base_dir.is_empty() {
                self.base_dir = meta.base_dir;
            }
        } else if let Some(meta) = meta.clone().as_object() {
            self.metadata = meta.clone();
        }

        self.clone()
    }
}

pub struct ZFlowModule;
impl Class for ZFlowModule {
    fn initialize(_: &VM) -> Self
    where
        Self: Sized,
    {
        return ZFlowModule;
    }
}

impl ModuleComponent for WrenComponent {
    fn as_component(&self) -> Result<Component, String> {
        let base_dir = self.base_dir.clone();
        let mut code = PathBuf::from(self.base_dir.clone());

        let source = if is_url(&self.source) || self.base_dir != "/" {
            code.push(self.source.clone());
            fs::read_to_string(code).expect("Could not read wren source")
        } else {
            self.source.clone()
        };

        let mut inports = self.inports.clone();
        let mut outports = self.outports.clone();
        if inports.is_empty() {
            inports.insert("in".to_owned(), PortOptions::default());
        }
        if outports.is_empty() {
            outports.insert("out".to_owned(), PortOptions::default());
        }

        return Ok(Component::new(ComponentOptions {
            metadata: Some(self.metadata.clone()),
            in_ports: HashMap::from_iter(
                inports
                    .clone()
                    .iter()
                    .map(|(key, options)| (key.clone(), InPort::new(options.clone())))
                    .collect::<Vec<_>>(),
            ),
            out_ports: HashMap::from_iter(
                outports
                    .clone()
                    .iter()
                    .map(|(key, options)| (key.clone(), OutPort::new(options.clone())))
                    .collect::<Vec<_>>(),
            ),
            description: self.description.clone(),
            icon: self.icon.clone(),
            ordered: self.ordered,
            activate_on_input: self.activate_on_input,
            forward_brackets: self.forward_brackets.clone(),
            graph: None,
            process: Some(Box::new(move |handle| {
                let inputs: Vec<&String> = inports.keys().collect();
                let handle_binding = handle.clone();
                let mut handle_binding = handle_binding.try_lock();
                let this = handle_binding
                    .as_mut()
                    .map_err(|_| ProcessError(String::from("Process Handle has dropped")))?;

                let outports = this.output().clone();
                let script_loader = BasicFileLoader::new().base_dir(base_dir.clone());

                static mut PROCESS_OUTPUT: OnceCell<ProcessOutput> = OnceCell::new();

                // Create an internal zflow module to be acessed from the wren runtime
                unsafe {
                    PROCESS_OUTPUT.take();
                    PROCESS_OUTPUT
                        .set(outports.clone())
                        .expect("expected process output");

                    impl ZFlowModule {
                        fn send(vm: &VM) {
                            let port = get_slot_checked!(vm => string 1);
                            let value = to_json_value(vm, 2);
                            let mut ip = Map::new();
                            ip.insert(port, value);
                            if let Some(process) = unsafe { PROCESS_OUTPUT.get() } {
                                process.clone().send(&ip).unwrap();
                            }
                        }
                        fn done(vm: &VM) {
                            let error = if let Some(err) = vm.get_slot_string(1) {
                                let val = ProcessError(err);
                                Some(val)
                            } else {
                                None
                            };
                         
                            if let Some(process) = unsafe { PROCESS_OUTPUT.get() } {
                                process.clone().done(error.as_ref().map(|v| v as &dyn Any));
                            }
                        }
                    }
                }
                create_module! {
                    class("ZFlow") crate::wren::ZFlowModule => _zflow {
                        static(fn "send", 2) send,
                        static(fn "done", 1) done
                    }
                    module => zflow
                }

                // Add the internal module as a wren library 
                let mut lib = ModuleLibrary::new();
                zflow::publish_module(&mut lib);

                // Tell wren how to find this foreign class
                struct ZFlowLoader;
                impl ModuleScriptLoader for ZFlowLoader {
                    fn load_script(&mut self, name: String) -> Option<String> {
                        if name == "zflow" {
                            Some(
                                "
                                class ZFlow {
                                    foreign static send(port, data)
                                    foreign static done(error)
                                }
                                "
                                .into(),
                            )
                        } else {
                            None
                        }
                    }
                }

                let vm = VMConfig::new()
                    .library(&lib)
                    .enable_relative_import(true)
                    .script_loader(script_loader)
                    .script_loader(ZFlowLoader)
                    .build();

                vm.interpret("main", source.clone()).map_err(|err| {
                    println!("err => {:?}", err);
                    ProcessError(format!(
                        "Could not interpret Wren source: {}",
                        err.to_string()
                    ))
                })?;

                let _inputs: HashMap<&String, Value> = HashMap::from_iter(
                    inputs
                        .clone()
                        .iter()
                        .map(|port| {
                            let value = this.input().get(*port);
                            if let Some(value) = value {
                                return (
                                    port.clone(),
                                    match value.datatype {
                                        IPType::Data(v) => v,
                                        _ => Value::Null,
                                    },
                                );
                            }
                            return (port.clone(), Value::Null);
                        })
                        .collect::<Vec<_>>(),
                );

                let mapped_inputs = json!(_inputs);

                let handle = vm.make_call_handle(FunctionSignature::new_function("process", 1));

                vm.execute(|vm| {
                    vm.ensure_slots(2);
                    vm.get_variable("main", "Component", 0);
                    to_wren_value(vm, 1, mapped_inputs);
                });

                vm.call_handle(&handle).map_err(|err| {
                    println!("err => {:?}", err);
                    ProcessError(format!("Wren runtime error: {}", err.to_string()))
                })?;

                return Ok(ProcessResult::default());
            })),
        }));
    }
}

fn to_wren_value(vm: &VM, slot: usize, value: Value) {
    match value {
        Value::Null => {
            vm.set_slot_null(slot);
        }
        Value::Bool(b) => {
            vm.set_slot_bool(slot, b);
        }
        Value::Number(d) => {
            vm.set_slot_double(slot, d.as_f64().unwrap());
        }
        Value::String(s) => {
            vm.set_slot_string(slot, s);
        }
        Value::Array(a) => {
            vm.set_slot_new_list(slot);
            vm.ensure_slots(2);
            for (index, value) in a.iter().enumerate() {
                to_wren_value(vm, 1, value.clone());
                vm.insert_in_list(slot, index as i32, 1);
            }
        }
        Value::Object(m) => {
            vm.set_slot_new_map(slot);
            for (k, v) in m {
                vm.ensure_slots(3);
                vm.set_slot_string(slot + 1, k);
                to_wren_value(vm, slot + 2, v);
                vm.set_map_value(slot, slot + 1, slot + 2);
            }
        }
    }
}

fn to_json_value(vm: &VM, slot: usize) -> Value {
    let data_type = vm.get_slot_type(slot);
    return match data_type {
        ruwren::SlotType::Num => {
            let val = get_slot_checked!(vm => num slot);
            json!(val)
        }
        ruwren::SlotType::Bool => {
            let val = get_slot_checked!(vm => bool slot);
            json!(val)
        }
        ruwren::SlotType::List => {
            let mut value = vec![];
            if let Some(count) = vm.get_list_count(slot) {
                for i in 0..count {
                    vm.get_list_element(slot, i as i32, slot + 1);
                    value.push(to_json_value(vm, slot + 1))
                }
            }
            json!(value)
        }
        ruwren::SlotType::Map => {
            // Todo: figure out a way to serialize wren map to json/rust map
            Value::Null
        }
        ruwren::SlotType::Null => Value::Null,
        ruwren::SlotType::String => {
            let val = get_slot_checked!(vm => string slot);
            json!(val)
        }
        ruwren::SlotType::Foreign => Value::Null,
        ruwren::SlotType::Unknown => Value::Null,
    };
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zflow_graph::Graph;

    use crate::network::{BaseNetwork, Network, NetworkOptions};

    #[test]
    fn create_wren_component() {
        let mut base_dir = std::env::current_dir().unwrap();
        base_dir.push("test_components");
        let base_dir = base_dir.to_str().unwrap();

        let mut graph = Graph::new("", false);
        graph
            .add_node("zflow", "add_wren", None)
            .add_initial(json!(1), "zflow", "left", None)
            .add_initial(json!(2), "zflow", "right", None);

        let mut network = Network::create(
            graph.clone(),
            NetworkOptions {
                subscribe_graph: false,
                delay: true,
                base_dir: base_dir.to_string(),
                ..Default::default()
            },
        );

        if let Ok(nw) = network.connect().unwrap().try_lock().as_mut() {
            nw.start().unwrap();
        }
    }
}
