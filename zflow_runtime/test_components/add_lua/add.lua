function zflow.process(data)
    if data ~= nil then
        local left = data.left
        local right = data.right

        if left ~= nil and right ~= nil then
            zflow.send({ sum = left + right })
        end
    end
end