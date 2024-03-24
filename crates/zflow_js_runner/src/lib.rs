pub mod std_lib;
pub mod utils;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rquickjs::{Context, Func, Object, Runtime};

use zflow_runtime::{
    ip::IPType,
    port::PortOptions,
    process::{ProcessError, ProcessHandle, ProcessResult},
    provider::ProviderComponent,
    runner::RunFunc,
};

use crate::utils::{js_value_to_json_value, json_value_to_js_value};

pub fn create_quickjs_runner(
    source: &'static str,
    component: impl ProviderComponent,
) -> Result<Box<RunFunc>, anyhow::Error> {
    let path = PathBuf::from(source);
    if !path.is_file() || !path.try_exists()? {
        return Err(anyhow::Error::msg("Could not create Javascript runner"));
    }

    let mut inports = component.get_inports();
    let mut outports = component.get_outports();
    if inports.is_empty() {
        inports.insert("in".to_owned(), PortOptions::default());
    }
    if outports.is_empty() {
        outports.insert("out".to_owned(), PortOptions::default());
    }

    return Ok(Box::new(move |handle| unsafe {
        let src = source.as_bytes();
        let res = &mut Result::Ok(ProcessResult::default()) as *mut Result<ProcessResult, ProcessError>;
        let res = run(&src[0], src.len(), &handle.clone(), res);
        res.read()
    }));
}

#[no_mangle]
pub unsafe fn run(
    source: *const u8,
    length: usize,
    handle: *const Arc<Mutex<ProcessHandle>>,
    result: *mut Result<ProcessResult, ProcessError>,
) -> *const Result<ProcessResult, ProcessError> {
    let source = std::str::from_utf8(std::slice::from_raw_parts(source, length))
        .expect("Expected to read source");
    let handle_binding = handle.clone().read();
    let mut handle_binding = handle_binding.try_lock();
    let this = handle_binding
        .as_mut()
        .map_err(|_| ProcessError(String::from("Process Handle has dropped")))
        .expect("Tried to get handle");

    let inports = this.input().in_ports.ports;

    let controlled_data = this
        .input()
        .in_ports
        .ports
        .iter()
        .filter(|(_, port)| port.options.control)
        .map(|(key, _)| this.input().get(key))
        .collect::<Vec<_>>();

    if !controlled_data.is_empty() && controlled_data.contains(&None) {
        return &Ok(ProcessResult::default());
    }

    let inport_keys = inports.keys();

    let rt = Runtime::new().expect("runtime error");
    let context = Context::full(&rt).unwrap();

    *result = context.with(|ctx| {
        let global = ctx.globals();
        let mut _inputs = rquickjs::Object::new(ctx).unwrap();

        for key in inport_keys {
            let value = this.input().get(key);
            if let Some(value) = value {
                _inputs
                    .set(
                        key,
                        match value.datatype {
                            IPType::Data(v) => {
                                json_value_to_js_value(ctx, v).expect("runtime error")
                            }
                            _ => ctx.eval("null").unwrap(),
                        },
                    )
                    .expect("runtime error");
            }
        }

        let mut _outputs = rquickjs::Object::new(ctx).unwrap();
        let output = this.output.clone();
        _outputs
            .set(
                "send",
                Func::new("", move |data: rquickjs::Value| {
                    let data = js_value_to_json_value(data).expect("runtime error");
                    output.clone().send(&data).unwrap();
                }),
            )
            .expect("runtime error");
        let output = this.output.clone();
        _outputs
            .set(
                "sendBuffer",
                Func::new("", move |port: rquickjs::String, data: rquickjs::Array| {
                    let v = data
                        .into_iter()
                        .map(|d| d.unwrap().as_int().unwrap() as u8)
                        .collect::<Vec<u8>>();
                    output
                        .clone()
                        .send_buffer(
                            &port.to_string().expect("expected port name to be string"),
                            &v,
                        )
                        .unwrap();
                }),
            )
            .expect("runtime error");

        let output = this.output.clone();
        _outputs
            .set(
                "sendDone",
                Func::new("", move |data: rquickjs::Value| {
                    output
                        .clone()
                        .send_done(&js_value_to_json_value(data).expect("runtime error"))
                        .unwrap();
                }),
            )
            .expect("runtime error");

        global
            .set("zflow", _outputs.as_value().clone())
            .expect("runtime error");

        let m = ctx.compile("process", source).expect("runtime error");

        let f = m
            .get::<&str, rquickjs::Function>("process")
            .expect("runtime error");

        let data = js_value_to_json_value(
            f.call::<(Object,), rquickjs::Value>((_inputs,))
                .expect("runtime error"),
        )?;
        if !data.is_null() {
            return Ok(ProcessResult{
                resolved: true,
                data,
                ..ProcessResult::default()
            })
        }

        return Ok(ProcessResult::default())
    });

    result
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        env,
        sync::{Arc, Mutex},
    };

    use libloading::{Library, Symbol};

    use zflow_runtime::{
        port::{InPort, InPorts, InPortsOptions, OutPort, OutPorts, OutPortsOptions},
        process::{ProcessHandle, ProcessInput, ProcessOutput, ProcessResult},
    };

    use zflow_runtime::runner::UnsafeRunFunc;

    #[test]
    fn test_lib() {
        let mut dir = env::current_dir().unwrap();
        dir.push("./bin/debug/libzflow_js_runner.dylib");
        if !dir.exists() || !dir.is_file() {
            panic!("Please the build library in debug mode before test")
        }
        unsafe {
            let lib = Library::new(dir).unwrap();
            let run: Symbol<UnsafeRunFunc> = lib.get(b"run").unwrap();
            let data = b"export const process = ({input}) => {
                //if(!input) return;
                //if (input.left === null && input.right === null ) return;
            
                //const left = input.left
                //const right = input.right
                //zflow.sendDone({ sum: Number(left) + Number(right) })
                zflow.sendDone('hello world')
            }";
            let process = Arc::new(Mutex::new(ProcessHandle {
                input: ProcessInput {
                    in_ports: InPorts::new(InPortsOptions {
                        ports: HashMap::from([
                            ("left".to_string(), InPort::default()),
                            ("right".to_string(), InPort::default()),
                        ]),
                    }),
                    ..ProcessInput::default()
                },
                output: ProcessOutput {
                    out_ports: OutPorts::new(OutPortsOptions {
                        ports: HashMap::from([("sum".to_string(), OutPort::default())]),
                    }),
                    ..Default::default()
                },
                ..Default::default()
            }));
            let res = std::ptr::from_mut(&mut Result::Ok(ProcessResult::default()));
            run(
                &data[0] as *const u8,
                data.len(),
                &process,
                res,
            );
            let data = res.read();
            println!("res: {:?}", data);
        }
    }
}
