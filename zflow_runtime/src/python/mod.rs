use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::HashMap, path::PathBuf};
use std::sync::{Arc, Mutex};

use crate::{
    component::{Component, ModuleComponent},
    port::PortOptions,
};
use crate::component::ComponentOptions;
use crate::port::{InPort, OutPort};
use crate::process::{ProcessHandle, ProcessResult};

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct PythonComponent {
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

impl PythonComponent {
    pub fn from_metadata(meta: Value) -> Option<PythonComponent> {
        PythonComponent::deserialize(meta).ok()
    }

    pub fn with_metadata(&mut self, meta: Value) -> PythonComponent {
        if let Some(meta) = PythonComponent::from_metadata(meta.clone()) {
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

impl ModuleComponent for PythonComponent {
    fn as_component(&self) -> Result<Component, String> {
        let source = self.source.clone();
        let mut code = PathBuf::from(&self.base_dir.clone());
        code.push(source);
        let code = code.as_os_str();

            let mut inport = self.inports.clone();
            let mut outport = self.outports.clone();

            if inport.is_empty() {
                inport.insert("in".to_string(), PortOptions::default());
            }
            if outport.is_empty() {
                outport.insert("out".to_string(), PortOptions::default());
            }

            return Ok(Component::new(ComponentOptions{
                metadata: Some(self.metadata.clone()),
                in_ports: HashMap::from_iter(inport.clone().iter().map(|(key, options) | (
                    key.clone(),InPort::new(options.clone())))),
                out_ports: HashMap::from_iter(outport.clone().iter().map(|(key, options) | (
                    key.clone(),OutPort::new(options.clone())))),

                icon: self.icon.clone(),
                description: self.description.clone(),
                ordered: self.ordered,
                activate_on_input: self.activate_on_input,
                forward_brackets: self.forward_brackets.clone(),
                graph: None,
                process: Some(Box::new(move |handle: Arc<Mutex<ProcessHandle>>| {
                    if let Ok(inst) = handle.clone().try_lock().as_mut() {
                        // do nothing for now, return ok
                    }
                    Ok(ProcessResult::default())
                }))
            }))
    }
}
