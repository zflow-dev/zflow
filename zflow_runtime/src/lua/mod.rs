use once_cell::sync::OnceCell;
use serde_json::{Map, Value};
use std::{any::Any, collections::HashMap, fs, path::PathBuf, sync::Mutex};

use rlua::{Lua, ToLua, UserData, UserDataMethods, Function, Table};
use serde::{Deserialize, Serialize};

use crate::{
    component::{Component, ComponentOptions, GraphDefinition, ModuleComponent},
    ip::{IPType, IP},
    port::{InPort, OutPort, PortOptions},
    process::{ProcessError, ProcessOutput, ProcessResult},
};

use is_url::is_url;

pub fn lua_instance() -> &'static Mutex<Lua> {
    static INSTANCE: OnceCell<Mutex<Lua>> = OnceCell::new();
    INSTANCE.get_or_init(|| Mutex::new(Lua::new()))
}

impl ToLua<'_> for IP {
    fn to_lua(self, lua: rlua::Context<'_>) -> rlua::Result<rlua::Value<'_>> {
        match self.datatype {
            IPType::Data(value) => {
                return rlua_serde::to_value(lua, value);
            }
            _ => {}
        }
        Err(rlua::Error::RuntimeError(format!(
            "Could not translate IP to Lua value"
        )))
    }
}

impl GraphDefinition for LuaComponent {
    fn to_any(&self) -> &dyn Any {
        Box::leak(Box::new(self.clone())) as &dyn Any
    }
}


fn default_base_dir() -> String {
    "/".to_string()
}


#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct LuaComponent {
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
    /// Base directory of lua sources
    pub base_dir: String,
    /// Path to lua source
    pub source: String,
    #[serde(default)]
    pub package_id: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl LuaComponent {
    pub fn from_metadata(meta: Value) -> Option<LuaComponent> {
        LuaComponent::deserialize(meta).ok()
    }

    pub fn with_metadata(&mut self, meta: Value) -> LuaComponent {
        if let Some(meta) = LuaComponent::from_metadata(meta.clone()) {
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

impl ModuleComponent for LuaComponent {
    fn as_component(&self) -> Result<Component, String> {
        let node_name = self.name.clone();
        let mut code = PathBuf::from(self.base_dir.clone());

        let source = if is_url(&self.source) || self.base_dir != "/"  {
            code.push(self.source.clone());
            fs::read_to_string(code).expect("Could not read lua code")
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

                let controlled = inports.iter().filter(|(k, v)| v.control).map(|(k, _)| k).collect::<Vec<_>>();
                        
                let controlled_data = controlled.iter().map(|k| this.input().get(k.clone())).collect::<Vec<_>>();

                if !controlled.is_empty() && controlled_data.contains(&None) {
                    return Ok(ProcessResult::default());
                }

                lua_instance()
                    .try_lock()
                    .map_err(|err| {
                        ProcessError(format!("Failed to load lua runtime instance: {:?}", err))
                    })?
                    .context(|lua_context| {
                        let _inputs = lua_context
                            .create_table_from(
                                inputs
                                    .clone()
                                    .iter()
                                    .map(|port| {
                                        let value = this.input().get(*port);
                                        if let Some(value) = value {
                                            if let Ok(lua_value) = value.to_lua(lua_context) {
                                                return (port.clone().as_str(), lua_value);
                                            }
                                        }
                                        return (port.clone().as_str(), rlua::Value::Nil);
                                    })
                                    .collect::<Vec<_>>(),
                            )
                            .map_err(|err| {
                                ProcessError(format!(
                                    "Could not serialize input for lua component: {}",
                                    err.to_string()
                                ))
                            })?;

                        let output = this.output();

                        let send: rlua::Function = lua_context
                            .create_function_mut(move |_, data: rlua::Value<'_>| {
                                let data = rlua_serde::from_value::<Value>(data)?;
                                println!("{:?}", data);
                                output
                                    .clone()
                                    .send(&data)
                                    .map_err(|err| rlua::Error::RuntimeError(err.0))
                            })
                            .map_err(|err| {
                                ProcessError(format!(
                                    "Could not create instance of Lua function: {}",
                                    err
                                ))
                            })?;

                        let output = this.output();

                        let send_done: rlua::Function = lua_context
                            .create_function_mut(move |_, data: rlua::Value<'_>| {
                                let data = rlua_serde::from_value::<Value>(data)?;
                                output
                                    .clone()
                                    .send_done(&data)
                                    .map_err(|err| rlua::Error::RuntimeError(err.0))
                            })
                            .map_err(|err| {
                                ProcessError(format!(
                                    "Could not create instance of Lua function: {}",
                                    err
                                ))
                            })?;

                        let output = this.output();

                        let send_buffer: rlua::Function = lua_context
                            .create_function_mut(move |_, data: (String, Vec<u8>)| {
                                output
                                    .clone()
                                    .send_buffer(&data.0, data.1)
                                    .map_err(|err| rlua::Error::RuntimeError(err.0))
                            })
                            .map_err(|err| {
                                ProcessError(format!(
                                    "Could not create instance of Lua function: {}",
                                    err
                                ))
                            })?;

                        let globals = lua_context.globals();

                        let outports_table = lua_context.create_table().unwrap();

                        outports_table.set("send", send).map_err(|err| {
                            ProcessError(format!(
                                "Could not create instance of Lua function: {}",
                                err
                            ))
                        })?;
                        outports_table.set("send_done", send_done).map_err(|err| {
                            ProcessError(format!(
                                "Could not create instance of Lua function: {}",
                                err
                            ))
                        })?;
                        outports_table
                            .set("send_buffer", send_buffer)
                            .map_err(|err| {
                                ProcessError(format!(
                                    "Could not create instance of Lua function: {}",
                                    err
                                ))
                            })?;


                        globals.set("zflow", outports_table).map_err(|err| {
                            ProcessError(format!("Could load zflow process: {}", err))
                        })?;

                        let lua_code = lua_context
                            .load(&source)
                            .set_name(&node_name)
                            .map_err(|err| ProcessError(format!("Unexpected error: {}", err)))?;
                        lua_code.eval().map_err(|err| ProcessError(format!("Unexpected error: {}", err)))?;
                        let proc = globals.get::<&str, Table>("zflow").unwrap().get::<&str, Function>("process").map_err(|err| ProcessError(format!("Unexpected error: {}", err)))?;
                       
                        proc.call::<(Table, ), ()>((_inputs, )).map_err(|err| ProcessError(format!("Unexpected error: {}", err)))?;
    
                        return Ok(ProcessResult::default());
                    })
            })),
        }));
    }
}

#[cfg(test)]
mod tests {

    use std::{thread, collections::HashMap};

    use serde::Deserialize;
    use serde_json::json;
    use zflow_graph::Graph;

    use crate::{
        network::{BaseNetwork, Network, NetworkOptions},
        registry::ComponentSource, port::PortOptions,
    };


    #[test]
    fn create_lua_component() {
        let mut base_dir = std::env::current_dir().unwrap();
        base_dir.push("test_components");
        let base_dir = base_dir.to_str().unwrap();

        let mut graph = Graph::new("", false);
        graph
            .add_node("zflow", "add_lua", None)
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

    #[test]
    fn create_lua_source_component() {
        let mut base_dir = std::env::current_dir().unwrap();
        base_dir.push("test_components");
        let base_dir = base_dir.to_str().unwrap();

        let mut graph = Graph::new("", false);
        graph
            .add_node("zflow", "add_inline", None)
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

        network
                .get_loader().set_source("zflow", "add_inline", ComponentSource{
                    name: "add_inline".to_owned(),
                    language: "lua".to_owned(),
                    source: "if zflow.inports[\"left\"] ~= nil and zflow.inports[\"right\"] ~= nil then\n
                    left = zflow.inports[\"left\"]\n
                    right = zflow.inports[\"right\"]\n
                    zflow.outports.send({sum= left + right})\n
                    end".to_owned(),
                    inports: HashMap::from_iter(vec![
                        ("left".to_owned(), PortOptions{
                            triggering: true,
                            control: true,
                            ..PortOptions::default()
                        }),
                        ("right".to_owned(), PortOptions{
                            triggering: true,
                            control: true,
                            ..PortOptions::default()
                        })
                    ]),
                    outports: HashMap::from_iter(vec![
                        ("sum".to_owned(), PortOptions::default())
                    ]),    
                }).unwrap();

        if let Ok(nw) = network.connect().unwrap().try_lock().as_mut() {
            nw.start().unwrap();
        }
    }
}
