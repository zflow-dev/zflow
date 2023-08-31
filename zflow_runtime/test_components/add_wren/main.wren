import "zflow" for ZFlow

class Main {
    static process(input){
        if(input == null) return
        if(!input.containsKey("left") && !input.containsKey("right")) return
        var result = input["left"] + input["right"]
        ZFlow.send({"sum": result})
    }
}