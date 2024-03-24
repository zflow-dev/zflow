use std::{
    any::Any,
    collections::HashMap,
    fs::{self, DirEntry, File},
    io::{self, BufReader},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
};

use libloading::Library;
use poll_promise::Promise;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{
    component::{Component, GraphDefinition, ModuleComponent},
    loader::{build_node_id, normalize_name, ComponentLoader},
    port::PortOptions,
};

#[cfg(feature = "js_runtime")]
use crate::js::JsComponent;

#[cfg(feature = "wasm_runtime")]
use crate::wasm::WasmComponent;

#[cfg(feature = "lua_runtime")]
use crate::lua::LuaComponent;

#[cfg(feature = "wren_runtime")]
use crate::wren::WrenComponent;

#[cfg(feature = "go_runtime")]
use crate::go::GoComponent;

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

fn default_base_dir() -> String {
    "/".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageRuntime {
    #[serde(default = "default_base_dir")]
    /// Base directory for code runners
    pub base_dir: String,
    /// Map of code file extensions mapped to their runners
    pub runners: Vec<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSource {
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
    /// Base directory of source packages
    pub base_dir: String,
    /// Path to code source
    pub source: String,
    /// Source language
    pub language: String,
    #[serde(default)]
    pub package_id: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
    // pub runtime: LanguageRuntime,
}

impl GraphDefinition for ComponentSource {
    fn to_any(&self) -> &dyn Any {
        Box::leak(Box::new(self.clone())) as &dyn Any
    }
}
/// Registry is a way to tell the Component Loader where to discover, load and execute custom components
pub trait RuntimeRegistry {
    fn set_base_dir(&mut self, dir: &str);
    fn get_base_dir(&self) -> &str;
    fn set_source(
        &mut self,
        namespace: &str,
        component_name: &str,
        source: ComponentSource,
    ) -> Result<(), String>;
    fn get_source(&self, component_name: &str) -> Option<ComponentSource>;
    fn get_languages(&mut self) -> Result<Vec<String>, String>;
    
    fn register_runner(&mut self, language:&str, runner: Library);

    fn get_runner(self, language:&str) -> Option<Library>;

    /// Register custom component loaders
    fn register(
        &mut self,
        loader: &mut ComponentLoader,
    ) -> Promise<Result<HashMap<String, Box<dyn GraphDefinition>>, String>>;

    /// Dynamically load a component from sourc or path
    fn dynamic_load(
        &mut self,
        component_name: &str,
        source: ComponentSource,
        options: Value,
    ) -> Result<Arc<Mutex<Component>>, String>;

    /// Factory function to creates an instance of a component that can be executed by this runtime.
    fn create_component(
        &mut self,
        name: &str,
        component: &dyn GraphDefinition,
        metadata: Value,
    ) -> Result<Arc<Mutex<Component>>, String>;
}

#[derive(Clone)]
pub struct DefaultRegistry {
    base_dir: PathBuf,
    supported_languages: Vec<String>,
    source_map: HashMap<String, ComponentSource>,
}

impl Default for DefaultRegistry {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::default(),
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
    fn set_base_dir(&mut self, dir: &str) {
        self.base_dir = PathBuf::from(dir);
    }
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
            let components = visit_dirs(&base_dir, &|entry| {
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
                        let package_id =
                            if _metadata.as_object().unwrap().contains_key("package_name") {
                                _metadata
                                    .get("package_name")
                                    .unwrap()
                                    .as_str()
                                    .expect("expected name to be string")
                            } else if _metadata.as_object().unwrap().contains_key("name") {
                                _metadata
                                    .get("name")
                                    .unwrap()
                                    .as_str()
                                    .expect("expected name to be string")
                            } else {
                                ""
                            };

                        let mut _metadata = metadata.clone();

                        let mut runners = None;

                        if let Some(runtime) = _metadata
                            .get("components")
                            .expect("Invalid metadata, config should have components field")
                            .as_object()
                        {
                            
                            let runtime_dir = runtime.get("base_dir").or(Some(&json!(dir.as_str()))).unwrap().as_str().unwrap();
                            runners = runtime.get("runners").or(Some(&json!({}))).unwrap().as_object();
                        }

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
                                            build_node_id(&wasm_component.package_id, &wasm_component.name),
                                            definition,
                                        ));
                                    }
                                    #[cfg(feature = "js_runtime")]
                                    Some("js") | Some("javascript") | Some("ts") => {
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
                                            build_node_id(&js_component.package_id, &js_component.name),
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
                                            build_node_id(&lua_component.package_id, &lua_component.name),
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
                                            build_node_id(&wren_component.package_id, &wren_component.name),
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
                                            build_node_id(&go_component.package_id, &go_component.name),
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

                        let mut components: HashMap<String, Box<dyn GraphDefinition>> =
                            HashMap::from_iter(components);
                        source_map.iter().for_each(|(k, v)| {
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
        name: &str,
        source: ComponentSource,
        metadata: Value,
    ) -> Result<Arc<Mutex<Component>>, String> {
        match source.language.as_str() {
            #[cfg(feature = "lua_runtime")]
            "lua" => {
                let comp =
                    LuaComponent::deserialize(json!(source)).map_err(|err| err.to_string())?;
                return self.create_component(name, &comp, metadata);
            }
            #[cfg(feature = "wren_runtime")]
            "wren" => {
                let comp =
                    WrenComponent::deserialize(json!(source)).map_err(|err| err.to_string())?;
                return self.create_component(name, &comp, metadata);
            }
            #[cfg(feature = "js_runtime")]
            "js" | "javascript" | "ts" => {
                return self.create_component(
                    name,
                    &JsComponent::deserialize(json!(source)).map_err(|err| err.to_string())?,
                    metadata,
                )
            }
            #[cfg(feature = "go_runtime")]
            "go" | "gos" => {
                return self.create_component(
                    name,
                    &GoComponent::deserialize(json!(source)).map_err(|err| err.to_string())?,
                    metadata,
                )
            }
            _ => return Err(format!("Unsupported source language: {}", source.language)),
        }
    }

    fn create_component(
        &mut self,
        name: &str,
        component: &dyn GraphDefinition,
        metadata: Value,
    ) -> Result<Arc<Mutex<Component>>, String> {
        // If a string was specified, attempt to load it dynamically
        if let Some(source) = component.to_any().downcast_ref::<ComponentSource>() {
            return self.dynamic_load(name, source.clone(), metadata);
        }

        #[cfg(feature = "wasm_runtime")]
        // Load and create a wasm component
        if let Some(wasm_component) = component.to_any().downcast_ref::<WasmComponent>() {
            let mut wasm = wasm_component.clone();
            wasm.base_dir = if wasm.base_dir == "/" {
                let mut buf = PathBuf::from(self.base_dir.clone());
                buf.push(wasm.base_dir);
                buf.to_str().unwrap().to_owned()
            } else {
                self.base_dir
                    .clone()
                    .as_os_str()
                    .to_str()
                    .expect("expected valid path string")
                    .to_owned()
            };
            return Ok(Component::from_instance(
                wasm_component
                    .clone()
                    .with_metadata(metadata)
                    .as_component()?,
            ));
        }

        #[cfg(feature = "js_runtime")]
        // Load and create a js component
        if let Some(js_component) = component.to_any().downcast_ref::<JsComponent>() {
            let mut js = js_component.clone();
            js.base_dir = if js.base_dir == "/" {
                let mut buf = self.base_dir.clone();
                buf.push(js.base_dir);
                buf.to_str().unwrap().to_owned()
            } else {
                self.base_dir
                    .clone()
                    .as_os_str()
                    .to_str()
                    .expect("expected valid path string")
                    .to_owned()
            };
            return Ok(Component::from_instance(
                js_component
                    .clone()
                    .with_metadata(metadata)
                    .as_component()?,
            ));
        }

        #[cfg(feature = "lua_runtime")]
        // Load and create a lua component
        if let Some(lua_component) = component.to_any().downcast_ref::<LuaComponent>() {
            let mut lua = lua_component.clone();
            lua.base_dir = if lua.base_dir == "/" {
                let mut buf = self.base_dir.clone();
                buf.push(lua.base_dir);
                buf.to_str().unwrap().to_owned()
            } else {
                self.base_dir
                    .clone()
                    .as_os_str()
                    .to_str()
                    .expect("expected valid path string")
                    .to_owned()
            };
            return Ok(Component::from_instance(
                lua_component
                    .clone()
                    .with_metadata(metadata)
                    .as_component()?,
            ));
        }

        #[cfg(feature = "wren_runtime")]
        // Load and create a wren component
        if let Some(wren_component) = component.to_any().downcast_ref::<WrenComponent>() {
            let mut wren = wren_component.clone();
            wren.base_dir = if wren.base_dir == "/" {
                let mut buf = self.base_dir.clone();
                buf.push(wren.base_dir);
                buf.to_str().unwrap().to_owned()
            } else {
                self.base_dir
                    .clone()
                    .as_os_str()
                    .to_str()
                    .expect("expected valid path string")
                    .to_owned()
            };
            return Ok(Component::from_instance(
                wren_component
                    .clone()
                    .with_metadata(metadata)
                    .as_component()?,
            ));
        }

        #[cfg(feature = "go_runtime")]
        // Load and create a go component
        if let Some(go_component) = component.to_any().downcast_ref::<GoComponent>() {
            let mut go = go_component.clone();
            go.base_dir = if go.base_dir == "/" {
                let mut buf = self.base_dir.clone();
                buf.push(go.base_dir);
                buf.to_str().unwrap().to_owned()
            } else {
                self.base_dir
                    .clone()
                    .as_os_str()
                    .to_str()
                    .expect("expected valid path string")
                    .to_owned()
            };
            return Ok(Component::from_instance(
                go_component
                    .clone()
                    .with_metadata(metadata)
                    .as_component()?,
            ));
        }

        // check if it's a component instance
        if let Some(instance) = component.to_any().downcast_ref::<Component>() {
            let mut instance = instance.clone();
            if let Some(meta) = metadata.as_object() {
                instance.metadata = Some(meta.clone());
            }
            return Ok(Component::from_instance(instance));
        }

        Err("".to_owned())
    }

    fn get_base_dir(&self) -> &str {
        self.base_dir.to_str().or(Some("/")).unwrap()
    }
}
