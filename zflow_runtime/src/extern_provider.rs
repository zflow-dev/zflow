use std::path::PathBuf;
use std::{collections::HashMap, rc::Rc};

use deno_core::PollEventLoopOptions;
use v8::HandleScope;
use zflow_plugin::{ComponentSource, Package, Platform, Runtime};

use crate::deno::DenoModuleLoader;
use crate::provider::{self, ProviderComponent};

use ::extism::*;

use extism_pdk::{HttpRequest, Json};
use log::log;
use rayon::iter::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum ExternProviderType {
    #[default]
    Wasi,
    Deno,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum ExternProviderSource {
    Data { data: Vec<u8> },
    Url { url: String },
    Local { path: String },
}

pub(crate) fn _default_base_dir() -> String {
    "/".to_string()
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ExternProvider {
    #[serde(default = "_default_base_dir")]
    worspace: String,
    source: ExternProviderSource,
    #[serde(default, rename = "type")]
    extern_type: ExternProviderType,
    #[serde(default)]
    logo: String,
    provider_id: String,
    platform: Platform,
    packages: Vec<Package>,
    #[serde(skip_serializing, skip_deserializing)]
    _components: HashMap<String, Box<dyn ProviderComponent>>,
    #[serde(skip_serializing, skip_deserializing)]
    _wasm_manifest: Option<Manifest>,
    #[serde(skip_serializing, skip_deserializing)]
    _deno_module_specifier: Option<deno_core::ModuleSpecifier>,
}

impl Debug for ExternProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternProvider")
            .field("worspace", &self.worspace)
            .field("source", &self.source)
            .field("extern_type", &self.extern_type)
            .field("logo", &self.logo)
            .field("provider_id", &self.provider_id)
            .field("platform", &self.platform)
            .field("packages", &self.packages)
            // .field("_components", &self._components)
            .field("_wasm_manifest", &self._wasm_manifest)
            .field("_deno_module_specifier", &self._deno_module_specifier)
            .finish()
    }
}

impl provider::Provider for ExternProvider {
    fn set_workspace(&mut self, dir: String) {
        let mut buf = PathBuf::from(dir);
        buf.push(self.worspace.clone());
        self.worspace = buf.to_str().unwrap().to_owned();
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
                        runner_id: self.provider_id.clone(),
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
    ) -> Result<
        (
            &Box<dyn provider::ProviderComponent>,
            provider::ProviderRunner,
        ),
        anyhow::Error,
    > {
        self.list_components()?;
        self.load_components()?;

        if let Some(component) = self._components.get(&id) {
            let runner = match self.extern_type {
                ExternProviderType::Wasi => self.get_wasm_process(component)?,
                ExternProviderType::Deno => self.get_deno_process(component)?,
            };
            return Ok((component, runner));
        }

        let err = format!("Could not find component for provider {}", self.get_id());
        log!(target:format!("Provider {}", self.get_id()).as_str(),log::Level::Debug, "Could not find component with ID {}", id);
        return Err(anyhow::Error::msg(err));
    }

    fn get_process(
        &self,
        component: &Box<dyn provider::ProviderComponent>,
    ) -> Result<provider::ProviderRunner, anyhow::Error> {
        return match self.extern_type {
            ExternProviderType::Wasi => self.get_wasm_process(component),
            // ExternProviderType::Deno => self.get_deno_process(component),
            _ => Err(anyhow::Error::msg("")),
        };
    }
}

impl ExternProvider {
    pub fn from_wasm(workspace: &str, source: ExternProviderSource) -> Result<Self, anyhow::Error> {
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

        Ok(Self {
            worspace: workspace.to_owned(),
            source,
            extern_type: ExternProviderType::Wasi,
            logo,
            provider_id,
            platform,
            packages,
            _components: HashMap::new(),
            _wasm_manifest: Some(manifest),
            _deno_module_specifier: None,
        })
    }

    pub async fn from_deno(
        workspace: &str,
        source: ExternProviderSource,
    ) -> Result<Self, anyhow::Error> {
        let module_error = |err| match err {
            deno_core::ModuleResolutionError::InvalidUrl(e) => anyhow::Error::msg(e),
            deno_core::ModuleResolutionError::InvalidBaseUrl(e) => anyhow::Error::msg(e),
            deno_core::ModuleResolutionError::InvalidPath(e) => {
                anyhow::Error::msg(format!("Invalid path: {:?}", e.as_os_str()))
            }
            _ => anyhow::Error::msg("Failed to load deno module from path"),
        };
        let module: deno_core::ModuleSpecifier = match source.clone() {
            ExternProviderSource::Local { path } => {
                let workspace = PathBuf::from(workspace);
                deno_core::resolve_path(&path, workspace.as_path()).map_err(module_error)?
            }
            ExternProviderSource::Url { url } => {
                deno_core::resolve_url(&url).map_err(module_error)?
            }
            ExternProviderSource::Data { data: _ } => Err(anyhow::Error::msg(
                "Bytes source not supported for Deno runtime",
            ))?,
        };

        // if module.is_none(){
        //     match source.clone() {
        //         ExternProviderSource::Data { data } => {
        //           let code =  deno_core::ModuleCodeBytes::Static(data.as_slice());

        //         },
        //         _=> {}
        //     }
        // }

        let mut runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
            module_loader: Some(Rc::new(DenoModuleLoader)),
            ..Default::default()
        });

        let mod_id = runtime.load_main_module(&module, None).await?;
        let result = runtime.mod_evaluate(mod_id);
        runtime
            .run_event_loop(PollEventLoopOptions {
                wait_for_inspector: false,
                pump_v8_message_loop: false,
            })
            .await?;

        let global = runtime.get_module_namespace(mod_id)?;
        let scope = &mut runtime.handle_scope();
        let module_obj = global.open(scope);

        fn call_func<'a>(
            module: &v8::Object,
            scope: &mut HandleScope<'a>,
            name: &str,
        ) -> Result<v8::Local<'a, v8::Value>, anyhow::Error> {
            let func_key = v8::String::new(scope, name).unwrap();
            let func = module.get(scope, func_key.into()).unwrap();
            let func = v8::Local::<v8::Function>::try_from(func)?;
            let rec = v8::null(scope).into();
            Ok(func.call(scope, rec, &[]).unwrap())
        }

        let provider_id = call_func(module_obj, scope, "providerId")?;
        let provider_id = provider_id.to_rust_string_lossy(scope);

        let logo = call_func(module_obj, scope, "getLogo")?;
        let logo = logo.to_rust_string_lossy(scope);

        let mut packages = vec![];
        let _packages = call_func(module_obj, scope, "getPackages")?;
        let _packages = v8::Local::<v8::Array>::try_from(_packages)?;

        for i in 0.._packages.length() {
            let index = v8::Integer::new(scope, i as i32);
            let item = _packages
                .get(scope, v8::Local::<v8::Value>::try_from(index)?)
                .unwrap();
            let package = serde_v8::from_v8::<Package>(scope, item)?;
            packages.push(package);
        }

        let _ = result.await?;

        Ok(Self {
            worspace: workspace.to_owned(),
            source,
            extern_type: ExternProviderType::Deno,
            logo,
            provider_id,
            platform: Platform::System,
            packages,
            _components: HashMap::new(),
            _wasm_manifest: None,
            _deno_module_specifier: Some(module),
        })

        // Err(anyhow::Error::msg(""))
    }

    fn load_components(&mut self) -> Result<(), anyhow::Error> {
        self._components = self
            .packages
            .par_iter()
            .map(|package| {
                package.components.par_iter().map(|component| {
                    let _component: Box<(dyn provider::ProviderComponent + 'static)> =
                        Box::new(component.clone());
                    (component.name.clone(), _component)
                })
            })
            .flatten()
            .collect::<HashMap<String, Box<dyn ProviderComponent>>>();

        Ok(())
    }
    fn get_wasm_process(
        &self,
        component: &Box<dyn provider::ProviderComponent>,
    ) -> Result<provider::ProviderRunner, anyhow::Error> {
        let component: &ComponentSource = component
            .as_any()
            .downcast_ref::<ComponentSource>()
            .unwrap();

        let manifest = self._wasm_manifest.clone().unwrap();

        let process = "run_component";
        let provider_id = self.provider_id.clone();

        let mut runner =
            crate::wasm::get_sys_wasm_runner(component.clone(), process, manifest, true)?;
        runner.runner_id = provider_id;
        return Ok(runner);
    }

    fn get_deno_process(
        &self,
        component: &Box<dyn provider::ProviderComponent>,
    ) -> Result<provider::ProviderRunner, anyhow::Error> {
        let component: &ComponentSource = component
            .as_any()
            .downcast_ref::<ComponentSource>()
            .unwrap();

        let process = "runComponent";
        let provider_id = self.provider_id.clone();

        let mut runner = crate::deno::get_sys_deno_runner(
            component.clone(),
            process,
            self._deno_module_specifier
                .clone()
                .as_ref()
                .unwrap()
                .clone(),
            true,
        )?;
        runner.runner_id = provider_id;
        return Ok(runner);
    }
}

mod test {
    use std::{collections::HashMap, env::current_dir, sync::Once};

    use serde_json::json;
    use simple_logger::SimpleLogger;
    use zflow_graph::Graph;

    use crate::{
        component::{Component, ComponentOptions},
        extern_provider::{ExternProvider, ExternProviderSource},
        network::{BaseNetwork, Network, NetworkOptions},
        port::InPort,
        process::ProcessResult,
    };
    use log::{log, Level};

    static INIT: Once = Once::new();

    fn init() {
        INIT.call_once(||{
            SimpleLogger::default()
            .with_colors(true)
            .without_timestamps()
            .with_level(log::LevelFilter::Off)
            .with_module_level("zflow_runtime::extern_provider", log::LevelFilter::Debug)
            .init()
            .unwrap();
        });
    }

    fn get_graph() -> Graph {
        let mut graph = zflow_graph::Graph::new("my_graph", false);
        graph
            .add_node("math/add", "add", None)
            .add_node("debug/logger", "logger", None)
            .add_initial(json!(48), "math/add", "a", None)
            .add_initial(json!(2), "math/add", "b", None)
            .add_edge("math/add", "result", "debug/logger", "message", None)
            .clone()
    }

    fn get_logger() -> Component {
        Component::new(ComponentOptions {
            in_ports: HashMap::from_iter([("message".to_owned(), InPort::default())]),
            process: Some(Box::new(|handle| {
                if let Ok(this) = handle.try_lock().as_mut() {
                    let input = this.input().get("message").unwrap();
                    match input.datatype {
                        crate::ip::IPType::Data(data) => {
                            log!(Level::Debug, "[Output] -> {:?}", data);
                        }
                        _ => {}
                    }
                }
                drop(handle);
                Ok(ProcessResult {
                    resolved: true,
                    ..Default::default()
                })
            })),
            ..Default::default()
        })
    }

    fn get_network() -> Network {
        let mut dir = current_dir().unwrap();
        dir.push("test_components");
        let dir = dir.to_str().unwrap();

        Network::create(
            get_graph(),
            NetworkOptions {
                subscribe_graph: false,
                delay: false,
                workspace_dir: dir.to_owned(),
                ..NetworkOptions::default()
            },
        )
    }

    #[test]
    fn test_wasm_provider() {
        init();

        let mut n = get_network();

        n.register_component("debug", "logger", get_logger())
            .unwrap();

        n.register_provider(
            ExternProvider::from_wasm(
                "test_providers",
                ExternProviderSource::Local {
                    path: "maths_provider/bin/maths_provider.wasm".to_owned(),
                },
            )
            .unwrap(),
        );

        let n = n.connect();
        assert!(n.is_ok());
        if let Ok(n) = n {
            assert!(n.start().is_ok());
            assert!(n.stop().is_ok());
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_deno_provider() {
        init();

        let mut n = get_network();

        n.register_component("debug", "logger", get_logger())
            .unwrap();

        let mut dir = current_dir().unwrap();
        dir.push("test_providers");
        let dir = dir.to_str().unwrap();

        let provider = ExternProvider::from_deno(
            dir,
            ExternProviderSource::Local {
                path: "maths_provider/math.ts".to_owned(),
            },
        )
        .await
        .unwrap();

        n.register_provider(provider);

        let n = n.connect();
        assert!(n.is_ok());
        if let Ok(n) = n {
            assert!(n.start().is_ok());
            assert!(n.stop().is_ok());
        }
    }
}
