
export const process = ({input}) => {
    if(!input) return;
    if (input.a === null && input.b === null ) return;

    const left = input.a
    const right = input.b
    zflow.sendDone({ result: Number(left) + Number(right) })
}