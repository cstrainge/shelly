
use std::collections::HashMap;

use serde_json::{ Value as Json, Number, Map, from_str, to_string };

use crate::language::data::{ value::{ Value, Executable }, map_key::MapKey, types::TypeKind };


pub(super) fn encode(value: &Value) -> Result<String, String>
{
    to_string(&to_json(value)?).map_err(|error| error.to_string())
}


fn to_json(value: &Value) -> Result<Json, String>
{
    Ok(match value.underlying()
        {
            Value::None => Json::Null,
            Value::Boolean(value) => Json::Bool(*value),
            Value::Integer(value) => Json::Number(Number::from(*value)),
            Value::Float(value, _) => Json::Number(Number::from_f64(*value)
                .ok_or_else(|| "JSON requires finite numbers".to_string())?),
            Value::String(value, Executable::No) => Json::String(value.clone()),
            Value::Array(items) => Json::Array(items.iter().map(to_json)
                .collect::<Result<_, _>>()?),
            Value::HashMap(items) =>
                {
                    let mut object = Map::new();
                    for (key, value) in items.iter()
                    {
                        let MapKey::String(key) = key else
                            { return Err("JSON object keys must be Strings".into()); };
                        object.insert(key.clone(), to_json(value)?);
                    }
                    Json::Object(object)
                },
            Value::Struct(item) =>
                {
                    let TypeKind::Struct(fields) = &item.definition.kind else { unreachable!(); };
                    let mut object = Map::new();
                    for (field, value) in fields.iter().zip(&item.fields)
                    { object.insert(field.name.clone(), to_json(value)?); }
                    Json::Object(object)
                },
            value => return Err(format!("JSON cannot encode {}", value.type_name())),
        })
}


pub(super) fn decode(text: &str) -> Result<Value, String>
{
    from_json(from_str(text).map_err(|error| error.to_string())?)
}


fn from_json(value: Json) -> Result<Value, String>
{
    Ok(match value
        {
            Json::Null => Value::None,
            Json::Bool(value) => Value::Boolean(value),
            Json::Number(value) =>
                {
                    if let Some(number) = value.as_i64() { Value::Integer(number) }
                    else if value.is_u64()
                    { return Err("JSON integer exceeds the signed 64-bit range".into()); }
                    else { Value::Float(value.as_f64().ok_or_else(||
                        "JSON number is out of range".to_string())?, None) }
                },
            Json::String(value) => Value::from_string(value),
            Json::Array(values) => Value::from_array(values.into_iter().map(from_json)
                .collect::<Result<_, _>>()?),
            Json::Object(values) => Value::from_hash_map(values.into_iter()
                .map(|(key, value)| Ok((MapKey::String(key), from_json(value)?)))
                .collect::<Result<HashMap<_, _>, String>>()?),
        })
}
