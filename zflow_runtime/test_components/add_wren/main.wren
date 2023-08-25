import "zflow" for ZFlow

class Component {
    static process(input){
        if(input["left"] != null && input["right"] != null){
            var result = input["left"] + input["right"]
            ZFlow.send("sum", result)
        }
    }
}

