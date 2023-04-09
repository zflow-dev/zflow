use std::{
    any::Any,
    collections::HashMap,
    sync::{Arc, Mutex},
};


use poll_promise::Promise;
use regex::Regex;
use serde_json::Value;
use zflow::graph::graph::Graph;

use crate::{
    component::{BaseComponentTrait, ComponentTrait},
    default_component_registry::DefaultRegistry,
    graph_component::{GraphComponent},
};
use zflow::graph::types::GraphJson;

/// Registry is a way to tell the Component Loader where to discover, load and execute custom components
pub trait RuntimeRegistry<T>
where
    T: ComponentTrait
{
    fn set_source(
        &mut self,
        namespace: &str,
        component_name: &str,
        source: ComponentSource,
    ) -> Result<(), String>;
    fn get_source(&self, component_name: &str) -> Option<ComponentSource>;
    fn get_languages(&mut self) -> Result<Vec<String>, String>;
    /// Register custom component loaders
    fn register(
        &mut self,
        loader: &mut ComponentLoader<T>,
    ) -> Promise<Result<HashMap<String, &'static (dyn ComponentDefinition<T> + Sync + Send)>, String>>;
    fn dynamic_load(&mut self, component_name: &str, path: &str) -> Result<Arc<Mutex<T>>, String>;
}

pub type ComponentList<T> =
    Arc<Mutex<HashMap<String, &'static (dyn ComponentDefinition<T> + Sync + Send)>>>;

pub struct ComponentSource {
    pub name: String,
    pub code: String,
    pub language: String,
}

/// Useful for when you want to use structs as a component provider
pub trait ModuleComponent<T: BaseComponentTrait>: Sync + Send {
    fn get_component(&self) -> ComponentFactory<T>;
}

type ComponentFactory<T> =
    Box<dyn Fn(Option<HashMap<String, Value>>) -> Result<Arc<Mutex<T>>, String> + Send + Sync>;

pub trait ComponentDefinition<T: ComponentTrait >: Send {}

impl<T: ComponentTrait> ComponentDefinition<T> for ComponentFactory<T> {}
impl<T: ComponentTrait> ComponentDefinition<T> for Box<dyn ModuleComponent<T>> {}
impl<T: ComponentTrait> ComponentDefinition<T> for String {}
impl<T: ComponentTrait> ComponentDefinition<T> for &str {}
impl<T: ComponentTrait> ComponentDefinition<T> for GraphJson {}

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

#[derive(Clone)]
pub struct ComponentLoader<T: ComponentTrait > {
    pub(crate) components: ComponentList<T>,
    pub options: ComponentLoaderOptions,
    pub base_dir: String,
    pub library_icons: HashMap<String, String>,
    pub(crate) processing: Option<
        Arc<
            Mutex<
                Promise<
                    Result<
                        HashMap<String, &'static (dyn ComponentDefinition<T> + Sync + Send)>,
                        String,
                    >,
                >,
            >,
        >,
    >,
    pub ready: bool,
    pub registry: Arc<Mutex<dyn RuntimeRegistry<T> + Send>>,
}

impl<T: ComponentTrait > ComponentLoader<T> {
    pub fn new(
        base_dir: &str,
        options: ComponentLoaderOptions,
        registry: Option<Arc<Mutex<dyn RuntimeRegistry<T> + Send>>>,
    ) -> Self {
        let mut _registry: Arc<Mutex<dyn RuntimeRegistry<T> + Send>> =
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
    pub fn list_components(&mut self) -> Result<ComponentList<T>, String> {
        if self.processing.is_some() {
            if let Some(processing) = self.processing.as_mut() {
                if let Ok(processing) = processing.clone().try_lock() {
                    loop {
                        let result = processing
                            .ready()
                            .expect("expected processing loaders to be ready");
                        if let Ok(complist) = result {
                            self.ready = true;
                            self.processing = None;

                            self.components = Arc::new(Mutex::new(HashMap::new()));
                            if let Ok(comp) = self.components.clone().try_lock().as_mut() {
                                for (name, def) in complist {
                                    comp.insert(name.clone(), def.to_owned());
                                }
                            }
                        }
                        return Ok(self.components.clone());
                    }
                }
            }
        } else if self.ready {
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
            self.processing = Some(Arc::new(Mutex::new(
                self.registry
                    .clone()
                    .try_lock()
                    .expect("Could not instantiate registry")
                    .register(self),
            )))
        }
        return Ok(self.components.clone());
    }

    /// Load an instance of a specific component. If the
    /// registered component is a JSON or FBP graph, it will
    /// be loaded as an instance a subgraph
    /// component.
    pub fn load(
        &mut self,
        name: &str,
        metadata: Option<HashMap<String, Value>>,
    ) -> Result<Arc<Mutex<T>>, String>  {
        let mut component = None;

        if !self.ready {
            let res = ComponentLoader::<T>::list_components(self);
            if res.is_err() {
                return Err(res.err().unwrap());
            }
            return self.load(name, metadata);
        }

        // let components:HashMap<String, Box<dyn ComponentDefinition<T> + Send + Sync>> = HashMap::from_iter(loader.components.clone().into());
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

        let comp = component.expect(format!("Component {} could not be loaded", name).as_str())
            as &dyn Any;
        if let Some(graph) = comp.downcast_ref::<GraphJson>() {
            return self.load_graph(name, graph.clone(), metadata);
        }

        let _instance = self.create_component(name, comp, metadata)?.clone();
        if let Ok(instance) = _instance.clone().try_lock().as_mut() {
            if instance.get_name() == Some("Graph".to_owned()) {
                instance.set_base_dir(self.base_dir.clone());
            }

            if !name.is_empty() {
                instance.set_name(name.to_owned());
            }

            self.set_icon("name", _instance.clone());

            return Ok(_instance.clone());
        }

        Err(format!("Component {} could not be loaded", name))
    }

    /// Creates an instance of a component.
    pub fn create_component(
        &self,
        name: &str,
        component: &dyn Any,
        metadata: Option<HashMap<String, Value>>,
    ) -> Result<Arc<Mutex<T>>, String> {
        if let Ok(registry) = self.registry.clone().try_lock().as_mut() {
            if let Some(implementation) = component.downcast_ref::<&str>() {
                return registry.dynamic_load(*implementation, &self.base_dir);
            } else if let Some(implementation) = component.downcast_ref::<String>() {
                return registry.dynamic_load(implementation, &self.base_dir);
            } else if let Some(implementation) = component.downcast_ref::<ComponentFactory<T>>() {
                return (implementation)(metadata);
            } else if let Some(implementation) =
                component.downcast_ref::<Box<dyn ModuleComponent<T>>>()
            {
                return ((implementation).get_component())(metadata);
            }
        }
        Err(format!("Could not load component {}. Ensure that component definition is of either ComponentFactory<dyn ComponentTrait>, ModuleComponent<dyn ComponentTrait>, &str, String, or GraphJson", name))
    }

    /// Load a graph as a subgraph component instance
    pub fn load_graph(
        &mut self,
        name: &str,
        graph_json: GraphJson,
        metadata: Option<HashMap<String, Value>>,
    ) -> Result<Arc<Mutex<T>>, String>    {
        let binding = self.components.clone();
        let binding = binding
            .try_lock()
            .expect("Expected component list container");
        let graph_component = binding
            .get("Graph")
            .expect("Expected Graph in components list");
        let graph = self
            .create_component(name, graph_component as &dyn Any, metadata)?
            .clone();
        if let Ok(g) = graph.try_lock().as_mut() {
            g.set_loader(self.clone());
            g.set_base_dir(self.base_dir.clone());
            g.get_inports_mut().ports.remove("graph");
            self.set_icon(name, graph.clone());

            GraphComponent::setup_graph(graph.clone(), &graph_json)?;
            return Ok(graph.clone());
        }

        Err(format!("Could not load graph of {}", name))
    }
    /// Set icon for the component instance. If the instance
    /// has an icon set, then this is a no-op. Otherwise we
    /// determine an icon based on the module it is coming
    /// from, or use a fallback icon separately for subgraphs
    /// and elementary components.
    pub fn set_icon(&mut self, name: &str, instance: Arc<Mutex<T>>) {
        if let Ok(instance) = instance.try_lock().as_mut() {
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
    fn register_component(
        &mut self,
        namespace: &str,
        name: &str,
        component: impl ComponentDefinition<T> + Send + Sync + 'static,
    ) {
        let f_name = normalize_name(namespace, name);
        self.components
            .clone()
            .try_lock()
            .as_mut()
            .expect("Expected component list container")
            .insert(f_name, Box::leak(Box::new(component)));
    }

    /// Register new graphs as loadable components.
    pub fn register_graph(&mut self, namespace: &str, name: &str, graph: Graph) {
        self.register_component(namespace, name, graph.clone().to_json());
    }

    /// With `set_source` you can register a component by providing
    /// a source code string. Supported languages and techniques
    /// depend on the runtime environment registry provided
    pub fn set_source(&mut self, namespace: &str, name: &str, source: ComponentSource) -> Result<(), String> {
        if !self.ready {
            self.list_components()?;
            return self.set_source(namespace, name, source);
        }
        if let Ok(registry) = self.registry.clone().try_lock().as_mut() {
            return registry.set_source(namespace, name, source)
        }
        Err(format!("Could not set code source for component '{}'", name))
    }
    /// Allows fetching the source code of a registered
    /// component as a string.
    pub fn get_source(
        &mut self,
        name: &str
    ) -> Option<ComponentSource> {
        if !self.ready {
            let _ = self.list_components();
            return self.get_source(name);
        }
        if let Ok(registry) = self.registry.clone().try_lock().as_mut() {
            return registry.get_source(name);
        }
        None
    }

    pub fn get_runtime_languages(
        &mut self
    ) -> Result<Vec<String>, String> {
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
    let re = Regex::new(r"^zflow-").unwrap();
    let result = re.replace_all(name, "");
    return result.to_string();
}
pub fn normalize_name(package_id: &str, name: &str) -> String {
    let prefix = get_prefix(name);
    let mut f_name = format!("{}/{}", prefix, name);
    if package_id.is_empty() {
        f_name = name.to_owned();
    }

    f_name
}
