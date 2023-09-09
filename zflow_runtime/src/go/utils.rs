use std::{borrow::Borrow, collections::HashMap, rc::Rc};

use crate::{goengine::ffi::*, process::ProcessError};
use serde_json::{json, Map, Value};

pub(crate) fn go_value_to_json_value(v: &GosValue) -> Result<Value, ProcessError> {
    if v.is_nil() {
        return Ok(Value::Null);
    }
    match v.typ() {
        ValueType::Void => Ok(Value::Null),
        ValueType::Bool => Ok(json!(*v.as_bool())),
        ValueType::Int => Ok(json!(*v.as_int())),
        ValueType::Int8 => Ok(json!(*v.as_int8())),
        ValueType::Int16 => Ok(json!(*v.as_int16())),
        ValueType::Int32 => Ok(json!(*v.as_int32())),
        ValueType::Int64 => Ok(json!(*v.as_int64())),
        ValueType::Uint => Ok(json!(*v.as_uint())),
        ValueType::UintPtr => Ok(json!(*v.as_uint_ptr())),
        ValueType::Uint8 => Ok(json!(*v.as_uint8())),
        ValueType::Uint16 => Ok(json!(*v.as_uint16())),
        ValueType::Uint32 => Ok(json!(*v.as_uint32())),
        ValueType::Uint64 => Ok(json!(*v.as_uint64())),
        ValueType::Float32 => Ok(json!(v.as_float32().0)),
        ValueType::Float64 => Ok(json!(v.as_float64().0)),
        ValueType::String => {
            let go_str = v.as_string().as_str();
            let string: &str = &go_str.as_ref();
            Ok(json!(string))
        }
        // is this feasible?
        ValueType::Array => {
            let (array_obj, _) = v.as_array::<GosElem>();

            let mut value = vec![];
            for data in array_obj.borrow_data().iter() {
                let d = data.clone().into_value(ValueType::Interface);
                if let Ok(underlying) = d.iface_underlying() {
                    if underlying.is_some() {
                        value.push(go_value_to_json_value(underlying.as_ref().unwrap())?);
                    }
                }
            }

            Ok(json!(value))
        }
        ValueType::Slice => {
            let mut value = vec![];
            if let Some(s) = v.clone().as_slice::<GosElem>() {
                let (slice_obj, _) = s.borrow();
                let (array_obj, _) = slice_obj.array().as_array::<GosElem>();
                for data in array_obj.borrow_data().iter() {
                    let d = data.clone().into_value(ValueType::Interface);
                    if let Ok(underlying) = d.iface_underlying() {
                        if underlying.is_some() {
                            value.push(go_value_to_json_value(underlying.as_ref().unwrap())?);
                        }
                    }
                }
            }
            Ok(json!(value))
        }
        ValueType::Map => {
            let mut m = Map::new();
            if let Some((mp, _)) = v.as_map() {
                let mp = mp.clone();
                for (k, v) in mp.borrow_data_mut().clone().into_iter() {
                    if k.typ() != ValueType::String {
                        return Err(ProcessError(format!(
                            "Hash Map should only be indexed by string"
                        )));
                    }
                    let key = go_value_to_json_value(&k)?.as_str().unwrap().to_string();
                    m.insert(key, go_value_to_json_value(&v)?);
                }
            }
            Ok(json!(m))
        }
        ValueType::Interface => {
            return go_value_to_json_value(v.as_interface().unwrap().underlying_value().unwrap())
        }
        // ValueType::UnsafePtr => {
        //     if let Some(data) = v.as_unsafe_ptr().unwrap().downcast_ref::<ZflowData>() {

        //     }
        //     return data.internal
        // }
        _ => Err(ProcessError(format!("unsupported GoScript type"))),
    }
}

pub(crate) fn map_go_types(v: Value) -> ValueType {
    match v {
        Value::Null => ValueType::Void,
        Value::Bool(_) => ValueType::Bool,
        Value::Number(n) => {
            if n.is_f64() {
                ValueType::Float64
            } else if n.is_i64() {
                ValueType::Int64
            } else {
                ValueType::Uint64
            }
        }
        Value::String(_) => ValueType::String,
        Value::Array(_) => ValueType::Array,
        Value::Object(_) => ValueType::Map,
    }
}

pub(crate) fn to_go_value(ctx: &FfiCtx, value: Value) -> GosValue {
    match value.clone() {
        Value::Null => FfiCtx::new_nil(ValueType::Void),
        Value::Bool(b) => GosValue::from(b),
        Value::Number(d) => {
            if d.is_f64() {
                return GosValue::from(d.as_f64().unwrap());
            } else if d.is_u64() {
                return GosValue::from(d.as_u64().unwrap());
            }
            return GosValue::from(d.as_i64().unwrap());
        }
        Value::String(s) => FfiCtx::new_string(&s),
        Value::Array(a) => {
            let data = a
                .iter()
                .map(|d| ZflowData::from_data(d.clone()).into_val())
                .collect::<Vec<_>>();
            ctx.new_array(data, ValueType::UnsafePtr)
        }
        Value::Object(m) => ctx.new_map(
            m.into_iter()
                .map(|(k, v)| {
                    let _val = ZflowData::from_data(v.clone()).into_val();
                    (FfiCtx::new_string(&k), _val)
                })
                .collect(),
        ),
    }
}


#[derive(Ffi)]
pub struct ZflowDataFfi;

#[ffi_impl(rename = "zflow.data")]
impl ZflowDataFfi {
    pub fn ffi_is_nil(ptr:GosValue) -> RuntimeResult<GosValue> {
        let data = ptr.as_non_nil_unsafe_ptr()?.downcast_ref::<ZflowData>()?;
        return Ok(GosValue::from(data.get_nil().is_some()))
    }
    pub fn ffi_i64(ptr:GosValue) -> RuntimeResult<(GosValue, GosValue)> {
        let data = ptr.as_non_nil_unsafe_ptr()?.downcast_ref::<ZflowData>()?;
     
        if let Some(data)= data.get_i64() {
            return Ok((GosValue::from(data), true.into()))
        }
        Ok((FfiCtx::new_nil(ValueType::Int64), false.into()))
    }
    pub fn ffi_u64(ptr:GosValue) -> RuntimeResult<(GosValue, GosValue)> {
        let data = ptr.as_non_nil_unsafe_ptr()?.downcast_ref::<ZflowData>()?;
     
        if let Some(data)= data.get_u64() {
            return Ok((GosValue::from(data), true.into()))
        }
        Ok((FfiCtx::new_nil(ValueType::Uint64), false.into()))
    }
    pub fn ffi_f64(ptr:GosValue) -> RuntimeResult<(GosValue, GosValue)> {
        let data = ptr.as_non_nil_unsafe_ptr()?.downcast_ref::<ZflowData>()?;
     
        if let Some(data)= data.get_f64() {
            return Ok((GosValue::from(data), true.into()))
        }
        Ok((FfiCtx::new_nil(ValueType::Float64), false.into()))
    }
    pub fn ffi_boolean(ctx:&mut FfiCtx, ptr:GosValue) -> RuntimeResult<GosValue> {
        let data = ptr.as_non_nil_unsafe_ptr()?.downcast_ref::<ZflowData>()?;
        if let Some(data)= data.get_boolean() {
            return Ok(GosValue::from(data))
        }
        Ok(false.into())
    }
    pub fn ffi_str(ctx:&mut FfiCtx, ptr:GosValue) -> RuntimeResult<(GosValue, GosValue)> {
        let data = ptr.as_non_nil_unsafe_ptr()?.downcast_ref::<ZflowData>()?;
        if let Some(data)= data.get_str() {
            return Ok((GosValue::from(data.to_owned()), true.into()))
        }
        Ok((FfiCtx::new_nil(ValueType::String), false.into()))
    }
    pub fn ffi_object(ctx:&mut FfiCtx, ptr:GosValue) -> RuntimeResult<(GosValue, GosValue)> {
        let data = ptr.as_non_nil_unsafe_ptr()?.downcast_ref::<ZflowData>()?;
        if let Some(data)= data.get_object() {
            return Ok((ctx.new_map(data), true.into()))
        }
        Ok((FfiCtx::new_nil(ValueType::Map), false.into()))
    }
    pub fn ffi_array(ctx:&mut FfiCtx, ptr:GosValue) -> RuntimeResult<(GosValue, GosValue)> {
        let data = ptr.as_non_nil_unsafe_ptr()?.downcast_ref::<ZflowData>()?;
        if let Some(data)= data.get_arr() {
            return Ok((ctx.new_array(data, ValueType::UnsafePtr), true.into()))
        }
        Ok((FfiCtx::new_nil(ValueType::Array), false.into()))
    }
}




#[derive(UnsafePtr)]
pub struct ZflowData {
    internal: Value
}

impl ZflowData {
    pub (crate) fn from_data(value:Value) -> Self {
        Self{
            internal: value
        }
    }
    pub (crate) fn get_nil(&self)-> Option<()> {
        self.internal.as_null()
    }
    pub (crate) fn get_i64(&self)-> Option<i64> {
        self.internal.as_i64()
    }
    pub (crate) fn get_f64(&self)-> Option<f64> {
        self.internal.as_f64()
    }
    pub (crate) fn get_u64(&self)-> Option<u64> {
        self.internal.as_u64()
    }
    pub (crate) fn get_str(&self)-> Option<&str> {
        self.internal.as_str()
    }
    pub (crate) fn get_boolean(&self)-> Option<bool> {
        self.internal.as_bool()
    }
    pub (crate) fn get_object(&self) -> Option<HashMap<GosValue, GosValue>> {
        let m = self.internal.as_object()?;
        Some(m.into_iter().map(|(k, v)| (GosValue::from(k.clone()), Self::from_data(v.clone()).into_val())).collect())
    }
    pub (crate) fn get_arr(&self) -> Option<Vec<GosValue>> {
        let m = self.internal.as_array()?;
        Some(m.into_iter().map(|v| Self::from_data(v.clone()).into_val()).collect())
    }

    pub (crate) fn into_val(self) -> GosValue {
        FfiCtx::new_unsafe_ptr(Rc::new(self))
    }

}
