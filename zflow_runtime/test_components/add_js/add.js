

const inport = zflow.inport;
const outport = zflow.outport;


if(inport.left && inport.right){
    const result = Number(inport.left) + Number(inport.right);
    console.log(result);
    outport.send({sum: result})
}