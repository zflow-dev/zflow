
export const process = (data) => {
    if(!data) return;
    const left = data.left
    const right = data.right
    // zflow.console.log(left, right);
    if (left && right) {
       zflow.send({ sum: Number(left) + Number(right) })
    }
}