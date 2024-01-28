use crate::process::{ProcessError, ProcessHandle, ProcessResult};
use crate::runner::{DynRunFunc, UnsafeRunFunc, QUICKJS_RUNNER_ID};
use crate::{port::PortOptions, runner::RunFunc};
use zflow_plugin::{ComponentSource, Package, Runtime, Platform};


use jwalk::WalkDir;

use libloading::{Library, Symbol};
use log::log;
use rayon::iter::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::any::Any;
use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};
use zflow_graph::types::GraphJson;
use zflow_graph::Graph;

pub(crate) const BUILT_IN_PROVIDER_ID: &str = "@zflow/builtin";

pub trait ProviderComponent: Sync + Send {
    fn get_name(&self) -> String;
    fn get_inports(&self) -> HashMap<String, PortOptions>;
    fn get_outports(&self) -> HashMap<String, PortOptions>;
    /// Get icon as a valid SVG string
    fn get_icon(&self) -> Result<String, anyhow::Error>;
    fn get_description(&self) -> String;
    /// Whether the component should keep send packets
    /// out in the order they were received
    fn is_ordered(&self) -> bool;
    /// Whether the component should activate when it receives packets
    fn should_activate_on_input(&self) -> bool;
    /// Bracket forwarding rules. By default we forward
    fn get_forward_brackets(&self) -> HashMap<String, Vec<String>>;
    fn get_metadata(&self) -> Map<String, Value>;
    fn get_runtime(&self) -> Runtime;
    fn get_source_directory(&self) -> String;
    fn is_subgraph(&self) -> bool;

    fn as_any(&self) -> &dyn Any;
}


pub struct ProviderRunner {
    pub runner_id: String,
    pub runner_func: Box<RunFunc>,
    pub platforms: Vec<Platform>,
}

pub(crate) struct DynProviderRunner {
    pub runner_id: String,
    pub runner_func: Arc<Mutex<DynRunFunc>>,
    pub platforms: Vec<Platform>,
}

pub trait Provider: Send + Sync {
    /// Base directory of the project workspace
    fn set_workspace(&mut self, dir: String);
    /// Must return a valid SVG string
    fn get_logo(&self) -> Result<String, anyhow::Error>;
    /// Get ID of this provider
    fn get_id(&self) -> String;
    /// List the IDs of all the components of this provider
    fn list_components(&mut self) -> Result<Vec<String>, anyhow::Error>;
    /// Load an instance of a specific component. If the
    /// registered component is a FBP graph, it will
    /// be loaded as an instance of a subgraph
    /// component.
    fn load_component(
        &mut self,
        id: String,
    ) -> Result<(&Box<dyn ProviderComponent>, ProviderRunner), anyhow::Error>;

    /// Get the process of a component
    fn get_process(
        &self,
        component: &Box<dyn ProviderComponent>,
    ) -> Result<ProviderRunner, anyhow::Error>;
}

impl ProviderComponent for ComponentSource {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_inports(&self) -> HashMap<String, PortOptions> {
        self.inports
            .clone()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    PortOptions::deserialize(json!(v.clone())).unwrap(),
                )
            })
            .collect()
    }

    fn get_outports(&self) -> HashMap<String, PortOptions> {
        self.outports
            .clone()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    PortOptions::deserialize(json!(v.clone())).unwrap(),
                )
            })
            .collect()
    }

    fn get_icon(&self) -> Result<String, anyhow::Error> {
        // Todo: Validate and return SVG string
        Ok(self.icon.clone())
    }

    fn get_description(&self) -> String {
        self.description.clone()
    }

    fn is_ordered(&self) -> bool {
        self.ordered
    }

    fn should_activate_on_input(&self) -> bool {
        self.activate_on_input
    }

    fn get_forward_brackets(&self) -> HashMap<String, Vec<String>> {
        self.forward_brackets.clone()
    }

    fn get_metadata(&self) -> Map<String, Value> {
        self.metadata.clone()
    }

    fn get_runtime(&self) -> Runtime {
        self.runtime.clone()
    }

    fn get_source_directory(&self) -> String {
        self.source_dir.clone()
    }

    fn is_subgraph(&self) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }
}

impl ProviderComponent for Graph {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_inports(&self) -> HashMap<String, PortOptions> {
        HashMap::from_iter([(
            String::from("in"),
            PortOptions {
                required: true,
                triggering: true,
                ..PortOptions::default()
            },
        )])
    }

    fn get_outports(&self) -> HashMap<String, PortOptions> {
        HashMap::from_iter([(String::from("out"), PortOptions::default())])
    }

    fn get_icon(&self) -> Result<String, anyhow::Error> {
        if let Some(icon) = self.properties.get("icon") {
            if icon.is_string() {
                return Ok(icon.as_str().unwrap().to_string());
            }
        }
        // Todo: provide default graph icon
        Ok("".to_owned())
    }

    fn get_description(&self) -> String {
        if let Some(description) = self.properties.get("description") {
            if description.is_string() {
                return description.as_str().unwrap().to_string();
            }
        }
        "This is a zflow subgraph that contains network of processes".to_owned()
    }

    fn is_ordered(&self) -> bool {
        false
    }

    fn should_activate_on_input(&self) -> bool {
        true
    }

    fn get_forward_brackets(&self) -> HashMap<String, Vec<String>> {
        HashMap::new()
    }

    fn get_metadata(&self) -> Map<String, Value> {
        self.properties.clone()
    }

    fn get_runtime(&self) -> Runtime {
        if let Some(meta) = self.properties.get("runtime") {
            if let Ok(runtime) = Runtime::deserialize(meta) {
                return runtime;
            }
        }
        return Runtime {
            provider_id: "@zflow/builtin".to_owned(),
            runner_id: "@zflow/graph".to_owned(),
            platform: Platform::System,
        };
    }

    fn get_source_directory(&self) -> String {
        "/".to_owned()
    }

    fn is_subgraph(&self) -> bool {
        true
    }

    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }
}

pub(crate) struct BuiltInProvider {
    pub(crate) components: HashMap<String, Box<dyn ProviderComponent>>,
    pub(crate) dynamic_runners: HashMap<String, DynProviderRunner>,
    pub(crate) workspace: String,
}

impl Provider for BuiltInProvider {
    fn set_workspace(&mut self, dir: String) {
        self.workspace = dir;
    }

    fn get_logo(&self) -> Result<String, anyhow::Error> {
        Err(anyhow::Error::msg("Not implemented"))
    }

    fn get_id(&self) -> String {
        BUILT_IN_PROVIDER_ID.to_owned()
    }

    fn list_components(&mut self) -> Result<Vec<String>, anyhow::Error> {
        self.load_components()?;
        let keys: Vec<String> = self.components.iter().map(|(k, _)| k.clone()).collect();
        Ok(keys)
    }

    fn load_component(
        &mut self,
        id: String,
    ) -> Result<(&Box<dyn ProviderComponent>, ProviderRunner), anyhow::Error> {
        self.load_components()?;

        if let Some(component) = self.components.get(&id) {
            let runner = self.get_process(component)?;
            return Ok((component, runner));
        }
        let err = format!("Could not find component for provider {}", self.get_id());
        log!(target:format!("Provider {}", self.get_id()).as_str(),log::Level::Debug, "Could not find component with ID {}", id);
        Err(anyhow::Error::msg(err))
    }

    fn get_process(
        &self,
        component: &Box<dyn ProviderComponent>,
    ) -> Result<ProviderRunner, anyhow::Error> {
        let runtime = component.get_runtime();
        if runtime.provider_id != self.get_id() {
            return Err(anyhow::Error::msg("Component provider mismatch"));
        }

        let source = component.get_source_directory();
        let path = PathBuf::from(source.clone());
        if !path.is_file() || !path.exists() {
            return Err(anyhow::Error::msg("Source is invalid"));
        }

        #[cfg(feature = "plugin_host")]
        {
            if runtime.runner_id == WASM_RUNNER_ID.to_owned() {
                let manifest = Manifest::new([if is_url(&source) {
                    Wasm::url(source)
                } else {
                    Wasm::file(source)
                }]);

                let component: &ComponentSource = component
                    .as_any()
                    .downcast_ref::<ComponentSource>()
                    .unwrap();

                let runner = crate::wasm::get_sys_wasm_runner(
                    component.clone(),
                    "process",
                    manifest,
                    false,
                )?;

                return Ok(runner);
            }
        }

        if let Some(runner) = self.dynamic_runners.get(&runtime.runner_id) {
            if !runner.platforms.contains(&runtime.platform) {
                return Err(anyhow::Error::msg(
                    "Provider does not support target platform",
                ));
            }

            let process = runner.runner_func.clone();

            let is_graph = component.is_subgraph();

            let runner_func = Box::new(move |handle| {
                if !is_graph {
                    if let Ok(process) = process.clone().try_lock().as_mut() {
                        return (process)(source.clone(), handle);
                    }
                }
                Err(ProcessError(format!("Runner: Unknown state")))
            });

            return Ok(ProviderRunner {
                runner_id: runner.runner_id.clone(),
                platforms: runner.platforms.clone(),
                runner_func,
            });
        }
        Err(anyhow::Error::msg("Process runner not found"))
    }
}

impl BuiltInProvider {
    pub fn new() -> Result<Self, anyhow::Error> {
        let mut dynamic_runners: HashMap<String, DynProviderRunner> = HashMap::new();

        if let Some(path) = option_env!("ZFLOW_QUICKJS_RUNNER_LIB") {
            unsafe {
                let lib = Library::new(path).unwrap();
                dynamic_runners.insert(
                    QUICKJS_RUNNER_ID.to_owned(),
                    DynProviderRunner {
                        runner_id: QUICKJS_RUNNER_ID.to_owned(),
                        platforms: vec![Platform::System],
                        runner_func: Arc::new(Mutex::new(
                            move |src: String, handle: Arc<Mutex<ProcessHandle>>| {
                                let runner: Symbol<UnsafeRunFunc> = lib.get(b"run").unwrap();
                                let source = src.as_bytes();

                                let res =
                                    std::ptr::from_mut(&mut Result::Ok(ProcessResult::default()));
                                (runner)(
                                    &source[0],
                                    source.len(),
                                    &handle as *const Arc<Mutex<ProcessHandle>>,
                                    res,
                                );

                                res.read()
                            },
                        )),
                    },
                );
            }
        }

        Ok(Self {
            components: HashMap::new(),
            dynamic_runners,
            workspace: "/".to_string(),
        })
    }

    pub(crate) fn load_components(&mut self) -> io::Result<()> {
        let base_dir = PathBuf::from_str(&self.workspace).unwrap();
        let provider_id = self.get_id();
        // Recursively look up all component directories
        let components: HashMap<String, Box<dyn ProviderComponent>> = visit_dirs(
            &base_dir,
            &|entry| {
                if entry.is_file() {
                    let file = File::open(entry.clone()).unwrap();
                    let reader = BufReader::new(file);
                    if let Some(name) = entry.file_name() {
                        if name == "zflow.json" {
                            let mut de = serde_json::Deserializer::from_reader(reader);
                            let mut package = Package::deserialize(&mut de)
                            .expect("Unexpected problem while attempting to deserialize zflow.json config file");

                            let components: HashMap<String, Box<dyn ProviderComponent>> = package
                                .components
                                .par_iter_mut()
                                .map(|component| {
                                    if component.runtime.provider_id == provider_id {
                                        let mut parent = entry.parent().unwrap().to_path_buf();
                                        parent.push(component.source.clone());

                                        (*component).source_dir =
                                            parent.to_str().unwrap().to_owned();

                                        let entry: Box<ComponentSource> =
                                            Box::new(component.clone());

                                        return Some((
                                            build_node_id(&package.package_id, &component.name),
                                            entry,
                                        ));
                                    } else {
                                        None
                                    }
                                })
                                .filter(|comp| comp.is_some())
                                .map(|comp| comp.unwrap())
                                .map(|(k, v)| {
                                    let comp: Box<dyn ProviderComponent> = v.clone();
                                    (k, comp)
                                })
                                .collect();

                            return Some(components);
                        } else if name == "graph.json" {
                            let mut de = serde_json::Deserializer::from_reader(reader);
                            let graph_json = GraphJson::deserialize(&mut de).expect(
                            "Unexpected problem while attempting to deserialize graph.json file",
                        );
                            let graph = Graph::from_json(
                                graph_json.clone(),
                                Some(graph_json.properties.clone()),
                            );
                            let mut components: HashMap<String, Box<dyn ProviderComponent>> =
                                HashMap::new();
                            let component: Box<dyn ProviderComponent> = Box::new(graph.clone());

                            components.insert(build_node_id("graph", &graph.name), component);

                            return Some(components);
                        } else {
                            return None;
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
        )?;

        self.components.extend(components);

        Ok(())
    }

    /// List all IDs of runners in this provider.
    /// Runners are functions that dynamically run zflow components from source
    pub(crate) fn list_dynamic_runners(&self) -> Vec<String> {
        self.dynamic_runners
            .par_iter()
            .map(|(k, _)| k.clone())
            .collect()
    }
    /// Get a specific runner by ID
    pub(crate) fn get_dynamic_runner(&self, runner_id: &str) -> Option<&DynProviderRunner> {
        self.dynamic_runners.get(runner_id)
    }

    /// Register a Dynamic Process Runner
    /// Dynamic Runners are capable of running zflow nodes directly from source
    pub fn register_dynamic_runner(&mut self, id: &str, mut runner: Box<DynRunFunc>) {
        self.dynamic_runners.insert(
            id.to_owned(),
            DynProviderRunner {
                runner_id: id.to_owned(),
                runner_func: Arc::new(Mutex::new(move |src, handle: Arc<Mutex<ProcessHandle>>| {
                    (runner)(src, handle)
                })),
                platforms: vec![Platform::System],
            },
        );
    }

    /// With the `register_component` method you can register
    /// a ZFlow Component constructor or subgraph
    /// as a component available for loading.
    ///
    /// WARNING: If component of the same namespace and name exists, this function will override it
    pub fn register_component(
        &mut self,
        namespace: &str,
        name: &str,
        component: Box<dyn ProviderComponent>,
    ) -> Result<(), anyhow::Error> {
        self.list_components()?;

        let f_name = build_node_id(namespace, name);

        self.components.insert(f_name, component);

        Ok(())
    }
}

pub fn visit_dirs<'a>(
    dir: &Path,
    cb: &dyn Fn(PathBuf) -> Option<HashMap<String, Box<dyn ProviderComponent>>>,
) -> io::Result<HashMap<String, Box<dyn ProviderComponent>>> {
    let mut _dir = dir;
    let mut components = HashMap::new();
    for entry in WalkDir::new(_dir) {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            _dir = &path;
            continue;
        } else {
            if let Some(comp) = cb(path) {
                components.extend(comp);
            }
        }
    }
    Ok(components)
}

pub fn build_node_id(package: &str, name: &str) -> String {
    if package.is_empty() || package == name {
        return name.to_owned();
    }
    if let Some(_name) = package.split("/").nth(1) {
        if _name == name {
            return package.to_owned();
        }
    }

    let mut prefix = PathBuf::from_str(package).unwrap();
    prefix.push(name);
    prefix.as_path().to_str().unwrap().to_string()
}
