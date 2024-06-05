const add = async (context,input) => {
    context.sendDone({ result: input.a + input.b })
}

const sub = async (context,input) => {
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


export const runComponent = (context, payload) => {
    const componentName = payload.component.name;
    // fmt.printf("%s", JSON.stringify(payload));
    fetch("https://deno.com").then(async (response)=>{
        console.log(response.status); 
        console.log(response.statusText);
        const jsonData = await response.json();
        console.log(jsonData)
    });

    switch (componentName) {
        case "math/add": {
            add(context, payload.input);
            break;
        }
        case "math/sub": {
            sub(context, payload.input);
            break;
        }
        default: throw "Unknown process"
    }
}