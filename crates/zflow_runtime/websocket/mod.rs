use std::{
    borrow::BorrowMut,
    collections::HashMap,
    sync::{mpsc, Arc, Mutex, RwLock},
    time::Duration,
};

use fp_rust::publisher::Publisher;
use rayon::iter::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use zflow_plugin::{ComponentSource, OutputBuffer, Package, Platform, Runtime};

use crate::{
    ip::IPType,
    process::{ProcessError, ProcessResult},
    provider::{Provider, ProviderComponent, ProviderRunner},
    runner::RunFunc,
};
use log::log;
use rust_socketio::{ClientBuilder, Payload};
use serde_json::{json, Value as JsonValue};

pub struct RemoteProvider {
    pub(crate) url: String,
    pub(crate) worker: SocketWorker,
    _components: HashMap<String, Box<dyn ProviderComponent>>,
    manifest: Option<Manifest>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Manifest {
    id: String,
    logo: String,
    packages: Vec<Package>,
    platform: Platform,
}

unsafe impl Send for Manifest {}
unsafe impl Sync for Manifest {}

impl Provider for RemoteProvider {
    fn set_workspace(&mut self, _dir: String) {}

    fn get_logo(&self) -> Result<String, anyhow::Error> {
        Ok(self.manifest.as_ref().unwrap().logo.clone())
    }

    fn get_id(&self) -> String {
        self.manifest.as_ref().unwrap().id.clone()
    }

    fn list_components(&mut self) -> Result<Vec<String>, anyhow::Error> {
        let manifest_cloned = self.manifest.clone().unwrap();
        Ok(self
            .manifest
            .as_mut()
            .unwrap()
            .packages
            .par_iter_mut()
            .map(|package| {
                package.components.par_iter_mut().map(|component| {
                    component.runtime = Runtime {
                        provider_id: manifest_cloned.id.clone(),
                        runner_id: manifest_cloned.id.clone(),
                        platform: manifest_cloned.platform.clone(),
                    };

                    component.name.clone()
                })
            })
            .flatten()
            .collect::<Vec<String>>())
    }

    fn load_component(
        &mut self,
        id: String,
    ) -> Result<
        (
            &Box<dyn crate::provider::ProviderComponent>,
            crate::provider::ProviderRunner,
        ),
        anyhow::Error,
    > {
        self.list_components()?;
        self.load_components()?;

        if let Some(component) = self._components.get(&id) {
            let _component: &ComponentSource = component
                .as_any()
                .downcast_ref::<ComponentSource>()
                .unwrap();
            let runner_func = self.rpc_process(_component.clone());
            let runner = ProviderRunner {
                runner_id: self.get_id(),
                runner_func,
                platforms: vec![Platform::System],
            };
            return Ok((component, runner));
        }

        let err = format!("Could not find component for provider {}", self.get_id());
        log!(target:format!("Provider {}", self.get_id()).as_str(),log::Level::Debug, "Could not find component with ID {}", id);
        return Err(anyhow::Error::msg(err));
    }

    fn get_process(
        &self,
        component: &Box<dyn crate::provider::ProviderComponent>,
    ) -> Result<crate::provider::ProviderRunner, anyhow::Error> {
        let _component: &ComponentSource = component
            .as_any()
            .downcast_ref::<ComponentSource>()
            .unwrap();
        let runner_func = self.rpc_process(_component.clone());
        let runner = ProviderRunner {
            runner_id: self.get_id(),
            runner_func,
            platforms: vec![Platform::System],
        };
        return Ok(runner);
    }

    fn start(&mut self) -> Result<(), anyhow::Error> {
        self.worker.start(&self.url)?;

        let manifest: Arc<RwLock<Option<Manifest>>> = Arc::new(RwLock::new(None));

        let manifest_cloned = manifest.clone();

        self.worker
            .call_ack("manifest", JsonValue::Null, move |data, _| match &data {
                Payload::Binary(b) => {
                    let data = String::from_utf8_lossy(b).to_string();
                    if let Ok(man) = serde_json::de::from_str::<Manifest>(&data) {
                        let mut _man = manifest_cloned.write().unwrap();
                        *_man = Some(man);
                    }
                }
                Payload::String(data) => {
                    if let Ok(man) = serde_json::de::from_str::<Manifest>(&data) {
                        let mut _man = manifest_cloned.write().unwrap();
                        *_man = Some(man);
                    }
                }
            })?;

        let _man = manifest
            .read()
            .map_err(|e| anyhow::Error::msg(e.to_string()))?;
        if _man.is_none() {
            log!(target:format!("Provider {}", self.url).as_str(),log::Level::Error, "No manifest found");
            return Err(anyhow::Error::msg(format!(
                "Provider {}: No manifest found",
                &self.url
            )));
        }
        self.manifest = _man.clone();

        Ok(())
    }
    fn stop(&mut self) -> Result<(), anyhow::Error> {
        self.worker.stop()
    }
}

impl RemoteProvider {
    pub fn new(url: &str) -> Self {
        let worker = SocketWorker {
            client: None,
            publisher: Arc::new(Mutex::new(Publisher::new())),
            status: SocketWorkerStatus::STOPPED,
        };

        Self {
            url: url.to_string(),
            worker,
            manifest: None,
            _components: HashMap::new(),
        }
    }

    fn load_components(&mut self) -> Result<(), anyhow::Error> {
        self._components = self
            .manifest
            .as_ref()
            .unwrap()
            .packages
            .par_iter()
            .map(|package| {
                package.components.par_iter().map(|component| {
                    let _component: Box<(dyn ProviderComponent + 'static)> =
                        Box::new(component.clone());
                    (component.name.clone(), _component)
                })
            })
            .flatten()
            .collect::<HashMap<String, Box<dyn ProviderComponent>>>();

        Ok(())
    }

    fn rpc_process(&self, component: ComponentSource) -> Box<RunFunc> {
        let process_name = component.name.to_owned();
        let rpc_client = self.worker.client.as_ref().unwrap().clone();
        let publisher = self.worker.publisher.clone();

        return Box::new(move |handle| {
            let handle_binding = handle.clone();
            let mut handle_binding = handle_binding.try_lock();
            let this = handle_binding
                .as_mut()
                .map_err(|_| ProcessError(String::from("Process Handle has dropped")))?;

            let inports = this.input().in_ports.ports;

            let inport_keys = inports.keys();
            let controlled_data = this
                .input()
                .in_ports
                .ports
                .iter()
                .filter(|(_, port)| port.options.control)
                .map(|(key, _)| this.input().get(key))
                .collect::<Vec<_>>();

            if !controlled_data.is_empty() && controlled_data.contains(&None) {
                return Ok(ProcessResult::default());
            }

            let _inputs: HashMap<String, JsonValue> = HashMap::from_iter(
                inport_keys
                    .map(|port| {
                        let value = this.input().get(port);
                        if let Some(value) = value {
                            return (
                                port.clone(),
                                match value.datatype {
                                    IPType::Data(v) => v,
                                    _ => JsonValue::Null,
                                },
                            );
                        }
                        return (port.clone(), JsonValue::Null);
                    })
                    .collect::<Vec<_>>(),
            );

            let mapped_inputs = json!(_inputs);

            let output = this.output.clone();
            let _process_name = process_name.clone();
            let (err_sender, err_reciever) = mpsc::channel::<Option<ProcessError>>();

            if let Ok(_pub) = publisher.try_lock().as_mut() {
                _pub.subscribe_fn(move |subscription| {
                    let event = &subscription.0;

                    let mut output = output.clone();

                    let id = _process_name.clone();
                    let send = format!("{}/send", &id);
                    let send_done = format!("/{}/send_done", &id);
                    let send_buffer = format!("/{}/send_buffer", &id);

                    if event.as_str() == send.as_str() {
                        if let Err(err) = output.send(&subscription.1.clone()) {
                            err_sender.send(Some(err)).unwrap();
                        } else {
                            err_sender.send(None).unwrap();
                        }
                    }
                    if event.as_str() == send_done.as_str() {
                        if let Err(err) = output.send_done(&subscription.1.clone()) {
                            err_sender.send(Some(err)).unwrap();
                        } else {
                            err_sender.send(None).unwrap();
                        }
                    }
                    if event.as_str() == send_buffer.as_str() {
                        if let Ok(out) = OutputBuffer::deserialize(subscription.1.clone()) {
                            if let Err(err) = output.send_buffer(out.port, out.packet) {
                                err_sender.send(Some(err)).unwrap();
                                return
                            }
                        }
                        err_sender.send(None).unwrap();
                    }
                });
            }

            rpc_client
                .emit(process_name.clone(), mapped_inputs)
                .map_err(|e| ProcessError(e.to_string()))?;

            if let Ok(error) = err_reciever.recv() {
                if let Some(err) = error {
                   return Err(err)
                }
            }

            Ok(ProcessResult::default())
        });
    }
}

pub enum SocketWorkerStatus {
    RUNNING,
    STOPPED,
}

pub struct SocketWorker {
    status: SocketWorkerStatus,
    client: Option<rust_socketio::client::Client>,
    pub publisher: Arc<Mutex<Publisher<(String, JsonValue)>>>,
}

impl SocketWorker {
    pub(crate) fn start(&mut self, url: &str) -> Result<(), anyhow::Error> {
        let publisher = self.publisher.clone();
        self.client = Some(
            ClientBuilder::new(url)
                .on_any(move |event, payload, _| match event {
                    rust_socketio::Event::Custom(key) => {
                        if let Ok(_pub) = publisher.clone().try_lock().as_mut() {
                            match &payload {
                                Payload::Binary(_) => unimplemented!(),
                                Payload::String(data) => {
                                    _pub.publish((key, serde_json::from_str(&data).unwrap()))
                                }
                            }
                        }
                    }
                    _ => {}
                })
                .connect()?,
        );
        self.status = SocketWorkerStatus::RUNNING;
        Ok(())
    }

    pub(crate) fn stop(&mut self) -> Result<(), anyhow::Error> {
        let client = self.client.clone();
        let (sender, receiver) = mpsc::channel::<SocketWorkerStatus>();

        self.call_ack("shutdown", JsonValue::Null, move |_, __| {
            client.as_ref().unwrap().disconnect().unwrap();
            sender.send(SocketWorkerStatus::STOPPED).unwrap();
        })?;
        if let Ok(status) = receiver.recv() {
            self.status = status;
        }
        Ok(())
    }

    pub(crate) fn call_ack(
        &self,
        process: &str,
        params: JsonValue,
        callback: impl (FnMut(Payload, rust_socketio::RawClient) -> ()) + Send + Sync + 'static,
    ) -> Result<(), anyhow::Error> {
        let client = self.client.clone();
        client
            .unwrap()
            .emit_with_ack(process, json!(params), Duration::from_secs(2), callback)?;
        Ok(())
    }
}
