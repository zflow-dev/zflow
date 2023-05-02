use std::{
    collections::HashMap,
    path::{PathBuf, Path},
    sync::{Arc, Mutex},
};

use poll_promise::Promise;
use serde::{Deserialize, Serialize};

use crate::{
    component::{Component, GraphDefinition},
    loader::ComponentLoader,
    port::PortOptions,
};

use is_url::is_url;

#[derive(Serialize, Deserialize)]
pub struct RemoteComponent {
    pub inports: HashMap<String, PortOptions>,
    pub outports: HashMap<String, PortOptions>,
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
    pub base_dir: String,
    pub process: ComponentSource,
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSource {
    pub name: String,
    pub code: String,
    pub language: String,
}
/// Registry is a way to tell the Component Loader where to discover, load and execute custom components
pub trait RuntimeRegistry {
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
        loader: &mut ComponentLoader,
    ) -> Promise<Result<HashMap<String, Box<dyn GraphDefinition>>, String>>;

    fn dynamic_load(
        &mut self,
        component_name: &str,
        path: &str,
    ) -> Result<Arc<Mutex<Component>>, String>;
}

#[derive(Clone)]
pub struct DefaultRegistry {
    supported_languages: Vec<String>,
    source_map: HashMap<String, ComponentSource>,
}

impl Default for DefaultRegistry {
    fn default() -> Self {
        Self {
            supported_languages: vec![
                "Javascript".to_owned(),
                "Typescript".to_owned(),
                "WebAssembly".to_owned(),
                "Wren".to_owned(),
                "Lua".to_owned(),
                "FBP".to_owned(),
            ],
            source_map: Default::default(),
        }
    }
}

unsafe impl Send for DefaultRegistry {}

impl RuntimeRegistry for DefaultRegistry {
    fn set_source(
        &mut self,
        namespace: &str,
        component_name: &str,
        source: ComponentSource,
    ) -> Result<(), String> {
        let mut path = PathBuf::new();
        path.push(format!("{}", namespace));
        path.push(format!("{}", component_name));
        // todo: validate source
        self.source_map
            .insert(path.as_os_str().to_str().unwrap().to_owned(), source);
        Ok(())
    }

    fn get_source(&self, component_name: &str) -> Option<ComponentSource> {
        self.source_map.get(component_name).cloned()
    }

    fn get_languages(&mut self) -> Result<Vec<String>, String> {
        Ok(self.supported_languages.clone())
    }

    fn register(
        &mut self,
        loader: &mut ComponentLoader,
    ) -> Promise<Result<HashMap<String, Box<dyn GraphDefinition>>, String>> {
        let base_dir = &loader.base_dir;

        todo!()
    }

    fn dynamic_load(
        &mut self,
        component_name: &str,
        path: &str,
    ) -> Result<Arc<Mutex<Component>>, String> {
        if is_url(path) {
            // fetch remote component and instantiate it
        } 

        if path.contains(std::path::is_separator) {
            // fetch local component and instantiate
            if Path::new(path).is_file() {
                if let Some(ext) = Path::new(path).extension() {
                    match ext.to_str() {
                        Some("js") | Some("ts") => {
                            // build js component
                        }
                        Some("wasm") => {
                            // build wasm component
                        }
                        Some("wren") => {
                            // build wren component
                        }
                        Some("lua") => {
                            // build lua component
                        }
                        Some(&_) =>{
                            return Err(format!("Unsupported component source"))
                        }
                        None => {
                            return Err(format!("Could not detect resource type"))
                        },
                    }
                }
            }

            if Path::new(path).is_dir() {
                // todo: find and parse zflow manifest to locate component or subgraph
            }
        }

        todo!()
    }
}
