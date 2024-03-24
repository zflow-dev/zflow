pub extern crate extism_pdk as extension;


use std::collections::HashMap;

use extism_pdk::*;
use json::Map;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
// #[serde(untagged)]
pub enum Platform {
    Browser,
    WebAssembly,
    #[default]
    System,
}

const fn _default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PortOptions {
    #[serde(default)]
    pub addressable: bool,
    #[serde(default)]
    pub buffered: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub caching: bool,
    #[serde(default)]
    pub data_type: IPType,
    #[serde(default)]
    pub user_data: Value,
    #[serde(default)]
    pub control: bool,
    #[serde(default = "_default_true")]
    pub triggering: bool,
    #[serde(default)]
    pub scoped: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub enum IPType {
    OpenBracket(Value),
    CloseBracket(Value),
    Data(Value),
    Buffer(Vec<u8>),
    All(Value),
    #[default]
    Unknown,
}

pub(crate) fn default_base_dir<'a>() -> String {
    "/".to_owned()
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Runtime {
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub runner_id: String,
    #[serde(default)]
    pub platform: Platform,
}

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub package_id: String,
    pub components: Vec<ComponentSource>,
    #[serde(default = "default_base_dir")]
    pub base_dir: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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
    /// Source directory
    pub source_dir: String,
    /// Path to code source
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    /// Name of the process to run (Useful for External Providers)
    pub process: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
    #[serde(default)]
    pub runtime: Runtime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputBuffer<'a> {
    pub port: &'a str,
    pub packet: &'a [u8],
}

#[host_fn]
extern "ExtismHost" {
    pub fn send<T: for<'a> Deserialize<'a> + Serialize>(output: Json<T>);
    pub fn send_done<T: for<'a> Deserialize<'a> + Serialize>(output: Json<T>);
    pub fn send_buffer(output: Json<OutputBuffer>);
}

#[repr(C)]
#[derive(Serialize, Deserialize, Debug)]
pub struct ComponentWithInput {
    pub component: ComponentSource,
    pub input: Value,
}

// impl<'a> Deserialize<'a> for ComponentWithInput<'a> {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: serde::Deserializer<'a>,
//     {
//         struct FieldVisitor;

//         let component_fields = json!(ComponentSource::default()).as_object().unwrap().keys().map(|k| k.as_str()).collect::<Vec<&str>>();

//         impl<'a> Visitor<'a> for FieldVisitor {
//             type Value = ComponentSource<'a>;

//             fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//                 formatter.write_str("Component source fields")
//             }

//             fn visit_map<E>(self, mut value: E) -> Result<ComponentSource<'a>, E::Error>
//             where
//                 E: MapAccess<'a>,
//             {
//                 let mut map = serde_json::Map::new();
//                 while let Some(key) = value.next_key()? {
//                    map.insert(key, value.next_value().unwrap());
//                 }
//                 let component = ComponentSource::<'a>::deserialize(json!(map)).unwrap();
//                 // let component_fields = json!(ComponentSource::default()).as_object().unwrap().keys().map(|k| k.as_str()).collect::<Vec<&str>>();
//                 // match value {
//                 //     "secs" => Ok(Field::Secs),
//                 //     "nanos" => Ok(Field::Nanos),
//                 //     _ => Err(de::Error::unknown_field(value, component_fields.as_slice().into())),
//                 // }
//                 Ok(component)
//             }
//         }
//         deserializer.deserialize_struct("component", &component_fields, FieldVisitor)?;
//         deserializer.deserialize_identifier()
        
//     }
// }
