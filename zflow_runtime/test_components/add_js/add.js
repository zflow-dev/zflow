

const inport = ProcessHandle.inport;
const outport = ProcessHandle.outport;


if(inport.left && inport.right){
    console.log("hello world!");
    outport.send({sum: Number(inport.left) + Number(inport.right)})
}