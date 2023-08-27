import "zflow" for ZFlow

class Component {
    static process(input){
        var result = input["left"] + input["right"]
        System.print({"sum": result})
        ZFlow.send("sum", result)
    }
}

