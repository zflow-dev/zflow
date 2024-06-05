use serde::{Deserialize, Serialize};

// #[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
// #[serde(untagged)]
// pub(crate) enum ExternProviderType {
//     #[default]
//     Wasi,
//     Deno,
// }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(untagged)]
pub enum ExternProviderSource {
    Data {
        data: Vec<u8>,
    },
    Url {
        url: String,
    },
    Local {
        path: String,
    },
    #[default]
    Undefined,
}

pub(crate) fn _default_base_dir() -> String {
    "/".to_string()
}

mod test {
    use std::{collections::HashMap, env::current_dir, sync::Once};

    use serde_json::json;
    use simple_logger::SimpleLogger;
    use zflow_graph::Graph;

    use crate::{
        component::{Component, ComponentOptions}, deno::DenoExternProvider, extern_provider::ExternProviderSource, network::{BaseNetwork, Network, NetworkOptions}, port::InPort, process::ProcessResult, wasm::WasmExternProvider
    };
    use log::{log, Level};

    static INIT: Once = Once::new();

    fn init() {
        INIT.call_once(|| {
            SimpleLogger::default()
                .with_colors(true)
                .without_timestamps()
                .with_level(log::LevelFilter::Off)
                .with_module_level("zflow_runtime::extern_provider", log::LevelFilter::Debug)
                .init()
                .unwrap();
        });
        // deno_core::JsRuntime::init_platform(Some(v8::Platform::new(1, false).into()));
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
        dir.push("../test_components");
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
    fn test_wasm_provider() -> Result<(), anyhow::Error> {
        init();
        let mut dir = current_dir()?;

        dir.push("../../test_providers");
        let dir = dir.to_str().unwrap();

        let mut n = get_network();

        n.register_component("debug", "logger", get_logger())?;

        let provider = WasmExternProvider::new(
            dir,
            ExternProviderSource::Local {
                path: "maths_provider/bin/maths_provider.wasm".to_owned(),
            },
        )?;

        n.register_provider(provider);

        let n = n.connect();
        assert!(n.is_ok());
        if let Ok(n) = n {
            assert!(n.start().is_ok());
            assert!(n.stop().is_ok());
        }

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_deno_provider() -> Result<(), anyhow::Error> {
        init();

        let mut n = get_network();

        n.register_component("debug", "logger", get_logger())?;

        let mut dir = current_dir()?;
        dir.push("../../test_providers");
        let dir = dir.to_str().unwrap();

        let provider = DenoExternProvider::new(
            dir,
            ExternProviderSource::Local {
                path: "maths_provider/math.ts".to_owned(),
            },
        )
        .await?;

        n.register_provider(provider);

        let n = n.connect();
        assert!(n.is_ok());
        if let Ok(n) = n {
            assert!(n.start().is_ok());
            assert!(n.stop().is_ok());
        }
        Ok(())
    }
}
