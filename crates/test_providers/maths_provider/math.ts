declare global {
    interface Context {
        sendDone: (data: any) => void;
        send: (data: any) => void;
        sendBuffer: (port: string, data: Uint8Array) => void;
    }
}

type Input = {
    a: number;
    b: number;
}

const add = async (context: Context, input: Input) => {
    context.sendDone({ result: input.a + input.b })
}

const sub = async (context: Context, input: Input) => {
    context.sendDone({ result: input.a - input.b })
}


export const providerId = () => "@test/deno_provider";

export const getLogo = () => "";

export const getPlatform = () => "System";

export const getPackages = () => [
    {
        "package_id": "math",
        "components": [
            {
                "name": "math/add",
                "inports": {
                    "a": {
                        "trigerring": true,
                        "control": true
                    },
                    "b": {
                        "trigerring": true,
                        "control": true
                    }
                },
                "outports": {
                    "result": {}
                },
                "description": "Addition of two numbers",
                "ordered": true
            },
            {
                "name": "math/sub",
                "inports": {
                    "input": {
                        "trigerring": true,
                        "control": true
                    }
                },
                "outports": {
                    "result": {}
                },
                "description": "Substraction of two numbers",
                "ordered": true
            }
        ]
    }
];


export const runComponent = async (context: Context, payload: any) => {
    const componentName = payload.component.name;
    await fetch('http://github.com');
    switch (componentName) {
        case "math/add": {
            await add(context, payload.input as Input);
            break;
        }
        case "math/sub": {
            await sub(context, payload.input as Input);
            break;
        }
        default: throw "Unknown process"
    }
}