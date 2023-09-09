function zflow.process(data)
    if data.input ~= nil then
        local input = data["input"]
        local left = input.left
        local right = input.right

        if left ~= nil and right ~= nil then
            zflow.send_done({ sum = left + right })
        end
    end
end