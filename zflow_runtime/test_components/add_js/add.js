
export const process = ({input}) => {
    if(!input) return;
    if (input.left === null && input.right === null ) return;

    const left = input.left
    const right = input.right

    zflow.console.log(left+right);
    zflow.sendDone({ sum: Number(left) + Number(right) })
}