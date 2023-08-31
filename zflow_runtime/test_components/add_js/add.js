
export const process = (inputs) => {
    const data = inputs
    if (data.left && data.right) {
        const result = Number(data.left) + Number(data.right);
       zflow.send({ sum: result })
    //    return { sum: result }
    }
}