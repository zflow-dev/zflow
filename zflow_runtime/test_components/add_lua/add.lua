if zflow.inports["left"] ~= nil and zflow.inports["right"] ~= nil then
    left = zflow.inports["left"]
    right = zflow.inports["right"]
    zflow.outports.send({sum= left + right})
end