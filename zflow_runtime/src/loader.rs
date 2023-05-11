use std::{
    borrow::Borrow,
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
};

use poll_promise::Promise;
use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};
use zflow_graph::types::GraphJson;

use crate::{
    component::{Component, GraphDefinition, ModuleComponent},
    process,
    registry::{ComponentSource, DefaultRegistry, RuntimeRegistry},
    wasm::WasmComponent,
};

use is_url::is_url;

#[derive(Debug, Default, Clone)]
pub struct ComponentLoaderOptions {
    pub cache: bool,
    pub discover: bool,
    pub recursive: bool,
    // pub runtimes: Vec<String>,
    pub manifest: HashMap<String, Value>,
}

/// ## The ZFlow Component Loader
///
/// The Component Loader is responsible for discovering components
/// available in the running system, as well as for instantiating
/// them.
// #[derive(Clone)]
pub struct ComponentLoader {
    pub(crate) components: Arc<Mutex<HashMap<String, Box<dyn GraphDefinition>>>>,
    pub options: ComponentLoaderOptions,
    pub base_dir: String,
    pub library_icons: HashMap<String, String>,
    pub(crate) processing:
        Option<Vec<Promise<Result<HashMap<String, Box<dyn GraphDefinition>>, String>>>>,
    pub ready: bool,
    pub registry: Arc<Mutex<dyn RuntimeRegistry + Send>>,
}

impl Clone for ComponentLoader {
    fn clone(&self) -> Self {
        Self {
            components: self.components.clone(),
            options: self.options.clone(),
            base_dir: self.base_dir.clone(),
            library_icons: self.library_icons.clone(),
            processing: None,
            ready: self.ready.clone(),
            registry: self.registry.clone(),
        }
    }
}

impl ComponentLoader {
    pub fn new(
        base_dir: &str,
        options: ComponentLoaderOptions,
        registry: Option<Arc<Mutex<dyn RuntimeRegistry + Send>>>,
    ) -> Self {
        let mut _registry: Arc<Mutex<dyn RuntimeRegistry + Send>> =
            Arc::new(Mutex::new(DefaultRegistry::default()));
        if let Some(reg) = registry {
            _registry = reg.clone();
        }
        Self {
            components: Arc::new(Mutex::new(HashMap::new())),
            options,
            base_dir: base_dir.to_owned(),
            library_icons: HashMap::new(),
            processing: None,
            ready: false,
            registry: _registry,
        }
    }

    /// Get the list of all available components
    pub fn list_components(
        &mut self,
    ) -> Result<Arc<Mutex<HashMap<String, Box<dyn GraphDefinition>>>>, String> {
        if let Some(processings) = self.processing.as_mut() {
            loop {
                let processing = processings.remove(0);
                let p = processing.block_until_ready();

                let comps = self.components.clone();
                if let Ok(comp) = comps.clone().try_lock().as_mut() {
                    if let Ok(p) = p {
                        for (name, def) in p {
                            comp.insert(name.clone(), dyn_clone::clone_box(&**def));
                        }
                    }
                }

                if processings.is_empty() {
                    self.ready = true;
                    self.processing = None;
                    return Ok(self.components.clone());
                }
            }
        }
        if self.ready {
            let binding = self.components.clone();
            let components = binding
                .try_lock()
                .expect("Expected component list container");
            if !components.is_empty() {
                return Ok(self.components.clone());
            } else {
                return Err("Could not list components from loader".to_owned());
            }
        } else {
            self.components = Arc::new(Mutex::new(HashMap::new()));
            self.ready = false;
            self.processing = Some(vec![self
                .registry
                .clone()
                .try_lock()
                .expect("Could not instantiate registry")
                .register(self)]);
            return self.list_components();
        }
    }

    /// Load an instance of a specific component. If the
    /// registered component is a FBP graph, it will
    /// be loaded as an instance a subgraph
    /// component.
    pub fn load(&mut self, name: &str, metadata: Value) -> Result<Arc<Mutex<Component>>, String> {
        let mut component = None;

        if !self.ready {
            let res = ComponentLoader::list_components(self);
            if res.is_err() {
                return Err(res.err().unwrap());
            }
            return self.load(name, metadata);
        }

        let binding = self.components.clone();
        let components = binding
            .try_lock()
            .expect("Expected component list container");

        if !components.contains_key(name) {
            for i in 0..components.keys().len() {
                if let Some(component_name) = components.keys().collect::<Vec<&String>>().get(i) {
                    if component_name.split("/").collect::<Vec<&str>>()[1] == name {
                        component = components.get(*component_name);
                        break;
                    }
                }
            }
        } else {
            component = components.get(name);
        }

        if component.is_none() {
            return Err(format!(
                "Component {} not available with base {}",
                name, self.base_dir
            ));
        }

        if let Some(graph) = component.unwrap().to_any().downcast_ref::<GraphJson>() {
            return self.load_graph(name, graph, metadata);
        }

        let _instance = self
            .create_component(name, &**component.unwrap(), metadata)?
            .clone();
        let binding = _instance.clone();
        let mut binding = binding.try_lock();
        let instance = binding
            .as_mut()
            .expect(format!("Component {} could not be loaded", name).as_str());
        if instance.get_name() == Some("Graph".to_owned()) || instance.is_subgraph() {
            instance.set_base_dir(self.base_dir.clone());
        }

        if !name.is_empty() {
            instance.set_name(name.to_owned());
        }

        // self.set_icon(name, _instance.clone());

        // See if component has an icon
        if !instance.get_icon().is_empty() {
            return Ok(_instance.clone());
        }
        // See if library has an icon
        let v: Vec<&str> = name.split("/").collect();
        let library = v[0];
        let component_name = v.get(1);

        if let Some(prefix) = self.get_library_icon(library) {
            if component_name.is_some() {
                instance.set_icon(prefix.to_owned());
                return Ok(_instance.clone());
            }
        }

        // See if instance is a subgraph
        if instance.is_subgraph() {
            instance.set_icon("჻".to_owned());
            return Ok(_instance.clone());
        }

        instance.set_icon("⚙".to_owned());

        return Ok(_instance.clone());
    }

    /// Creates an instance of a component.
    pub fn create_component(
        &mut self,
        name: &str,
        component: &dyn GraphDefinition,
        metadata: Value,
    ) -> Result<Arc<Mutex<Component>>, String> {
        // If a string was specified, attempt to load it dynamically
        if let Some(path) = component.to_any().downcast_ref::<String>() {
            if let Ok(registry) = self.registry.clone().try_lock().as_mut() {
                if is_url(path) {
                    return registry.dynamic_load(name, path, metadata);
                }

                if Path::new(path).extension().is_some() || path.contains(std::path::is_separator) {
                    if let Ok(buf) = PathBuf::from_str(&self.base_dir).as_mut() {
                        buf.push(path);
                        return registry.dynamic_load(
                            name,
                            buf.as_os_str()
                                .to_str()
                                .expect("expected valid path string"),
                            metadata,
                        );
                    }
                }
            }
        }

        // Load and create a wasm component
        if let Some(wasm_component) = component.to_any().downcast_ref::<WasmComponent>() {
            let mut wasm = wasm_component.clone();
            wasm.base_dir = if wasm.base_dir == "/" {
                let mut buf = PathBuf::from(self.base_dir.clone());
                buf.push(wasm.base_dir);
                buf.to_str().unwrap().to_owned()
            } else {
                self.base_dir.clone()
            };
            return Ok(Component::from_instance(
                wasm_component
                    .clone()
                    .with_metadata(metadata)
                    .as_component()?,
            ));
        }

        // check if it's a component instance
        if let Some(instance) = component.to_any().downcast_ref::<Component>() {
            return Ok(Component::from_instance(instance.clone()));
        }

        Err("".to_owned())
    }

    /// Load a graph as a ZFlow subgraph component instance
    pub fn load_graph(
        &mut self,
        name: &str,
        component: &dyn GraphDefinition,
        metadata: Value,
    ) -> Result<Arc<Mutex<Component>>, String> {
        if let Ok(components) = self.components.clone().try_lock().as_mut() {
            if let Some(graph_component) = components.get_mut("Graph") {
                let _instance = self
                    .create_component(name, &**graph_component, metadata)
                    .expect("expected to create subgraph component instance");
                if let Ok(instance) = _instance.clone().try_lock().as_mut() {
                    instance.set_base_dir(self.base_dir.clone());
                    instance.set_loader(self.clone());
                    instance.in_ports.ports.remove("graph");
                    if !instance.get_icon().is_empty() {
                        return Ok(_instance.clone());
                    }
                    // See if library has an icon
                    let v: Vec<&str> = name.split("/").collect();
                    let library = v[0];
                    let component_name = v.get(1);

                    if let Some(prefix) = self.get_library_icon(library) {
                        if component_name.is_some() {
                            instance.set_icon(prefix.to_owned());
                            return Ok(_instance.clone());
                        }
                    }
                    instance.set_icon("჻".to_owned());

                    Component::setup_graph(_instance.clone(), Some(component))?;
                }
                return Ok(_instance.clone());
            }
        }
        Err(format!("Could not load Graph component {}", name))
    }

    /// Set icon for the component instance. If the instance
    /// has an icon set, then this is a no-op. Otherwise we
    /// determine an icon based on the module it is coming
    /// from, or use a fallback icon separately for subgraphs
    /// and elementary components.
    pub fn set_icon(&mut self, name: &str, instance: Arc<Mutex<Component>>) {
        if let Ok(instance) = instance.clone().try_lock().as_mut() {
            // See if component has an icon
            if !instance.get_icon().is_empty() {
                return;
            }
            // See if library has an icon
            let v: Vec<&str> = name.split("/").collect();
            let library = v[0];
            let component_name = v.get(1);

            if let Some(prefix) = self.get_library_icon(library) {
                if component_name.is_some() {
                    instance.set_icon(prefix.to_owned());
                    return;
                }
            }

            // See if instance is a subgraph
            if instance.is_subgraph() {
                instance.set_icon("჻".to_owned());
                return;
            }

            instance.set_icon("⚙".to_owned());
        }
    }

    pub fn get_library_icon(&self, prefix: &str) -> Option<&String> {
        self.library_icons.get(prefix)
    }
    pub fn set_library_icon(&mut self, prefix: &str, icon: &str) {
        self.library_icons
            .insert(prefix.to_owned(), icon.to_owned());
    }

    /// ### Registering components at runtime
    ///
    /// In addition to components discovered by the loader,
    /// it is possible to register components at runtime.
    ///
    /// With the `register_component` method you can register
    /// a ZFlow Component constructor or factory method
    /// as a component available for loading.
    pub fn register_component(
        &mut self,
        namespace: &str,
        name: &str,
        component: impl GraphDefinition,
    ) {
        let f_name = normalize_name(namespace, name);
        self.components
            .clone()
            .try_lock()
            .as_mut()
            .expect("Expected component list container")
            .insert(f_name, Box::new(component));
    }

    /// With `register_loader` you can register custom component
    /// loaders. They will be called immediately and can register
    /// any components or graphs they wish.
    pub fn register_loader(
        &mut self,
        loader: impl FnOnce(
            &mut Self,
        )
            -> Promise<Result<HashMap<String, Box<dyn GraphDefinition>>, String>>,
    ) {
        let loading_components = (loader)(self);
        if let Some(processing) = self.processing.as_mut() {
            processing.push(loading_components);
            return;
        }

        self.processing = Some(vec![loading_components]);
    }

    /// With `set_source` you can register a component by providing
    /// a source code string. Supported languages and techniques
    /// depend on the runtime environment registry provided
    pub fn set_source(
        &mut self,
        namespace: &str,
        name: &str,
        source: ComponentSource,
    ) -> Result<(), String> {
        if !self.ready {
            self.list_components()?;
            return self.set_source(namespace, name, source);
        }
        if let Ok(registry) = self.registry.clone().try_lock().as_mut() {
            return registry.set_source(namespace, name, source);
        }
        Err(format!(
            "Could not set code source for component '{}'",
            name
        ))
    }
    /// Allows fetching the source code of a registered
    /// component as a string.
    pub fn get_source(&mut self, name: &str) -> Option<ComponentSource> {
        if !self.ready {
            let _ = self.list_components();
            return self.get_source(name);
        }
        if let Ok(registry) = self.registry.clone().try_lock().as_mut() {
            return registry.get_source(name);
        }
        None
    }

    pub fn get_runtime_languages(&mut self) -> Result<Vec<String>, String> {
        if !self.ready {
            self.list_components()?;
            return self.get_runtime_languages();
        }

        if let Ok(registry) = self.registry.clone().try_lock().as_mut() {
            return registry.get_languages();
        }
        Err("Could not fetch runtime languages".to_string())
    }
}

pub fn get_prefix(name: &str) -> String {
    if name == "zflow" {
        return "".to_owned();
    }
    if name.starts_with("@") {
        let re = Regex::new(r"^@[a-z-]+/").unwrap();
        let result = re.replace_all(name, "");
        return get_prefix(result.to_string().as_str());
    }
    let re = Regex::new(r"^zflow-").unwrap();
    let result = re.replace_all(name, "");
    return result.to_string();
}
pub fn normalize_name(package_id: &str, name: &str) -> String {
    let prefix = get_prefix(name);
    let mut f_name = if prefix == name {
        name.to_owned()
    } else {
        format!("{}/{}", prefix, name)
    };
    if !package_id.is_empty() {
        f_name = format!("{}/{}", package_id, name);
    }

    f_name
}

#[cfg(test)]
mod tests {
    use crate::loader::get_prefix;
    use beady::scenario;

    #[scenario]
    #[test]
    fn test_prefix() {
        'given_normalizing_names: {
            'then_should_return_simple_module_names_as_is: {
                assert_eq!(get_prefix("foo"), "foo");
            }
            'then_should_return_empty_for_zflow_core: {
                assert_eq!(get_prefix("zflow"), "");
            }
            'then_should_strip_zflow_with_dash: {
                assert_eq!(get_prefix("zflow-color"), "color");
            }
            'then_should_strip_zflow_namespace: {
                assert_eq!(get_prefix("@zflow/color"), "color");
            }
            'then_should_strip_zflow_namespace_and_zflow_with_dash: {
                assert_eq!(get_prefix("@zflow/zflow-color"), "color");
            }
        }
    }
}
