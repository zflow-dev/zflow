use std::{collections::HashMap, path::PathBuf, str::FromStr, rc::Rc, cell::RefCell};

use extism::{
    manifest::Wasm, Context, CurrentPlugin, Error, Function, Manifest, Plugin, UserData, Val,
    ValType,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    component::{Component, ComponentOptions, ModuleComponent},
    port::{InPort, OutPort, PortOptions},
    process::{ProcessError, ProcessResult}, ip::{IP, IPType},
};

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct WasmComponent {
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
    /// Base directory of wasm sources
    pub base_dir: String,
    /// Path to wasm source
    pub source: String,
    pub package_id: String,
}

impl WasmComponent {
    pub fn from_metadata(meta: Value) -> Option<WasmComponent> {
        WasmComponent::deserialize(meta).ok()
    }

    pub fn with_metadata(&mut self, meta: Value) -> WasmComponent {
        if let Some(meta) = WasmComponent::deserialize(meta).ok() {
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
            
        }
        self.clone()
    }
}

impl ModuleComponent for WasmComponent {
    fn as_component(&self) -> Result<Component, String> {
        let source = self.source.clone();
        let mut code = PathBuf::from(self.base_dir.clone());
        code.push(source);
        let code = code.as_os_str();


        if let Some(source_code) = code.to_str() {
            let manifest = Manifest::new([Wasm::file(source_code.clone().to_string())]);
            let mut inports = self.inports.clone();
            let mut outports = self.outports.clone();
            if inports.is_empty() {
                inports.insert("in".to_owned(), PortOptions::default());
            }
            if outports.is_empty() {
                outports.insert("out".to_owned(), PortOptions::default());
            }

            return Ok(Component::new(ComponentOptions {
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
                    if let Ok(this) = handle.clone().try_lock().as_mut() {
                        let available_inputs = this.input().get_many(inports.keys().map(|s| s.as_str()).collect()).iter().filter(|s| **s != None).collect::<Vec<_>>().len();
                        if inputs.len() > available_inputs {
                            return Ok(ProcessResult::default());
                        }
              
                        let _inputs: HashMap<&String, Value> = HashMap::from_iter(
                            inputs
                                .clone()
                                .iter()
                                .map(|port| {
                                    let value = this.input().get(*port);
                                    if let Some(value) = value {
                                        return (port.clone(), match value.datatype {
                                            IPType::Data(v) => v,
                                            _ => Value::Null
                                        });
                                    }
                                    return (port.clone(), Value::Null);
                                })
                                .collect::<Vec<_>>(),
                        );
                        
                        let data = json!(_inputs);

                        let output = this.output();

                        let send_fn = Function::new(
                            "send",
                            [ValType::ExternRef],
                            [ValType::I64],
                            None,
                            move |_plugin: &mut CurrentPlugin,
                                  params: &[Val],
                                  _: &mut [Val],
                                  _: UserData| {
                                if !params.is_empty() {
                                    if let Some(Some(payload)) = params[0].externref() {
                                        output.clone().send(payload.data());
                                    }
                                }
                                Ok(())
                            },
                        );

                        let output = this.output();
                        let send_done_fn = Function::new(
                            "send",
                            [ValType::ExternRef],
                            [ValType::I64],
                            None,
                            move |_plugin: &mut CurrentPlugin,
                                  params: &[Val],
                                  _: &mut [Val],
                                  _: UserData| {
                                if !params.is_empty() {
                                    if let Some(Some(payload)) = params[0].externref() {
                                        output.clone().send_done(payload.data());
                                    }
                                }
                                Ok(())
                            },
                        );

                        let context = Context::new();

                        let plugin =
                            Plugin::new_with_manifest(&context, &manifest, [], false);
                        
                        if plugin.is_err() {
                            let err = plugin.err().unwrap().to_string();
                            return Err(ProcessError(err));
                        }

                        let mut plugin = plugin.unwrap();
         
                        let x = plugin.call("process", serde_json::to_string(&data).unwrap());
                        if x.is_err() {
                            return Err(ProcessError(format!(
                                "Failed to call main function from wasm component: {}",
                                x.err().unwrap().to_string()
                            )));
                        }
                        if let Ok(result) = serde_json::from_str::<Value>(
                            std::str::from_utf8(x.unwrap())
                                .expect("expected to decode return value from wasm component"),
                        ) {
                           
                            if let Some(res) = result.as_object() {
                                println!("{:?}", res);
                                this.output().send(res);
                            }
                        }
                    }
                    Ok(ProcessResult::default())
                })),
            }));
        }
        Err(format!("Could not load wasm component"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use futures::executor::block_on;

    use crate::{
        loader::{ComponentLoader, ComponentLoaderOptions},
        port::BasePort,
        registry::DefaultRegistry,
        sockets::InternalSocket,
    };

    use super::*;

    #[test]
    fn create_wasm_component() {
        let mut base_dir = std::env::current_dir().unwrap();
        base_dir.push("test_components");
        let base_dir = base_dir.to_str().unwrap();

        let registry = Arc::new(Mutex::new(DefaultRegistry::default()));
        let mut loader = ComponentLoader::new(
            base_dir,
            ComponentLoaderOptions::default(),
            Some(registry.clone()),
        );

        let wasm_component = loader.load("zflow/add_wasm.wasm", Value::Null);

        assert!(wasm_component.is_ok());

        // Todo: test process execution
        let s_left = InternalSocket::create(None);
        let s_right = InternalSocket::create(None);
        let s_sum = InternalSocket::create(None);

        let wasm_component = wasm_component.unwrap();
        // left operand inport
        wasm_component
            .clone()
            .try_lock()
            .unwrap()
            .get_inports_mut()
            .ports
            .get_mut("left")
            .map(|v| v.attach(s_left.clone(), None));
        // right operand inport
        wasm_component
            .clone()
            .try_lock()
            .unwrap()
            .get_inports_mut()
            .ports
            .get_mut("right")
            .map(|v| v.attach(s_right.clone(), None));
        // sum outport
        wasm_component
            .clone()
            .try_lock()
            .unwrap()
            .get_outports_mut()
            .ports
            .get_mut("sum")
            .map(|v| v.attach(s_sum.clone(), None));

        block_on(async move {
            // send 1
            let _ = s_left
                .clone()
                .try_lock()
                .unwrap()
                .send(Some(&json!(1)))
                .await;

            // send 2
            let _ = s_right
                .clone()
                .try_lock()
                .unwrap()
                .send(Some(&json!(2)))
                .await;
        });
    }
}
