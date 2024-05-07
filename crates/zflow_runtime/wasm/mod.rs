use std::{
    collections::HashMap, path::PathBuf, sync::{Arc, Mutex}
};

use extism::{Function, Manifest, Plugin, UserData, ValType, Wasm, WasmInput, WasmMetadata};
use rayon::iter::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};
use serde_json::{json, Value};
use zflow_plugin::{ComponentSource, ComponentWithInput, OutputBuffer, Package, Platform, Runtime};
use extism_pdk::{HttpRequest, Json};
use log::log;
use crate::{
    extern_provider::ExternProviderSource, ip::IPType, process::{ProcessError, ProcessResult}, provider::{Provider, ProviderComponent, ProviderRunner}, runner::{RunFunc, WASM_RUNNER_ID}
};

pub struct WasmExternProvider {
    pub workspace:String,
    pub logo: String,
    pub provider_id: String,
    pub platform: Platform,
    pub packages: Vec<Package>,
    manifest: Manifest,
    components: HashMap<String, Box<dyn ProviderComponent>>,
}

impl WasmExternProvider {
    pub fn new(workspace: &str, source:ExternProviderSource) -> Result<Self, anyhow::Error> {
        let wasm = match source.clone() {
            ExternProviderSource::Url { url } => Wasm::Url {
                req: HttpRequest::new(url),
                meta: WasmMetadata::default(),
            },
            ExternProviderSource::Local { path } => {
                let mut workspace = PathBuf::from(workspace);
                workspace.push(path);
                Wasm::File {
                    path: PathBuf::from(workspace.to_str().unwrap().to_owned()),
                    meta: WasmMetadata::default(),
                }
            }
            ExternProviderSource::Data { data } => Wasm::Data {
                data,
                meta: WasmMetadata::default(),
            },
            ExternProviderSource::Undefined => return Err(anyhow::Error::msg("Undefined extern provider source"))
        };
        let manifest = Manifest::new([wasm.clone()]);

        let stub_func = |name: &str| -> Function {
            Function::new(
                name,
                [ValType::I64],
                [],
                UserData::new(()),
                move |_plugin, _params, _, _userdata| Ok(()),
            )
        };

        let mut plugin = Plugin::new(
            WasmInput::Manifest(manifest.clone()),
            [
                stub_func("send_done"),
                stub_func("send"),
                stub_func("send_buffer"),
            ],
            true,
        )?;

        let provider_id = plugin.call::<(), String>("provider_id", ())?;
        let logo = plugin.call::<(), String>("get_logo", ())?;
        let packages = plugin
            .call::<(), Json<Vec<Package>>>("get_packages", ())?
            .into_inner();
        let platform = plugin
            .call::<(), Json<Platform>>("get_platform", ())?
            .into_inner();

        Ok(WasmExternProvider{
            provider_id,
            manifest,
            logo,
            packages,
            platform,
            workspace: workspace.to_owned(),
            components: HashMap::new()
        })
    }
    fn load_components(&mut self) -> Result<(), anyhow::Error> {
        self.components = self
            .packages
            .par_iter()
            .map(|package| {
                package.components.par_iter().map(|component| {
                    let _component: Box<(dyn ProviderComponent + 'static)> =
                        Box::new(component.clone());
                    (component.name.clone(), _component)
                })
            })
            .flatten()
            .collect::<HashMap<String, Box<dyn ProviderComponent>>>();

        Ok(())
    }
}



impl Provider for WasmExternProvider {
    fn set_workspace(&mut self, dir: String) {
        let mut buf = PathBuf::from(dir);
        buf.push(self.workspace.clone());
        self.workspace = buf.to_str().unwrap().to_owned();
    }

    fn get_logo(&self) -> Result<String, anyhow::Error> {
        Ok(self.logo.clone())
    }

    fn get_id(&self) -> String {
        self.provider_id.clone()
    }

    fn list_components(&mut self) -> Result<Vec<String>, anyhow::Error> {
        Ok(self
            .packages
            .par_iter_mut()
            .map(|package| {
                package.components.par_iter_mut().map(|component| {
                    component.runtime = Runtime {
                        provider_id: self.provider_id.clone(),
                        runner_id: WASM_RUNNER_ID.to_owned(),
                        platform: self.platform.clone(),
                    };

                    component.name.clone()
                })
            })
            .flatten()
            .collect::<Vec<String>>())
    }

    fn load_component(
        &mut self,
        id: String,
    ) -> Result<(&Box<dyn crate::provider::ProviderComponent>, ProviderRunner), anyhow::Error> {
        self.list_components()?;
        self.load_components()?;

        if let Some(component) = self.components.get(&id) {
            return Ok((component, self.get_process(component)?));
        }

        let err = format!("Could not find component for provider {}", self.get_id());
        log!(target:format!("Provider {}", self.get_id()).as_str(),log::Level::Debug, "Could not find component with ID {}", id);
        return Err(anyhow::Error::msg(err));
    }

    fn get_process(
        &self,
        component: &Box<dyn crate::provider::ProviderComponent>,
    ) -> Result<ProviderRunner, anyhow::Error> {
        let component: &ComponentSource = component
            .as_any()
            .downcast_ref::<ComponentSource>()
            .unwrap();

        let manifest = self.manifest.clone();

        let process = "run_component";
        let provider_id = self.provider_id.clone();

        let mut runner =
            crate::wasm::get_sys_wasm_runner(component.clone(), process, manifest, true)?;
        runner.runner_id = WASM_RUNNER_ID.to_owned();
        return Ok(runner);
    }
}

pub fn get_sys_wasm_runner<'a>(
    component: ComponentSource,
    process_name: &'a str,
    manifest: Manifest,
    is_provider: bool,
) -> Result<ProviderRunner, anyhow::Error> {
    let id = component.name.to_owned();
    let process_name = process_name.to_owned();

    let runner_func: Box<RunFunc> = Box::new(move |handle| {
        let handle_binding = handle.clone();
        let mut handle_binding = handle_binding.try_lock();
        let this = handle_binding
            .as_mut()
            .map_err(|_| ProcessError(String::from("Process Handle has dropped")))?;

        let inports = this.input().in_ports.ports;

        let inport_keys = inports.keys();
        let controlled_data = this
            .input()
            .in_ports
            .ports
            .iter()
            .filter(|(_, port)| port.options.control)
            .map(|(key, _)| this.input().get(key))
            .collect::<Vec<_>>();

        if !controlled_data.is_empty() && controlled_data.contains(&None) {
            return Ok(ProcessResult::default());
        }

        let _inputs: HashMap<String, Value> = HashMap::from_iter(
            inport_keys
                .map(|port| {
                    let value = this.input().get(port);
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

        let _id = id.clone();

        let send_done_fn = Function::new(
            "send_done",
            [ValType::I64],
            [],
            UserData::Rust(Arc::new(Mutex::new(this.output()))),
            move |plugin, params, _, userdata| {
                let _output = &params[0];
                let handle = plugin.memory_handle(_output.unwrap_i64() as u64).unwrap();
                let bytes = plugin.memory_str(handle)?;
                let data: Value = serde_json::from_str(bytes)?;

                let output = userdata.get()?.clone();
                output.lock().unwrap().send_done(&data).expect(&format!(
                    "expected to send output data from component {}\n",
                    _id.clone()
                ));
                Ok(())
            },
        );

        let _id = id.clone();

        let send_fn = Function::new(
            "send",
            [ValType::I64],
            [],
            UserData::Rust(Arc::new(Mutex::new(this.output()))),
            move |plugin, params, _, userdata| {
                let _output = &params[0];
                let handle = plugin.memory_handle(_output.unwrap_i64() as u64).unwrap();
                let bytes = plugin.memory_str(handle)?;
                let data: Value = serde_json::from_str(bytes)?;
                let output = userdata.get()?.clone();
                output.lock().unwrap().send(&data).expect(&format!(
                    "expected to send output data from component {}\n",
                    _id.clone()
                ));
                Ok(())
            },
        );

        let _id = id.clone();

        // `send_buffer` Host function for use in the wasm binary
        let send_buffer_fn = Function::new(
            "send_buffer",
            [ValType::I64],
            [ValType::I64],
            UserData::Rust(Arc::new(Mutex::new(this.output()))),
            move |plugin, params, _, userdata| {
                let handle = plugin.memory_from_val(&params[0]).unwrap();
                let data =
                    serde_json::from_str::<OutputBuffer>(plugin.memory_str(handle)?).unwrap();
                let output = userdata.get()?.clone();
                output
                    .lock()
                    .unwrap()
                    .send_buffer(data.port, data.packet)
                    .expect(&format!(
                        "expected to send output data from component {}\n",
                        _id.clone()
                    ));

                Ok(())
            },
        );

        let plugin = Plugin::new(
            WasmInput::Manifest(manifest.clone()),
            [send_done_fn, send_fn, send_buffer_fn],
            true,
        );
        if plugin.is_err() {
            let err = plugin.err().unwrap().to_string();
            return Err(ProcessError(err));
        }
        let mut plugin = plugin.unwrap();

        let call_input = if is_provider {
            json!(ComponentWithInput {
                component: component.clone(),
                input: mapped_inputs
            })
        } else {
            mapped_inputs
        };

        let data = plugin.call(
            process_name.clone(),
            serde_json::to_string(&call_input).unwrap(),
        );

        if data.is_err() {
            return Err(ProcessError(format!(
                "Failed to call main function from wasm component: {}",
                data.err().unwrap().to_string()
            )));
        }
        if let Ok(result) = serde_json::from_str::<Value>(
            std::str::from_utf8(data.unwrap())
                .expect("expected to decode return value from wasm component"),
        ) {
            if let Some(res) = result.as_object() {
                return Ok(ProcessResult {
                    data: json!(res.clone()),
                    resolved: true,
                    ..ProcessResult::default()
                });
            }
        }

        Ok(ProcessResult::default())
    });

    return Ok(ProviderRunner {
        runner_id: WASM_RUNNER_ID.to_owned(),
        runner_func,
        platforms: vec![Platform::System],
    });
}


