use std::rc::Rc;

use _ops::RustToV8;
use deno_ast::MediaType;
use deno_ast::ParseParams;
use deno_ast::SourceTextInfo;
use deno_core::futures::FutureExt;
use deno_core::*;
use futures::TryFutureExt;
use v8::Handle;
use v8::HandleScope;
use v8::MapFnTo;
use zflow_plugin::ComponentSource;
use zflow_plugin::Platform;

use crate::deno;
use crate::ip::IPType;
use crate::process::ProcessError;
use crate::process::ProcessResult;
use crate::provider::ProviderRunner;
use crate::runner::RunFunc;
use crate::runner::DENO_RUNNER_ID;

pub struct DenoModuleLoader;

impl deno_core::ModuleLoader for DenoModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: deno_core::ResolutionKind,
    ) -> Result<deno_core::ModuleSpecifier, anyhow::Error> {
        deno_core::resolve_import(specifier, referrer).map_err(|e| e.into())
    }

    fn load(
        &self,
        module_specifier: &deno_core::ModuleSpecifier,
        _maybe_referrer: Option<&deno_core::ModuleSpecifier>,
        _is_dyn_import: bool,
        _requested_module_type: deno_core::RequestedModuleType,
    ) -> std::pin::Pin<Box<deno_core::ModuleSourceFuture>> {
        let module_specifier = module_specifier.clone();
        async move {
            let path = module_specifier.to_file_path().unwrap();
            // Determine what the MediaType is (this is done based on the file
            // extension) and whether transpiling is required.
            let media_type = MediaType::from_path(&path);

            let (module_type, should_transpile) = match MediaType::from_path(&path) {
                MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                    (deno_core::ModuleType::JavaScript, false)
                }
                MediaType::Jsx => (deno_core::ModuleType::JavaScript, true),
                MediaType::TypeScript
                | MediaType::Mts
                | MediaType::Cts
                | MediaType::Dts
                | MediaType::Dmts
                | MediaType::Dcts
                | MediaType::Tsx => (deno_core::ModuleType::JavaScript, true),
                MediaType::Json => (deno_core::ModuleType::Json, false),
                _ => panic!("Unknown extension {:?}", path.extension()),
            };

            // Transpile if Typescript
            let code = std::fs::read_to_string(&path)?;
            let code = if should_transpile {
                let parsed = deno_ast::parse_module(ParseParams {
                    specifier: module_specifier.to_string(),
                    text_info: SourceTextInfo::from_string(code),
                    media_type,
                    capture_tokens: false,
                    scope_analysis: false,
                    maybe_syntax: None,
                })?;
                parsed.transpile(&Default::default())?.text
            } else {
                code
            };

            // Load and return module.
            let module = deno_core::ModuleSource::new(
                module_type,
                deno_core::ModuleSourceCode::Bytes(ModuleCodeBytes::Boxed(
                    code.into_bytes().into_boxed_slice(),
                )),
                &module_specifier,
            );

            Ok(module)
        }
        .boxed_local()
    }
}

struct Callback {
    pub callback:
        Box<dyn FnMut(&mut HandleScope<'_>, Vec<v8::Local<v8::Value>>) -> Result<(), ProcessError>>,
}

extern "C" fn v8_callback(info: *const v8::FunctionCallbackInfo) {
    let info = unsafe { &*info };
    let args = v8::FunctionCallbackArguments::from_function_callback_info(info);
    let rv = v8::ReturnValue::from_function_callback_info(info);
    let scope = unsafe { &mut v8::CallbackScope::new(info) };

    v8_func(scope, args, rv);
}

fn v8_func(
    scope: &mut v8::HandleScope,
    fca: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let undef_value: v8::Local<v8::Value> = v8::undefined(scope).into();

    let data = fca.data();
    let ext = v8::Local::<v8::External>::try_from(data).unwrap();
    let callback_ptr = ext.value() as *mut Callback;
    let callback_wrapper = unsafe { &mut *callback_ptr };
    let mut vals = vec![];
    for i in 0..fca.length() {
        let val = fca.get(i);
        vals.push(val);
    }
    let res = (callback_wrapper.callback)(scope, vals);
    if let Ok(_) = res {
        rv.set(undef_value.clone());
        return;
    }
    rv.set(
        v8::String::new(scope, &res.err().unwrap().0)
            .unwrap()
            .into(),
    );
}

pub fn get_sys_deno_runner(
    component: ComponentSource,
    process_name: &str,
    module: ModuleSpecifier,
    _is_provider: bool,
) -> Result<ProviderRunner, anyhow::Error> {
    let id = component.name.to_owned();
    let process_name = process_name.to_owned();

    let runner_func: Box<RunFunc> = Box::new(move |handle| {
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

        let module = module.clone();
        let id = id.clone();
        let process_name = process_name.clone();

        let mut runtime = deno_core::JsRuntime::new(deno_core::RuntimeOptions {
            module_loader: Some(Rc::new(deno::DenoModuleLoader)),
            ..Default::default()
        });

        futures_lite::future::block_on(async move {
            let mod_id = runtime
                .load_main_module(&module, None)
                .map_err(|e| ProcessError(e.to_string()))
                .await?;
            let runner = runtime.mod_evaluate(mod_id);
            runtime
                .run_event_loop(PollEventLoopOptions {
                    wait_for_inspector: false,
                    pump_v8_message_loop: false,
                })
                .map_err(|e| ProcessError(e.to_string()))
                .await?;

            let global = runtime
                .get_module_namespace(mod_id)
                .map_err(|e| ProcessError(e.to_string()))?;
            let scope = &mut runtime.handle_scope();
            let module_obj = global.open(scope);

            let undef_value: v8::Local<v8::Value> = v8::undefined(scope).into();

            let zflow_obj = v8::Object::new(scope);

            let _output = this.output();
            let send_cb = Box::into_raw(Box::new(Callback {
                callback: Box::new(move |scope, data| {
                    let data = data[0];
                    if data.is_undefined() {
                        return _output.clone().send(&serde_json::json!(null));
                    }
                    let val = v8::json::stringify(scope, data.into())
                        .unwrap()
                        .to_rust_string_lossy(scope);
                    let val: serde_json::Value = serde_json::from_str(&val).unwrap();
                    _output.clone().send(&val)
                }),
            }));
            let send_ext = v8::External::new(scope, send_cb as _);

            let _output = this.output();
            let send_done_cb = Box::into_raw(Box::new(Callback {
                callback: Box::new(move |scope, data| {
                    let data = data[0];
                    if data.is_undefined() {
                        return _output.clone().send(&serde_json::json!(null));
                    }
                    let val = v8::json::stringify(scope, data.into())
                        .unwrap()
                        .to_rust_string_lossy(scope);
                    let val: serde_json::Value = serde_json::from_str(&val).unwrap();
                    _output.clone().send_done(&val)
                }),
            }));
            let send_done_ext = v8::External::new(scope, send_done_cb as _);

            let _output = this.output();
            let send_buf_cb = Box::into_raw(Box::new(Callback {
                callback: Box::new(move |scope, data| {
                    let port = data[0];
                    if port.is_undefined() || data[1].is_undefined() {
                        return Err(ProcessError(format!("Port or Data can not be undefined")));
                    }
                    if !port.is_string() {
                        return Err(ProcessError(format!("Port name should be type of string")));
                    }
                    let buf = v8::Local::<v8::Array>::try_from(data[1])
                        .map_err(|err| ProcessError(format!("{:?}", err)))?;
                    if !buf.is_uint8_array() {
                        return Err(ProcessError(format!("Data must by type of Uint8Array")));
                    }
                    let mut data = vec![];
                    for i in 0..buf.length() {
                        let index = i.to_v8(scope);
                        let char = buf.get(scope, index).unwrap();
                        let char = char.open(scope).uint32_value(scope).unwrap() as u8;
                        data.push(char);
                    }

                    _output
                        .clone()
                        .send_buffer(&port.to_rust_string_lossy(scope), &data)
                }),
            }));
            let send_buf_ext = v8::External::new(scope, send_buf_cb as _);

            let send_fn = v8::Function::builder_raw(v8_callback)
                .data(send_ext.into())
                .build(scope)
                .unwrap();
            let send_fn_key = v8::Local::<v8::Value>::from(v8::String::new(scope, "send").unwrap());
            let send_fn =
                v8::Local::<v8::Value>::from(v8::Local::<v8::Function>::new(scope, send_fn));
            zflow_obj.set(scope, send_fn_key, send_fn).unwrap();

            let send_done_fn = v8::Function::builder_raw(v8_callback)
                .data(send_done_ext.into())
                .build(scope)
                .unwrap();
            let send_done_fn_key =
                v8::Local::<v8::Value>::from(v8::String::new(scope, "send_done").unwrap());
            let send_done_fn =
                v8::Local::<v8::Value>::from(v8::Local::<v8::Function>::new(scope, send_done_fn));
            zflow_obj
                .set(scope, send_done_fn_key, send_done_fn)
                .unwrap();

            let send_buf_fn = v8::Function::builder_raw(v8_callback)
                .data(send_buf_ext.into())
                .build(scope)
                .unwrap();
            let send_buf_fn_key =
                v8::Local::<v8::Value>::from(v8::String::new(scope, "send_buffer").unwrap());
            let send_buf_fn =
                v8::Local::<v8::Value>::from(v8::Local::<v8::Function>::new(scope, send_buf_fn));
            zflow_obj.set(scope, send_buf_fn_key, send_buf_fn).unwrap();

            let zflow_obj_key =
                v8::Local::<v8::Value>::from(v8::String::new(scope, "zflow").unwrap());
            module_obj.set(scope, zflow_obj_key, zflow_obj.into());

            let mut mapped_inputs = v8::Map::new(scope);

            inport_keys.for_each(|port| {
                let value = this.input().get(port);
                let port = v8::Local::<v8::Value>::from(v8::String::new(scope, port).unwrap());
                if let Some(value) = value {
                    let val = match value.datatype {
                        IPType::Data(v) => {
                            let val = v8::String::new(scope, &serde_json::to_string(&v).unwrap())
                                .unwrap();
                            v8::json::parse(scope, val).unwrap()
                        }
                        _ => undef_value,
                    };
                    mapped_inputs = mapped_inputs.set(scope, port, val).unwrap();
                    return;
                }
                mapped_inputs = mapped_inputs.set(scope, port, undef_value).unwrap();
            });

            let _id = id.clone();

            fn call_func<'a>(
                module: &v8::Object,
                scope: &mut HandleScope<'a>,
                name: &str,
            ) -> Result<v8::Local<'a, v8::Value>, anyhow::Error> {
                let func_key = v8::String::new(scope, name).unwrap();
                let func = module.get(scope, func_key.into()).unwrap();
                let func = v8::Local::<v8::Function>::try_from(func)?;
                let rec = v8::null(scope).into();
                Ok(func.call(scope, rec, &[]).unwrap())
            }

            let res = call_func(module_obj, scope, &process_name)
                .map_err(|e| ProcessError(e.to_string()))?;

            if let Some(v) = v8::json::stringify(scope, res) {
                let value: serde_json::Value = serde_json::from_str(&v.to_rust_string_lossy(scope))
                    .map_err(|e| ProcessError(e.to_string()))?;
                this.output().send_done(&value)?;
                runner.await.map_err(|e| ProcessError(e.to_string()))?;
                return Ok(ProcessResult {
                    resolved: true,
                    ..Default::default()
                });
            };
            runner.await.map_err(|e| ProcessError(e.to_string()))?;
            Ok(ProcessResult::default())
        })
    });
    return Ok(ProviderRunner {
        runner_id: DENO_RUNNER_ID.to_owned(),
        runner_func,
        platforms: vec![Platform::System],
    });
}

// mod test {

//     use serde_json::json;

//     use crate::{
//         extern_provider::{ExternProvider, ExternProviderSource},
//         network::{BaseNetwork, Network, NetworkOptions},
//     };

//     fn test_deno() {
//         let mut graph = zflow_graph::Graph::new("my_graph", false);
//         graph
//             .add_node("test/add", "add", None)
//             .add_initial(json!(48), "test/add", "a", None)
//             .add_initial(json!(2), "test/add", "b", None);

//         let mut n = Network::create(
//             graph.clone(),
//             NetworkOptions {
//                 subscribe_graph: false,
//                 delay: true,
//                 workspace_dir: "/".to_string(),
//                 ..NetworkOptions::default()
//             },
//         );
//         let deno_provider_source = ExternProviderSource::Url {
//             url: "https://zflow.dev/deno/examples/my_provider.ts".to_owned(),
//         };
//         let deno_provider =
//             futures_lite::future::block_on(ExternProvider::from_deno("/", deno_provider_source))
//                 .unwrap();
//         n.register_provider(deno_provider);

//         let n = n.connect().unwrap();
//         n.start().unwrap();
//     }
// }
