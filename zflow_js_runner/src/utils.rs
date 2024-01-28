use rquickjs::Ctx;
use serde_json::{json, Map};
use zflow_runtime::process::ProcessError;




pub fn js_value_to_json_value(value: rquickjs::Value) -> Result<serde_json::Value, ProcessError> {
    if let Some(v) = value.as_string() {
        return Ok(json!(v
            .to_string()
            .expect("expected to read javascript string value")));
    }

    if let Some(v) = value.as_int() {
        return Ok(json!(v));
    }

    if let Some(v) = value.as_float() {
        return Ok(json!(v));
    }
    if let Some(v) = value.as_number() {
        return Ok(json!(v));
    }

    if let Some(v) = value.as_array() {
        let mut arr = Vec::new();
        for val in v.clone().into_iter() {
            if let Ok(val) = val {
                arr.push(js_value_to_json_value(val)?);
            }
        }
        return Ok(json!(arr));
    }

    if let Some(v) = value.as_bool() {
        return Ok(json!(v));
    }

    if let Some(v) = value.as_object() {
        let mut arr = Map::new();
        for val in v.clone().into_iter() {
            if let Ok((k, v)) = val {
                arr.insert(
                    k.to_string()
                        .expect("expected to read key from javascript object"),
                    js_value_to_json_value(v)?,
                );
            }
        }
        return Ok(json!(arr));
    }

    Ok(json!(null))
}

pub fn json_value_to_js_value<'js>(
    ctx: Ctx<'js>,
    value: serde_json::Value,
) -> Result<rquickjs::Value<'js>, ProcessError> {
    if let Some(v) = value.as_str() {
        return Ok(rquickjs::String::from_str(ctx, v)
            .expect("expected to convert value to js value")
            .into());
    }

    if let Some(v) = value.as_i64() {
        let value = rquickjs::Value::new_number(ctx, v as f64);
        return Ok(value);
    }

    if let Some(v) = value.as_f64() {
        let value = rquickjs::Value::new_number(ctx, v);
        return Ok(value);
    }
    if let Some(v) = value.as_u64() {
        let value = rquickjs::Value::new_number(ctx, v as f64);
        return Ok(value);
    }

    if let Some(v) = value.as_array() {
        if let Ok(arr) = rquickjs::Array::new(ctx).as_mut() {
            for (i, val) in v.clone().into_iter().enumerate() {
                arr.set(i, json_value_to_js_value(ctx, val)?).expect("");
            }
            return Ok(arr.as_value().clone());
        }
    }

    if let Some(v) = value.as_bool() {
        if v {
            return Ok(rquickjs::Value::new_bool(ctx, v));
        }
    }

    if let Some(v) = value.as_object() {
        if let Ok(obj) = rquickjs::Object::new(ctx).as_mut() {
            for (k, val) in v.clone().into_iter() {
                obj.set(k, json_value_to_js_value(ctx, val)?).expect("");
            }
            return Ok(obj.as_value().clone());
        }
    }

    Ok(rquickjs::Value::new_undefined(ctx))
}