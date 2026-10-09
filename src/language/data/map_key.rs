use std::rc::Rc;

use super::value::{ ExecResult, Value };
use super::range::Range;
use super::types::EnumValue;


// Immutable, canonical keys keep hashing consistent with language equality.
// Collections are snapshots; map entry order and string execution flags do not
// affect identity. NaNs form one canonical key so lookup remains reflexive.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MapKey
{
    None,
    Enum(Rc<EnumValue>),
    ExecResult(u8),
    Signaled,
    Integer(i64),
    Float(u64),
    Boolean(bool),
    String(String),
    Range(Range),
    Array(Rc<Vec<MapKey>>),
    ArgumentExpansion(Rc<Vec<MapKey>>),
    HashMap(Rc<Vec<(MapKey, MapKey)>>)
}


impl MapKey
{
    pub fn from_value(value: &Value) -> Self
    {
        match value
        {
            Value::None => Self::None,
            Value::Enum(value) => Self::Enum(value.clone()),
            Value::Range(range) => Self::Range(*range),
            Value::ExecResult(ExecResult::Value(code)) => Self::ExecResult(*code),
            Value::ExecResult(ExecResult::Signaled) => Self::Signaled,
            Value::Integer(value) => Self::Integer(*value),
            Value::Float(value, _) =>
                {
                    if value.fract() == 0.0 && *value >= i64::MIN as f64
                        && *value < -(i64::MIN as f64)
                    {
                        Self::Integer(*value as i64)
                    }
                    else
                    {
                        Self::Float(if value.is_nan() { f64::NAN.to_bits() } else { value.to_bits() })
                    }
                },
            Value::Boolean(value) => Self::Boolean(*value),
            Value::String(value, _) => Self::String(value.clone()),
            Value::Array(values) => Self::Array(Rc::new(values.iter().map(Self::from_value).collect())),
            Value::ArgumentExpansion(values) =>
                Self::ArgumentExpansion(Rc::new(values.iter().map(Self::from_value).collect())),
            Value::HashMap(values) =>
                {
                    let mut entries: Vec<_> = values.iter()
                        .map(|(key, value)| (key.clone(), Self::from_value(value))).collect();
                    entries.sort();
                    Self::HashMap(Rc::new(entries))
                }
        }
    }

    pub fn to_value(&self) -> Value
    {
        match self
        {
            Self::None => Value::None,
            Self::Enum(value) => Value::Enum(value.clone()),
            Self::Range(range) => Value::Range(*range),
            Self::ExecResult(code) => Value::ExecResult(ExecResult::Value(*code)),
            Self::Signaled => Value::ExecResult(ExecResult::Signaled),
            Self::Integer(value) => Value::Integer(*value),
            Self::Float(bits) => Value::Float(f64::from_bits(*bits), None),
            Self::Boolean(value) => Value::Boolean(*value),
            Self::String(value) => Value::from_string(value.clone()),
            Self::Array(values) => Value::from_array(values.iter().map(Self::to_value).collect()),
            Self::ArgumentExpansion(values) =>
                Value::from_argument_expansion(values.iter().map(Self::to_value).collect()),
            Self::HashMap(values) => Value::from_hash_map(values.iter()
                .map(|(key, value)| (key.clone(), value.to_value())).collect())
        }
    }
}
