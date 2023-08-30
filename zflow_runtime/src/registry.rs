use std::{
    collections::HashMap,
    fs::{self, DirEntry, File},
    io::{self, BufReader},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex}, any::Any,
};

use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{
    component::{Component, GraphDefinition, ModuleComponent},
    loader::{normalize_name, ComponentLoader},
    port::PortOptions
};

#[cfg(feature="js_runtime")]
use crate::js::JsComponent;

#[cfg(feature = "wasm_runtime")]
use crate::wasm::WasmComponent;

#[cfg(feature="lua_runtime")]
use crate::lua::LuaComponent;

#[cfg(feature="wren_runtime")]
use crate::wren::WrenComponent;

#[cfg(feature="go_runtime")]
use crate::go::GoComponent;

use is_url::is_url;

#[derive(Serialize, Deserialize, Clone, Debug)]
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
    pub package_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSource {
    pub name: String,
    pub inports: HashMap<String, PortOptions>,
    pub outports: HashMap<String, PortOptions>,
    pub source: String,
    pub language: String,
}

impl GraphDefinition for ComponentSource {
    fn to_any(&self) -> &dyn Any {
        Box::leak(Box::new(self.clone())) as &dyn Any
    }
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
        options: Value,
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

fn visit_dirs(
    dir: &Path,
    cb: &dyn Fn(&DirEntry) -> Option<HashMap<String, Box<dyn GraphDefinition>>>,
) -> io::Result<HashMap<String, Box<dyn GraphDefinition>>> {
    let mut components = HashMap::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let comps = visit_dirs(&path, cb)?;
                components.extend(comps);
            } else {
                if let Some(comp) = cb(&entry) {
                    components.extend(comp);
                }
            }
        }
    }
    Ok(components)
}

impl RuntimeRegistry for DefaultRegistry {
    fn set_source(
        &mut self,
        namespace: &str,
        component_name: &str,
        source: ComponentSource,
    ) -> Result<(), String> {
        // todo: validate source
        self.source_map
            .insert(normalize_name(namespace, component_name), source);
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
        let dir = loader.base_dir.clone();

        let source_map = self.source_map.clone();

        Promise::spawn_thread("register_components", move || {
            let base_dir = PathBuf::from_str(dir.as_str()).unwrap();
            // Recursively look up all component directories
            let mut components = visit_dirs(&base_dir, &|entry| {
                if entry.path().is_file()
                    && (entry.path().file_name().unwrap() == "zflow.json"
                        || entry.path().file_name().unwrap() == "package.json")
                {
                    let file =
                        File::open(entry.path()).expect("expected to open zflow.json manifest");
                    let reader = BufReader::new(file);

                    let mut de = serde_json::Deserializer::from_reader(reader);
                    if let Ok(metadata) = Value::deserialize(&mut de).as_mut() {
                        let _metadata = metadata.clone();
                        let package_id = if _metadata.as_object().unwrap().contains_key("package_name") {
                            _metadata
                            .get("package_name").unwrap().as_str().expect("expected name to be string")
                        } else if _metadata.as_object().unwrap().contains_key("name") {
                            _metadata
                            .get("name").unwrap().as_str().expect("expected name to be string")
                        } else {
                            ""
                        };

                        let mut _metadata = metadata.clone();
                        let components = _metadata
                            .get_mut("components")
                            .expect("Invalid metadata, zflow.js should have components field")
                            .as_array_mut()
                            .expect("Components should be array");

                        let components = components.iter_mut().map(|meta| {
                            let meta_str = serde_json::to_string(meta).unwrap();
                            let mut de = serde_json::Deserializer::from_str(meta_str.as_str());
                            let component_meta = Value::deserialize(&mut de).expect(
                                "expected to decode component metadata from zflow.json",
                            );
                            if let Some(metadata) = meta.as_object_mut() {
                                let copy_meta = metadata.clone();
                                let language = copy_meta.get("language").expect("component metadata must specify a language");
                                metadata.remove("language");
                                match language.as_str() {
                                    #[cfg(feature = "wasm_runtime")]
                                    Some("wasm") => {
                                        // Read wasm
                                        let mut wasm_component = WasmComponent::deserialize(component_meta)
                                        .expect(
                                            "expected to decode component metadata from zflow.json or package.json",
                                        );
                                        wasm_component.base_dir = entry
                                            .path()
                                            .parent()
                                            .unwrap()
                                            .as_os_str()
                                            .to_str()
                                            .unwrap()
                                            .to_owned();
                                        wasm_component.package_id = package_id.to_owned();
                                        let definition: Box<dyn GraphDefinition> =
                                            Box::new(wasm_component.clone());
                                        return Some((
                                            normalize_name(&wasm_component.package_id, &wasm_component.name),
                                            definition,
                                        ));
                                    }
                                    #[cfg(feature = "js_runtime")]
                                    Some("js") | Some("ts") => {
                                        // Read js/ts
                                        let mut js_component = JsComponent::deserialize(component_meta)
                                        .expect(
                                            "expected to decode component metadata from zflow.json or package.json",
                                        );

                                        js_component.base_dir = entry
                                            .path()
                                            .parent()
                                            .unwrap()
                                            .as_os_str()
                                            .to_str()
                                            .unwrap()
                                            .to_owned();
                                        js_component.package_id = package_id.to_owned();

                                        let definition: Box<dyn GraphDefinition> =
                                            Box::new(js_component.clone());
                                        return Some((
                                            normalize_name(&js_component.package_id, &js_component.name),
                                            definition,
                                        ));
                                    },
                                    #[cfg(feature = "lua_runtime")]
                                    Some("lua") => {
                                        // Read lua
                                        let mut lua_component = LuaComponent::deserialize(component_meta)
                                        .expect(
                                            "expected to decode component metadata from zflow.json or package.json",
                                        );

                                        lua_component.base_dir = entry
                                            .path()
                                            .parent()
                                            .unwrap()
                                            .as_os_str()
                                            .to_str()
                                            .unwrap()
                                            .to_owned();
                                        lua_component.package_id = package_id.to_owned();

                                        let definition: Box<dyn GraphDefinition> =
                                            Box::new(lua_component.clone());
                                        return Some((
                                            normalize_name(&lua_component.package_id, &lua_component.name),
                                            definition,
                                        ));
                                    }
                                    #[cfg(feature = "wren_runtime")]
                                    Some("wren") => {
                                        // Read lua
                                        let mut wren_component = WrenComponent::deserialize(component_meta)
                                        .expect(
                                            "expected to decode component metadata from zflow.json or package.json",
                                        );

                                        wren_component.base_dir = entry
                                            .path()
                                            .parent()
                                            .unwrap()
                                            .as_os_str()
                                            .to_str()
                                            .unwrap()
                                            .to_owned();
                                        wren_component.package_id = package_id.to_owned();

                                        let definition: Box<dyn GraphDefinition> =
                                            Box::new(wren_component.clone());
                                        return Some((
                                            normalize_name(&wren_component.package_id, &wren_component.name),
                                            definition,
                                        ));
                                    }
                                    #[cfg(feature = "go_runtime")]
                                    Some("go") |  Some("gos") => {
                                        // Read go
                                        let mut go_component = GoComponent::deserialize(component_meta)
                                        .expect(
                                            "expected to decode component metadata from zflow.json or package.json",
                                        );

                                        go_component.base_dir = entry
                                            .path()
                                            .parent()
                                            .unwrap()
                                            .as_os_str()
                                            .to_str()
                                            .unwrap()
                                            .to_owned();
                                        go_component.package_id = package_id.to_owned();

                                        let definition: Box<dyn GraphDefinition> =
                                            Box::new(go_component.clone());
                                        return Some((
                                            normalize_name(&go_component.package_id, &go_component.name),
                                            definition,
                                        ));
                                    }
                                    _=>{
                                       return None
                                    }
                                }
                            }
                            None
                        }).filter(|component| component.is_some()).map(|component| component.unwrap());

                        let mut components:HashMap<String, Box<dyn GraphDefinition>> = HashMap::from_iter(components);
                        source_map.iter().for_each(|(k, v)|{
                            components.insert(k.clone(), Box::new(v.clone()));
                            
                        });
                        return Some(components);
                    }
                }
                None
            });
            
            if components.is_err() {
                return Err(format!("{}", components.err().unwrap().to_string()));
            }
           
            Ok(components.ok().unwrap())
        })
    }

    fn dynamic_load(
        &mut self,
        component_name: &str,
        path: &str,
        _options: Value,
    ) -> Result<Arc<Mutex<Component>>, String> {
        let options: Option<&Map<String, Value>> = _options.as_object();
        if is_url(path) {
            // fetch remote component and instantiate it
        }

        if is_url(path) && !Path::new(path).exists() {
            return Err(format!("Could not find component at {}", path));
        }
        if path.contains(std::path::is_separator) {
            // fetch local component and instantiate
            if Path::new(path).is_file() {
                if let Some(ext) = Path::new(path).extension() {
                    match ext.to_str() {
                        #[cfg(feature = "js_runtime")]
                        Some("js") | Some("ts") => {
                            // build js component
                        }
                        #[cfg(feature = "wasm_runtime")]
                        Some("wasm") => {
                            // build wasm component
                            if let Some(wasm) = WasmComponent::from_metadata(_options).as_mut() {
                                let path = Path::new(path);
                                wasm.base_dir = path.parent().unwrap().to_str().unwrap().to_owned();
                                wasm.source =
                                    path.file_name().unwrap().to_str().unwrap().to_owned();
                                return Ok(Component::from_instance(wasm.as_component()?));
                            }
                        }
                        #[cfg(feature = "wren_runtime")]
                        Some("wren") => {
                            // build wren component
                        }
                        #[cfg(feature = "lua_runtime")]
                        Some("lua") => {
                            // build lua component
                        }
                        #[cfg(feature = "go_runtime")]
                        Some("go")|Some("gos") => {
                            // build lua component
                        }
                        Some(&_) => return Err(format!("Unsupported component source")),
                        None => return Err(format!("Could not detect resource type")),
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
