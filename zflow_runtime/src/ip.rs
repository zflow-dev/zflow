use std::any::Any;

use serde::{Deserialize, Serialize};
use serde_json::Value;



#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub enum IPType {
    OpenBracket(Value),
    CloseBracket(Value),
    Data(Value),
    Buffer(Vec<u8>),
    All(Value),
    #[default]
    Unknown,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct IPOptions {
    pub schema: String,
    pub scope: String,
    pub initial: bool,
    pub clonable: bool,
    pub userdata: Value,
    pub index: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct IP {
    pub index: Option<usize>,
    pub datatype: IPType,
    pub schema: String,
    pub scope: String,
    pub initial: bool,
    // the node
    pub owner: Option<String>,
    pub clonable: bool,
    pub userdata: Value,
}

impl IP {
    pub fn new(datatype: IPType, options: IPOptions) -> Self {
        IP {
            index: options.index,
            datatype,
            schema: options.schema,
            scope: options.scope,
            initial: options.initial,
            clonable: options.clonable,
            owner: None,
            userdata: options.userdata,
        }
    }

    pub fn fake_clone(&self) -> Self {
        // Todo: filter out unimportant data
        self.clone()
    }
    pub fn can_fake_clone(&self) -> bool {
        false
    }
}
