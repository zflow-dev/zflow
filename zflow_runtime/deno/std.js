((globalThis) => {
    const { core } = Deno;
    const { ops } = core;

    globalThis.zflow = {
        send: (input)=>{
            return ops.op_output_send(input);
        },
        sendDone: (input)=>{
            return ops.op_output_send_done(input);
        },
        sendBuffer: (port, buf)=>{
            return ops.op_output_send(port, buf);
        }
    }
})(globalThis)