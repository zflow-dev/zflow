
pub mod transpiler;
// pub mod runtime;
pub mod snapshot;



use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;


use deno_ast::MediaType;
use deno_ast::ParseParams;
use deno_ast::SourceTextInfo;
use deno_broadcast_channel::InMemoryBroadcastChannel;
use deno_cache::CreateCache;
use deno_cache::SqliteBackedCache;
use deno_core::error::AnyError;
use deno_core::futures::FutureExt;

use deno_cron::local::LocalCronHandler;
use deno_http::DefaultHttpPropertyExtractor;
use deno_kv::dynamic::MultiBackendDbHandler;
use deno_runtime::deno_core::*;
use deno_runtime::ops;
use deno_runtime::permissions::PermissionsContainer;
use deno_runtime::BootstrapOptions;
use futures::TryFutureExt;
use tempdir::TempDir;
use v8::Handle;
use v8::HandleScope;
use zflow_plugin::ComponentSource;
use zflow_plugin::Platform;

use crate::deno;
use crate::deno::transpiler::maybe_transpile_source;
use crate::ip::IPType;
use crate::process::ProcessError;
use crate::process::ProcessResult;
use crate::provider::ProviderRunner;
use crate::runner::RunFunc;
use crate::runner::DENO_RUNNER_ID;
use deno_runtime::deno_core::_ops::RustToV8;

pub fn create_deno_runtime(module:ModuleSpecifier) -> Result<JsRuntime, AnyError>{
    deno_runtime::deno_core::extension!(deno_permissions_worker,
        options = {
          permissions: PermissionsContainer,
          enable_testing_features: bool,
        },
        state = |state, options| {
          state.put::<PermissionsContainer>(options.permissions);
          state.put(ops::TestingFeaturesEnabled(options.enable_testing_features));
        },
    );

    let create_cache_fn =
        || SqliteBackedCache::new(TempDir::new("zflow_deno").unwrap().into_path());
    let cache = CreateCache(Arc::new(create_cache_fn));

    let bootstrap: BootstrapOptions = Default::default();

    let stdio = deno_runtime::deno_io::Stdio::default();

    let fs = Arc::new(deno_fs::RealFs);

    let create_web_worker_cb = Arc::new(|_| unimplemented!("web workers are not supported"));

    let exit_code = deno_runtime::worker::ExitCode::default();

    let extensions = vec![
        // Web APIs
        deno_webidl::deno_webidl::init_ops(),
        deno_console::deno_console::init_ops(),
        deno_url::deno_url::init_ops(),
        deno_web::deno_web::init_ops::<PermissionsContainer>(
            Arc::new(deno_web::BlobStore::default()),
            bootstrap.location.clone(),
        ),
        deno_webgpu::deno_webgpu::init_ops(),
        deno_canvas::deno_canvas::init_ops(),
        deno_fetch::deno_fetch::init_ops::<PermissionsContainer>(deno_fetch::Options {
            user_agent: bootstrap.user_agent.clone(),
            // root_cert_store_provider: None,
            // unsafely_ignore_certificate_errors: None,
            file_fetch_handler: Rc::new(deno_fetch::FsFetchHandler),
            ..Default::default()
        }),
        deno_cache::deno_cache::init_ops::<SqliteBackedCache>(Some(cache)),
        deno_websocket::deno_websocket::init_ops::<PermissionsContainer>(
            bootstrap.user_agent.clone(),
            None,
            None,
        ),
        deno_webstorage::deno_webstorage::init_ops(Default::default()),
        deno_crypto::deno_crypto::init_ops(None),
        deno_broadcast_channel::deno_broadcast_channel::init_ops(InMemoryBroadcastChannel::default()),
        deno_ffi::deno_ffi::init_ops::<PermissionsContainer>(),
        deno_net::deno_net::init_ops::<PermissionsContainer>(None, None),
        deno_tls::deno_tls::init_ops(),
        deno_kv::deno_kv::init_ops(MultiBackendDbHandler::remote_or_sqlite::<
            PermissionsContainer,
        >(
            Default::default(),
            None,
            deno_kv::remote::HttpOptions {
                user_agent: bootstrap.user_agent.clone(),
                root_cert_store_provider: None,
                unsafely_ignore_certificate_errors: None,
                client_cert_chain_and_key: None,
                proxy: None,
            },
        )),
        deno_cron::deno_cron::init_ops(LocalCronHandler::new()),
        deno_napi::deno_napi::init_ops::<PermissionsContainer>(),
        deno_http::deno_http::init_ops::<DefaultHttpPropertyExtractor>(),
        deno_io::deno_io::init_ops(Some(stdio)),
        deno_fs::deno_fs::init_ops::<PermissionsContainer>(fs.clone()),
        deno_node::deno_node::init_ops::<PermissionsContainer>(None, fs.clone()),
        // Ops from deno runtime
        ops::worker_host::deno_worker_host::init_ops(
            create_web_worker_cb.clone(),
            Default::default(),
        ),
        ops::fs_events::deno_fs_events::init_ops(),
        ops::os::deno_os::init_ops(exit_code.clone()),
        ops::permissions::deno_permissions::init_ops(),
        ops::process::deno_process::init_ops(),
        ops::signal::deno_signal::init_ops(),
        ops::tty::deno_tty::init_ops(),
        ops::http::deno_http_runtime::init_ops(),
        ops::bootstrap::deno_bootstrap::init_ops(Some(Default::default())),
        deno_permissions_worker::init_ops(PermissionsContainer::allow_all(), true),
        deno_runtime::runtime::init_ops(),
        deno_runtime::ops::runtime::deno_runtime::init_ops(module),
        ops::web_worker::deno_web_worker::init_ops(),
    ];

    let zflow_deno_runtime = deno_runtime::deno_core::JsRuntime::new(deno_runtime::deno_core::RuntimeOptions{
        module_loader: Some(Rc::new(DenoModuleLoader)),
        extensions,
        extension_transpiler: Some(Rc::new(|specifier, source| {
            maybe_transpile_source(specifier, source)
          })),
        is_main: false,
        startup_snapshot: Some(deno::snapshot::DENO_SNAPSHOT),
        ..Default::default()
    });

    Ok(zflow_deno_runtime)
}


pub struct DenoModuleLoader;


impl deno_runtime::deno_core::ModuleLoader for DenoModuleLoader {
   
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: deno_runtime::deno_core::ResolutionKind,
    ) -> Result<deno_runtime::deno_core::ModuleSpecifier, anyhow::Error> {
        deno_runtime::deno_core::resolve_import(specifier, referrer).map_err(|e| e.into())
    }

    fn load(
        &self,
        module_specifier: &deno_runtime::deno_core::ModuleSpecifier,
        _maybe_referrer: Option<&deno_runtime::deno_core::ModuleSpecifier>,
        _is_dyn_import: bool,
        _requested_module_type: deno_runtime::deno_core::RequestedModuleType,
    ) -> deno_runtime::deno_core::ModuleLoadResponse {
        let module_specifier = module_specifier.clone();

        let module = async move {
            let scheme = module_specifier.scheme();
            let is_remote = scheme == "http" || scheme == "https";
            let path = if is_remote {
                PathBuf::from(module_specifier.as_str())
            } else {
                module_specifier.to_file_path().unwrap()
            };
            // Determine what the MediaType is (this is done based on the file
            // extension) and whether transpiling is required.
            let media_type = if path.starts_with("node:") {
                MediaType::TypeScript
            } else {
                MediaType::from_path(&path)
            };
            

            let (module_type, should_transpile) = match deno_ast::MediaType::from_path(&path) {
                MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                    (deno_runtime::deno_core::ModuleType::JavaScript, true)
                }
                MediaType::Jsx => (deno_runtime::deno_core::ModuleType::JavaScript, true),
                MediaType::TypeScript
                | MediaType::Mts
                | MediaType::Cts
                | MediaType::Dts
                | MediaType::Dmts
                | MediaType::Dcts
                | MediaType::Tsx => (deno_runtime::deno_core::ModuleType::JavaScript, true),
                MediaType::Json => (deno_runtime::deno_core::ModuleType::Json, false),
                _ => panic!("Unknown extension {:?}", path.extension()),
            };

            // Transpile if Typescript
            let code = if !is_remote {
                std::fs::read_to_string(&path)?
            } else {
                reqwest::get(module_specifier.as_str())
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap()
            };
          
            let code = if should_transpile {
                let parsed = deno_ast::parse_module(ParseParams {
                    specifier: module_specifier.clone(),
                    text_info: SourceTextInfo::from_string(code),
                    media_type,
                    capture_tokens: false,
                    scope_analysis: false,
                    maybe_syntax:None,
                })?;
   
                parsed.transpile(&Default::default())?.text
            } else {
                code
            };

            // Load and return module.
            let module = deno_runtime::deno_core::ModuleSource::new(
                module_type,
                deno_runtime::deno_core::ModuleSourceCode::Bytes(deno_runtime::deno_core::ModuleCodeBytes::Boxed(
                    code.into_bytes().into_boxed_slice(),
                )),
                &module_specifier,
            );

            Ok(module)
        }
        .boxed_local();

        deno_runtime::deno_core::ModuleLoadResponse::Async(module)
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
    module: deno_runtime::deno_core::ModuleSpecifier,
    is_provider: bool,
) -> Result<ProviderRunner, anyhow::Error> {
    let id = component.name.to_owned();
    let process_name = process_name.to_owned();
    let component = component.clone();
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

        let mut runtime = create_deno_runtime(module.clone()).map_err(|err| ProcessError(err.to_string()))?;

  
        let current_thread = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|err| ProcessError(err.to_string()))?;
    
        let component = component.clone();

        current_thread.block_on(async move {
            let mod_id = runtime
                .load_main_es_module(&module)
                .map_err(|e| ProcessError(e.to_string()))
                .await?;
            let runner = runtime.mod_evaluate(mod_id);
            runtime
                .run_event_loop(deno_runtime::deno_core::PollEventLoopOptions {
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
                        return _output.clone().send_done(&serde_json::json!(null));
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
                v8::Local::<v8::Value>::from(v8::String::new(scope, "sendDone").unwrap());
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
                v8::Local::<v8::Value>::from(v8::String::new(scope, "sendBuffer").unwrap());
            let send_buf_fn =
                v8::Local::<v8::Value>::from(v8::Local::<v8::Function>::new(scope, send_buf_fn));
            zflow_obj.set(scope, send_buf_fn_key, send_buf_fn).unwrap();


            let component_with_input = v8::Object::new(scope);
            let mapped_inputs = v8::Object::new(scope);

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
                    mapped_inputs.set(scope, port, val).unwrap();
                    return;
                }
                mapped_inputs.set(scope, port, undef_value).unwrap();
            });

            let _id = id.clone();

            let components_key =
                v8::Local::<v8::Value>::from(v8::String::new(scope, "component").unwrap());
            let inputs_key = v8::Local::<v8::Value>::from(v8::String::new(scope, "input").unwrap());

            let component = serde_v8::to_v8(scope, component.clone())
                .map_err(|e| ProcessError(e.to_string()))?;
            component_with_input.set(scope, components_key, component);
            component_with_input.set(scope, inputs_key, mapped_inputs.into());
            let call_input = v8::Local::<v8::Value>::from(if is_provider {
                component_with_input
            } else {
                mapped_inputs
            });

            fn call_func<'a>(
                module: &v8::Object,
                scope: &mut HandleScope<'a>,
                name: &str,
                args: &[v8::Local<v8::Value>],
            ) -> Result<v8::Local<'a, v8::Value>, anyhow::Error> {
                let func_key = v8::String::new(scope, name).unwrap();
                let func = module.get(scope, func_key.into()).unwrap();
                let func = v8::Local::<v8::Function>::try_from(func)?;
                
                let rec = v8::undefined(scope).into();
                let val = func.call(scope, rec, args).unwrap();
                if val.is_promise() {
                    let promise = v8::Local::<v8::Promise>::try_from(val)?;
                
                    loop{
                        match promise.state() {
                            v8::PromiseState::Pending => continue,
                            v8::PromiseState::Fulfilled => break,
                            v8::PromiseState::Rejected => break,
                        }
                    }

                    return Ok(promise.result(scope))
                }
                
               Ok(val)  
            }

            

            let res = call_func(module_obj, scope, &process_name, &[zflow_obj.into(), call_input])
                .map_err(|e| ProcessError(e.to_string()))?;

            if !res.is_null_or_undefined() {
                if let Some(v) = v8::json::stringify(scope, res) {
                    let value: serde_json::Value = serde_json::from_str(&v.to_rust_string_lossy(scope))
                        .map_err(|e| ProcessError(e.to_string()))?;
                    
                    return Ok(ProcessResult {
                        resolved: true,
                        data: value,
                        ..Default::default()
                    });
                }
            }
          
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
