
use std::{collections::HashMap, fs, path::Path};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::state::NodeId;

#[derive(Clone, Serialize, Deserialize)]
pub enum IconLocation<'a> {
    Remote { url: &'a str },
    File { path: &'a Path },
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct IconLibrary {
    pub icons: HashMap<NodeId, Box<[u8]>>,
}

impl IconLibrary {
    pub fn new() -> Self {
        Self {
            icons: HashMap::new(),
        }
    }
    pub fn extend(&mut self, library: IconLibrary) {
        self.icons.extend(library.icons);
    }

    pub fn add(&mut self, node_id: NodeId, location: IconLocation) -> Result<(), String> {
        match location {
            IconLocation::File { path } => {
                if !path.is_file() {
                    return Err(format!(
                        "Could not find icon image in path {:?}",
                        path.as_os_str().to_str().unwrap_or("")
                    ));
                }
                // let debug_name = Path::file_name(path).unwrap().to_str().unwrap_or("");
                let data = fs::read(path).expect(
                    format!(
                        "Unable to icon image from path: {}",
                        path.as_os_str().to_str().unwrap_or("")
                    )
                    .as_str(),
                );
                // let source = path.as_os_str().to_str().unwrap_or("");
                self.icons.insert(node_id, data.into_boxed_slice());
            }
            IconLocation::Remote { url } => {
                let source_url = Url::parse(url).expect("invalid url");
                let client = reqwest::Client::new();
                if let Ok(runtime) = tokio::runtime::Runtime::new() {
                    let res = runtime.block_on(async {
                        client
                            .get(source_url.as_str())
                            .send()
                            .await
                            .expect("Failed to fetch remote resource")
                            .bytes()
                            .await
                    });
                    if let Ok(res) = res {
                        let data = res.to_vec().into_boxed_slice();
                        self.icons.insert(node_id, data);
                        return Ok(());
                    }
                }

                return Err(format!(
                    "Could not fetch icon image from url {}",
                    source_url
                ));
            }
        };

        Ok(())
    }

    pub fn remove(&mut self, node_id: &NodeId) -> Option<Box<[u8]>> {
        self.icons.remove(node_id)
    }

    pub fn get(&mut self, node_id: &NodeId) -> Option<&Box<[u8]>> {
        self.icons.get(node_id)
    }
}
