import "zflow" for ZFlow

class Main {
    static process(packet){
        if(packet == null) return
        if(!packet.containsKey("input")) return
        var data = packet["input"]
        if (!data.containsKey("left") && !data.containsKey("right")) return
        var result = data["left"] + data["right"]
        ZFlow.send({"sum": result})
    }
}