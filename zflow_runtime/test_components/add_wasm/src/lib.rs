use extism_pdk::*;
use serde::{Serialize, Deserialize};

#[derive(Serialize)]
pub struct Output {
   pub sum: i64
}

#[derive(Serialize, Deserialize)]
pub struct Input {
   pub left: Value,
   pub right: Value
}

#[plugin_fn]
pub fn process(input:Json<Input>) -> FnResult<Json<Output>>  {
   let left = input.0.left.as_i64().unwrap();
   let right = input.0.right.as_i64().unwrap();
   return Ok(Json(Output { sum: left + right }));
}

