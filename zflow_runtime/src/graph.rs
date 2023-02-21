use std::sync::{Mutex, Arc};

use zflow::graph::graph::Graph;

pub struct GraphRuntime {
    pub graph:Arc<Mutex<Graph>>
}