[![Rust](https://github.com/darmie/zflow/actions/workflows/rust.yml/badge.svg)](https://github.com/darmie/zflow/actions/workflows/rust.yml)

# The FBP Graph Specification
An implementation of the  [Flow-Based Programming](https://flow-based.org/) graph specification.
## Graph Usage 
```rust
let mut g = Graph::new("Foo bar", true);
// listen to the graph add_node event
g.connect("add_node", |this, data|{
    if let Ok(node) = GraphNode::deserialize(data){
        assert_eq!(node.id, "Foo");
        assert_eq!(node.component, "Bar");
    }
}, true);
// add a node
g.add_node("Foo", "Bar", None);

// listen to the add_edge event
g.connect("add_edge", |this, data|{
    if let Ok(edge) = GraphEdge::deserialize(data){
        assert_eq!(edge.from.node_id, "Foo");
        assert_eq!(edge.to.port, "In");
    }
});

// add node with ID `Foo` and Component named `foo`
g.add_node("Foo", "foo", None);
// add node with ID `Bar` and Component named `bar`
g.add_node("Bar", "bar", None);
// add a connection between `Foo` and `Bar` by their output port and input ports respectively.
g.add_edge("Foo", "Out", "Bar", "In", None);
```
See [graph_test.rs](https://github.com/darmie/zflow/blob/main/zflow_graph/src/graph_test.rs) for more usage examples

## Journal Usage
```rs
let mut graph = Graph::new("", false);
// start recording events in the graph to the memory journal
graph.start_journal(None);
graph.add_node("Foo", "Bar", None);
graph.add_node("Baz", "Foo", None);
graph.add_edge("Foo", "out", "Baz", "in", None);
graph.add_initial(json!(42), "Foo", "in", None);
graph.remove_node("Foo");

// move to initial state in journal history
graph.move_to_revision(0);
// move to second revision in journal history
graph.move_to_revision(2);
// move to fifth revision in journal history
graph.move_to_revision(5);
```
See [journal.rs](https://github.com/darmie/zflow/blob/main/zflow_graph/src/journal.rs#L1013) for more usage examples