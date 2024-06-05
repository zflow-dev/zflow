
use gloo_utils::format::JsValueSerdeExt;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tsify::Tsify;
use wasm_bindgen::prelude::*;
use crate::types::*;
use crate::Graph;


#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = Graph))]
pub struct WasmGraph {
    #[serde(skip)]
    _graph: crate::Graph,
}

#[cfg_attr(feature = "build_wasm", wasm_bindgen(js_class = Graph))]
impl WasmGraph {
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(constructor))]
    pub fn new(name: &str, case_sensitive: bool) -> WasmGraph {
        let _graph = crate::Graph::new(name, case_sensitive);
        WasmGraph { _graph }
    }

    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = getPortName))]
    pub fn get_port_name(&self, port: &str) -> String {
        if self._graph.case_sensitive {
            return port.to_string();
        }
        port.to_lowercase()
    }
   
    /// This method allows changing properties of the graph.
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = setProperties))]
    pub fn set_properties(&mut self, properties: JsValue) -> Result<JsValue, JsError> {
        let metadata = properties.into_serde::<Map<String, Value>>()?;
        self._graph.set_properties(metadata);

        Ok(JsValue::undefined())
    }

    /// Nodes objects can be retrieved from the graph by their ID:
    /// ```no_run
    /// const node = my_graph.getNode('Read');
    /// ```
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = getNode))]
    pub fn get_node(&self, key: &str) -> Option<GraphNode> {
        self._graph.get_node(key).cloned()
    }

    /// Add an inport to the graph
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addInport))]
    pub fn add_inport(
        &mut self,
        public_port: &str,
        node_key: &str,
        port_key: &str,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        if metadata.is_null() || metadata.is_undefined() {
            self._graph
                .add_inport(public_port, node_key, port_key, None);
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph
                .add_inport(public_port, node_key, port_key, Some(metadata));
        }

        Ok(JsValue::undefined())
    }

    /// Remove an inport from the graph
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = removeInport))]
    pub fn remove_inport(&mut self, public_port: &str) {
        self._graph.remove_inport(public_port);
    }

    /// Rename an inport
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = renameInport))]
    pub fn rename_inport(&mut self, old: &str, new: &str) {
        self._graph.rename_inport(old, new);
    }

    /// Add an outport to the graph
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addOutport))]
    pub fn add_outport(
        &mut self,
        public_port: &str,
        node_key: &str,
        port_key: &str,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        if metadata.is_null() || metadata.is_undefined() {
            self._graph
                .add_outport(public_port, node_key, port_key, None);
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph
                .add_outport(public_port, node_key, port_key, Some(metadata));
        }

        Ok(JsValue::undefined())
    }

    /// Remove an outport from the graph
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = removeOutport))]
    pub fn remove_outport(&mut self, public_port: &str) {
        self._graph.remove_outport(public_port);
    }

    /// Rename an outport
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = renameOutport))]
    pub fn rename_outport(&mut self, old: &str, new: &str) {
        self._graph.rename_outport(old, new);
    }

    /// Grouping nodes in a graph
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addGroup))]
    pub fn add_group(
        &mut self,
        group: &str,
        nodes: Vec<String>,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        if metadata.is_null() || metadata.is_undefined() {
            self._graph.add_group(group, nodes, None);
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph.add_group(group, nodes, Some(metadata));
        }
        Ok(JsValue::undefined())
    }

    /// Rename group
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = renameGroup))]
    pub fn rename_group(&mut self, old: &str, new: &str) {
        self._graph.rename_group(old, new);
    }

    /// Remove group from the graph
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = removeGroup))]
    pub fn remove_group(&mut self, group: &str) {
        self._graph.remove_group(group);
    }

    /// Adding a node to the graph
    /// Nodes are identified by an ID unique to the graph. Additionally,
    /// a node may contain information on what FBP component it is and
    /// possibly display coordinates.
    /// ```js
    /// let metadata = {"x": 91, "y": 154};
    /// myGraph.addNode("Read", "ReadFile", metadata);
    /// ```
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addNode))]
    pub fn add_node(
        &mut self,
        id: &str,
        component: &str,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        if metadata.is_null() || metadata.is_undefined() {
            self._graph.add_node(id, component, None);
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph.add_node(id, component, Some(metadata));
        }
        Ok(JsValue::undefined())
    }
    /// Removing a node from the graph
    /// Existing nodes can be removed from a graph by their ID. This
    /// will remove the node and also remove all edges connected to it.
    /// ```js
    /// myGraph.removeNode('Read');
    /// ```
    /// Once the node has been removed, the `remove_node` event will be
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = removeNode))]
    pub fn remove_node(&mut self, id: &str) {
        self._graph.remove_node(id);
    }

    /// Renaming a node
    ///
    /// Nodes IDs can be changed by calling this method.
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = renameNode))]
    pub fn rename_node(&mut self, old_id: &str, new_id: &str) {
        self._graph.rename_node(old_id, new_id);
    }

    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = setNodeMetadata))]
    pub fn set_node_metadata(&mut self, id: &str, metadata: JsValue) -> Result<JsValue, JsError> {
        if metadata.is_null() || metadata.is_undefined() {
            self._graph.set_node_metadata(id, Map::new());
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph.set_node_metadata(id, metadata);
        }
        Ok(JsValue::undefined())
    }

    /// Connecting nodes
    ///
    /// Nodes can be connected by adding edges between a node's outport
    ///	and another node's inport:
    /// ```js
    /// myGraph.addEdge("Read", "out", "Display", "in");
    /// myGraph.addEdgeIndex("Read", "out", None, "Display", "in", 2);
    /// ```
    /// Adding an edge will emit the `addEdge` event.
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addEdge))]
    pub fn add_edge(
        &mut self,
        out_node: &str,
        out_port: &str,
        in_node: &str,
        in_port: &str,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        if metadata.is_null() || metadata.is_undefined() {
            self._graph
                .add_edge(out_node, out_port, in_node, in_port, None);
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph
                .add_edge(out_node, out_port, in_node, in_port, Some(metadata));
        }
        Ok(JsValue::undefined())
    }

    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addEdgeIndex))]
    pub fn add_edge_index(
        &mut self,
        out_node: &str,
        out_port: &str,
        index_1: usize,
        in_node: &str,
        in_port: &str,
        index_2: usize,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        if metadata.is_null() || metadata.is_undefined() {
            self._graph.add_edge_index(
                out_node,
                out_port,
                Some(index_1),
                in_node,
                in_port,
                Some(index_2),
                None,
            );
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph.add_edge_index(
                out_node,
                out_port,
                Some(index_1),
                in_node,
                in_port,
                Some(index_2),
                Some(metadata),
            );
        }
        Ok(JsValue::undefined())
    }

    /// Disconnected nodes
    ///
    /// Connections between nodes can be removed by providing the
    ///	nodes and ports to disconnect.
    /// ```js
    /// myGraph.removeEdge("Display", "out", "Foo", "in");
    /// ```
    /// Removing a connection will emit the `removeEdge` event.
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = removeEdge))]
    pub fn remove_edge(
        &mut self,
        node: &str,
        port: &str,
        node2: Option<String>,
        port2: Option<String>,
    ) {
        if !node2.is_none() || !port2.is_none() {
            self._graph.remove_edge(node, port, None, None);
        } else {
            let node_2 = node2.unwrap();
            let port_2 = port2.unwrap();
            self._graph
                .remove_edge(node, port, Some(node_2.as_str()), Some(port_2.as_str()));
        }
    }
    /// Getting an edge
    ///
    /// Edge objects can be retrieved from the graph by the node and port IDs:
    /// ```no_run
    /// myEdge = myGraph.getEdge("Read", "out", "Write", "in");
    /// ```
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = getEdge))]
    pub fn get_edge(&self, node: &str, port: &str, node2: &str, port2: &str) -> Option<GraphEdge> {
        self._graph.get_edge(node, port, node2, port2)
    }

    /// Changing an edge's metadata
    ///
    /// Edge metadata can be set or changed by calling this method.
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = setEdgeMetadata))]
    pub fn set_edge_metadata(
        &mut self,
        node: &str,
        port: &str,
        node2: &str,
        port2: &str,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        if metadata.is_null() || metadata.is_undefined() {
            self._graph
                .set_edge_metadata(node, port, node2, port2, Map::new());
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph
                .set_edge_metadata(node, port, node2, port2, metadata);
        }
        Ok(JsValue::undefined())
    }

    /// Adding Initial Information Packets
    ///
    /// Initial Information Packets (IIPs) can be used for sending data
    /// to specified node inports without a sending node instance.
    ///
    /// IIPs are especially useful for sending configuration information
    /// to components at FBP network start-up time. This could include
    /// filenames to read, or network ports to listen to.
    ///
    /// ```js
    /// myGraph.addInitial("somefile.txt", "Read", "source");
    /// myGraph.addInitialIndex("somefile.txt", "Read", "source", 2);
    /// ```
    /// If inports are defined on the graph, IIPs can be applied calling
    /// the `addGraphInitial` or `addGraphInitialIndex` methods.
    /// ```js
    /// myGraph.addGraphInitial("somefile.txt", "file");
    ///	myGraph.addGraphInitialIndex("somefile.txt", "file", 2);
    /// ```
    ///
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addInitial))]
    pub fn add_initial(
        &mut self,
        data: JsValue,
        node: &str,
        port: &str,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        let _data = data.into_serde::<Value>().unwrap();
        if metadata.is_null() || metadata.is_undefined() {
            self._graph.add_initial(_data, node, port, None);
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph.add_initial(_data, node, port, Some(metadata));
        }
        Ok(JsValue::undefined())
    }
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addInitialIndex))]
    pub fn add_initial_idex(
        &mut self,
        data: JsValue,
        node: &str,
        port: &str,
        index: usize,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        let _data = data.into_serde::<Value>().unwrap();
        if metadata.is_null() || metadata.is_undefined() {
            self._graph
                .add_initial_index(_data, node, port, Some(index), None);
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph
                .add_initial_index(_data, node, port, Some(index), Some(metadata));
        }
        Ok(JsValue::undefined())
    }

    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addGraphInitial))]
    pub fn add_graph_initial(
        &mut self,
        data: JsValue,
        node: &str,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        let _data = data.into_serde::<Value>().unwrap();
        if metadata.is_null() || metadata.is_undefined() {
            self._graph.add_graph_initial(_data, node, None);
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph.add_graph_initial(_data, node, Some(metadata));
        }
        Ok(JsValue::undefined())
    }
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = addGraphInitialIndex))]
    pub fn add_graph_initial_index(
        &mut self,
        data: JsValue,
        node: &str,
        index: usize,
        metadata: JsValue,
    ) -> Result<JsValue, JsError> {
        let _data = data.into_serde::<Value>().unwrap();
        if metadata.is_null() || metadata.is_undefined() {
            self._graph.add_graph_initial_index(_data, node, Some(index), None);
        } else {
            let metadata = metadata.into_serde::<Map<String, Value>>()?;
            self._graph.add_graph_initial_index(_data, node, Some(index), Some(metadata));
        }
        Ok(JsValue::undefined())
    }

    /// Removing Initial Information Packets
    ///
    /// IIPs can be removed by calling the `removeInitial` method.
    /// ```js
    /// myGraph.removeInitial("Read", "source");
    /// ```
    /// If the IIP was applied via the `addGraphInitial` or
    /// `addGraphInitialIndex` functions, it can be removed using
    /// the `removeGraphInitial` method.
    /// ```js
    /// myGraph.removeGraphInitial("file");
    /// ```
    /// Remove an IIP will emit a `removeInitial` event.
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = removeInitial))]
    pub fn remove_initial(&mut self, id: &str, port: &str) {
        self._graph.remove_initial(id, port);
    }
    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = removeGraphInitial))]
    pub fn remove_graph_initial(&mut self, id: &str) {
        self._graph.remove_graph_initial(id);
    }

    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = toJSON))]
    pub fn to_json(&self) -> GraphJson {
        self._graph.to_json()
    }

    #[cfg_attr(feature = "build_wasm", wasm_bindgen(js_name = fromJSON))]
    pub fn from_json(json: GraphJson, metadata: JsValue) -> WasmGraph {
        if metadata.is_null() || metadata.is_undefined() {
            let _graph = Graph::from_json(json, None);
            return Self{_graph};
        } else {
            let _graph = Graph::from_json(json, Some(metadata.into_serde::<Map<String, Value>>().unwrap()));
            return Self{_graph};
        }
    }
}