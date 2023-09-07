
export const process = (data) => {
    if(!data) return;
    if (data.left === null && data.right === null ) return;

    const left = data.left
    const right = data.right

    zflow.console.log(left+right);
    zflow.sendDone({ sum: Number(left) + Number(right) })
}