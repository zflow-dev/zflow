use array_tool::vec::Join;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::any::Any;
use std::fs;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, path::PathBuf};
use is_url::is_url;

use rquickjs::{
    BuiltinLoader,
    BuiltinResolver,
    // loader::{
    //     BuiltinLoader, BuiltinResolver, FileResolver, ModuleLoader, NativeLoader, ScriptLoader,
    // },
    Context,
    Ctx,
    FileResolver,
    Func,
    ModuleDef,
    ModuleLoader,
    NativeLoader,
    Runtime,
};

use crate::component::GraphDefinition;
use crate::process::ProcessHandle;
use crate::{
    component::{Component, ComponentOptions, ModuleComponent},
    ip::IPType,
    port::{InPort, OutPort, PortOptions},
    process::{ProcessError, ProcessResult},
};

impl GraphDefinition for JsComponent {
    fn to_any(&self) -> &dyn Any {
        Box::leak(Box::new(self.clone())) as &dyn Any
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct JsComponent {
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
    /// Base directory of js packages
    pub base_dir: String,
    /// Path to js source
    pub source: String,
    #[serde(default)]
    pub package_id: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl JsComponent {
    pub fn from_metadata(meta: Value) -> Option<JsComponent> {
        JsComponent::deserialize(meta).ok()
    }

    pub fn with_metadata(&mut self, meta: Value) -> JsComponent {
        if let Some(meta) = JsComponent::from_metadata(meta.clone()) {
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

impl ModuleComponent for JsComponent {
    fn as_component(&self) -> Result<Component, String> {
        // let source = self.source.clone();
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
            process: Some(Box::new(move |handle: Arc<Mutex<ProcessHandle>>| {
                if let Ok(this) = handle.clone().try_lock().as_mut() {
                    let rt = Runtime::new().expect("runtime error");
                    let context = Context::full(&rt).unwrap();

                    rt.set_loader(
                        (
                            BuiltinResolver::default().with_module("builtin/console"),
                            FileResolver::default().with_native(),
                        ),
                        (
                            NativeLoader::default(),
                            BuiltinLoader::default(),
                            ModuleLoader::default().with_module("builtin/console", JsConsole),
                        ),
                    );
            
                    return context.with(|ctx| {
                        // let inputs: Vec<&String> = inports.keys().collect();
                        let global = ctx.globals();

                        // JsConsole::load(ctx, )
                        let process_object = rquickjs::Object::new(ctx).unwrap();
                        let mut _inputs = rquickjs::Object::new(ctx).unwrap();

                        for key in inports.keys() {
                            let value = this.input().get(key);
                            if let Some(value) = value {
                                _inputs
                                    .set(
                                        key,
                                        match value.datatype {
                                            IPType::Data(v) => json_value_to_js_value(ctx, v)
                                                .expect("runtime error"),
                                            _ => ctx.eval("null").unwrap(),
                                        },
                                    )
                                    .expect("runtime error");
                            }
                        }

                        process_object
                            .set("inport", _inputs.as_value().clone())
                            .unwrap();

                        let mut _outputs = rquickjs::Object::new(ctx).unwrap();
                        let output = this.output.clone();
                        _outputs
                            .set(
                                "send",
                                Func::new("", move |data: rquickjs::Value| {
                                    let data = js_value_to_json_value(data).expect("runtime error");
                                    println!("{:?}", data);
                                    output.clone().send(&data).unwrap();
                                }),
                            )
                            .expect("runtime error");
                        let output = this.output.clone();
                        _outputs
                            .set(
                                "sendBuffer",
                                Func::new(
                                    "",
                                    move |port: rquickjs::String, data: rquickjs::Array| {
                                        let v = data
                                            .into_iter()
                                            .map(|d| d.unwrap().as_int().unwrap() as u8)
                                            .collect::<Vec<u8>>();
                                        output
                                            .clone()
                                            .send_buffer(
                                                &port
                                                    .to_string()
                                                    .expect("expected port name to be string"),
                                                &v,
                                            )
                                            .unwrap();
                                    },
                                ),
                            )
                            .expect("runtime error");

                        let output = this.output.clone();
                        _outputs
                            .set(
                                "sendDone",
                                Func::new("", move |data: rquickjs::Value| {
                                    output
                                        .clone()
                                        .send_done(
                                            &js_value_to_json_value(data).expect("runtime error"),
                                        )
                                        .unwrap();
                                }),
                            )
                            .expect("runtime error");

                        process_object
                            .set("outport", _outputs.as_value().clone())
                            .unwrap();

                        global
                            .set("ProcessHandle", process_object)
                            .expect("runtime error");

                        let console_module = r#"
                            import {log, info, warn, error, debug} from "builtin/console";
                            export default  {log, info, warn, error, debug};
                        "#;

                        let console = ctx.compile("default", console_module).unwrap();
                        global
                            .set(
                                "console",
                                console.get::<&str, rquickjs::Object>("default").unwrap(),
                            )
                            .expect("runtime error");
                        
                        return match ctx.eval(source.clone()) {
                            Ok(val) => {
                                if let Err(err) = this
                                    .output
                                    .send(&js_value_to_json_value(val).expect("runtime error"))
                                {
                                    Err(err)
                                } else {
                                    Ok(ProcessResult::default())
                                }
                            }
                            Err(err) => Err(ProcessError(err.to_string())),
                        };
                    });
                }
                Ok(ProcessResult::default())
            })),
            ..Default::default()
        }));
    }
}

fn js_value_to_json_value(value: rquickjs::Value) -> Result<Value, ProcessError> {
    if let Some(v) = value.as_string() {
        return Ok(json!(v
            .to_string()
            .expect("expected to read javascript string value")));
    }

    if let Some(v) = value.as_int() {
        return Ok(json!(v));
    }

    if let Some(v) = value.as_float() {
        return Ok(json!(v));
    }
    if let Some(v) = value.as_number() {
        return Ok(json!(v));
    }

    if let Some(v) = value.as_array() {
        let mut arr = Vec::new();
        for val in v.clone().into_iter() {
            if let Ok(val) = val {
                arr.push(js_value_to_json_value(val)?);
            }
        }
        return Ok(json!(arr));
    }

    if let Some(v) = value.as_bool() {
        return Ok(json!(v));
    }

    if let Some(v) = value.as_object() {
        let mut arr = Map::new();
        for val in v.clone().into_iter() {
            if let Ok((k, v)) = val {
                arr.insert(
                    k.to_string()
                        .expect("expected to read key from javascript object"),
                    js_value_to_json_value(v)?,
                );
            }
        }
        return Ok(json!(arr));
    }

    Ok(json!(null))
}

fn json_value_to_js_value<'js>(
    ctx: Ctx<'js>,
    value: Value,
) -> Result<rquickjs::Value<'js>, ProcessError> {
    if let Some(v) = value.as_str() {
        return Ok(rquickjs::String::from_str(ctx, v)
            .expect("expected to convert value to js value")
            .into());
    }

    if let Some(v) = value.as_i64() {
        let value = rquickjs::Value::new_number(ctx, v as f64);
        return Ok(value);
    }

    if let Some(v) = value.as_f64() {
        let value = rquickjs::Value::new_number(ctx, v);
        return Ok(value);
    }
    if let Some(v) = value.as_u64() {
        let value = rquickjs::Value::new_number(ctx, v as f64);
        return Ok(value);
    }

    if let Some(v) = value.as_array() {
        if let Ok(arr) = rquickjs::Array::new(ctx).as_mut() {
            for (i, val) in v.clone().into_iter().enumerate() {
                arr.set(i, json_value_to_js_value(ctx, val)?).expect("");
            }
            return Ok(arr.as_value().clone());
        }
    }

    if let Some(v) = value.as_bool() {
        if v {
            return Ok(rquickjs::Value::new_bool(ctx, v));
        }
    }

    if let Some(v) = value.as_object() {
        if let Ok(obj) = rquickjs::Object::new(ctx).as_mut() {
            for (k, val) in v.clone().into_iter() {
                obj.set(k, json_value_to_js_value(ctx, val)?).expect("");
            }
            return Ok(obj.as_value().clone());
        }
    }

    Ok(rquickjs::Value::new_undefined(ctx))
}

pub struct JsConsole;

impl ModuleDef for JsConsole {
    fn load<'js>(
        _ctx: Ctx<'js>,
        _module: &rquickjs::Module<'js, rquickjs::Created>,
    ) -> rquickjs::Result<()> {
        _module.add("log")?;
        _module.add("info")?;
        _module.add("warn")?;
        _module.add("error")?;
        _module.add("debug")?;
        Ok(())
    }

    fn eval<'js>(
        _ctx: Ctx<'js>,
        _module: &rquickjs::Module<'js, rquickjs::Loaded<rquickjs::Native>>,
    ) -> rquickjs::Result<()> {
        fn fun(name: &'static str) -> impl Fn(rquickjs::Rest<rquickjs::Value>) {
            move |data: rquickjs::Rest<rquickjs::Value>| {
                println!(
                    "[console.{}] {:?}",
                    name,
                    data.0
                        .iter()
                        .map(|v| js_value_to_json_value(v.clone()).unwrap())
                        .collect::<Vec<Value>>()
                        .join(",")
                );
            }
        }

        _module.set("log", Func::new("", fun("log")))?;
        _module.set("info", Func::new("", fun("info")))?;
        _module.set("warn", Func::new("", fun("warn")))?;
        _module.set("error", Func::new("", fun("error")))?;
        _module.set("debug", Func::new("", fun("debug")))?;
        Ok(())
    }
}

rquickjs::module_init!(JsConsole);

#[cfg(test)]
mod tests {

    use zflow_graph::Graph;

    use crate::network::{BaseNetwork, Network, NetworkOptions};

    use super::*;

    #[test]
    fn create_js_component() {
        let mut base_dir = std::env::current_dir().unwrap();
        base_dir.push("test_components");
        let base_dir = base_dir.to_str().unwrap();

        let mut graph = Graph::new("", false);
        graph
            .add_node("zflow", "add", None)
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
