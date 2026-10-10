
use std::{ collections::HashMap, rc::Rc, fmt::{ self, Debug, Formatter } };

use crate::{ runtime::process::Terminal,
             language::{ data::{ map_key::MapKey,
                                 range::Range,
                                 methods::BoundMethod,
                                 types::{ EnumValue, StructValue, TypeKind } },
                         bytecode::FunctionRef } };

#[derive(Clone, Debug, PartialEq)]
pub enum ExecResult
{
    Value(u8),
    Signaled
}


#[derive(Clone)]
pub enum Executable
{
    Yes,
    No,
    Function(FunctionRef),
    Method(Rc<BoundMethod>)
}

impl Debug for Executable
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            Self::Yes => write!(f, "Yes"),
            Self::No => write!(f, "No"),
            Self::Function(function) => write!(f, "Function({:p})", Rc::as_ptr(function)),
            Self::Method(method) => write!(f, "Method({:p})", Rc::as_ptr(method))
        }
    }
}

impl PartialEq for Executable
{
    fn eq(&self, other: &Self) -> bool
    {
        match (self, other)
        {
            (Self::Yes, Self::Yes) | (Self::No, Self::No) => true,
            (Self::Function(left), Self::Function(right)) => Rc::ptr_eq(left, right),
            (Self::Method(left), Self::Method(right)) => Rc::ptr_eq(left, right),
            _ => false
        }
    }
}


#[derive(Clone, Debug, PartialEq)]
pub enum Value
{
    None,
    Terminal(Rc<Terminal>),
    Enum(Rc<EnumValue>),
    Struct(Rc<StructValue>),
    ExecResult(ExecResult),
    Integer(i64),
    Float(f64, Option<String>),
    Boolean(bool),
    String(String, Executable),
    Array(Rc<Vec<Value>>),
    HashMap(Rc<HashMap<MapKey, Value>>),
    Range(Range),
    ArgumentExpansion(Rc<Vec<Value>>)
}


impl Value
{
    pub fn type_name(&self) -> String
    {
        match self
        {
            Value::None => "None",
            Value::Terminal(_) => "Terminal",
            Value::Integer(_) => "Integer",
            Value::Float(_, _) => "Float",
            Value::Boolean(_) => "Boolean",
            Value::String(_, _) => "String",
            Value::Array(_) => "Array",
            Value::HashMap(_) => "HashMap",
            Value::Range(_) => "Range",
            Value::ArgumentExpansion(_) => "ArgumentExpansion",
            Value::ExecResult(_) => "ExecResult",
            Value::Enum(item) => return item.definition.name.clone(),
            Value::Struct(item) => return item.definition.name.clone(),
        }.to_string()
    }

    pub fn from_hash_map(values: HashMap<MapKey, Value>) -> Value
    {
        Value::HashMap(Rc::new(values))
    }

    pub fn from_array(values: Vec<Value>) -> Value
    {
        Value::Array(Rc::new(values))
    }

    pub fn from_argument_expansion(values: Vec<Value>) -> Value
    {
        Value::ArgumentExpansion(Rc::new(values))
    }

    pub fn from_string(s: String) -> Value
    {
        Value::String(s, Executable::No)
    }

    pub fn from_executable_string(s: String) -> Value
    {
        Value::String(s, Executable::Yes)
    }

    pub fn from_status_code(code: Option<i32>) -> Value
    {
        match code
        {
            Some(code) => Value::ExecResult(ExecResult::Value(code as u8)),
            None => Value::ExecResult(ExecResult::Signaled),
        }
    }

    pub fn as_text(&self) -> String
    {
        match self
        {
            Value::None => "()".to_string(),
            Value::Terminal(_) => "Terminal".to_string(),
            Value::Enum(value) => value.to_string(),
            Value::Struct(value) =>
                {
                    let TypeKind::Struct(fields) = &value.definition.kind else { unreachable!(); };
                    format!(
                        "{}({})",
                        value.definition.name,
                        fields
                            .iter()
                            .zip(&value.fields)
                            .map(|(field, value)| format!(
                                "{}: {}",
                                field.name,
                                value.collection_text()
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
            Value::Range(range) => range.to_string(),
            Value::ExecResult(code) => format!("ExecResult({})", match code
                {
                    ExecResult::Value(v) => v.to_string(),
                    ExecResult::Signaled => "Signaled".to_string(),
                }),
            Value::Integer(i) => i.to_string(),
            Value::Float(_, Some(s)) => s.clone(),
            Value::Float(f, None) => f.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::String(s, _) => s.clone(),
            Value::Array(arr) => arr.iter().map(|v| v.as_text()).collect::<Vec<String>>().join(":"),
            Value::ArgumentExpansion(args) => args
                .iter()
                .map(|v| v.as_text())
                .collect::<Vec<String>>()
                .join(":"),
            Value::HashMap(values) =>
                {
                    if values.is_empty() { return "[:]".to_string(); }
                    let mut entries: Vec<_> = values.iter().collect();
                    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
                    format!(
                        "[{}]",
                        entries
                            .into_iter()
                            .map(|(key, value)| format!(
                                "{}: {}",
                                key.to_value().collection_text(),
                                value.collection_text()
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
        }
    }

    fn collection_text(&self) -> String
    {
        match self
        {
            Value::String(text, _) => format!("{:?}", text),
            Value::Array(values) | Value::ArgumentExpansion(values) => format!("[{}]", values.iter()
                .map(Self::collection_text).collect::<Vec<_>>().join(", ")),
            value => value.as_text()
        }
    }

    pub fn as_integer(&self) -> i64
    {
        match self
        {
            Value::None | Value::Terminal(_) => 0,
            Value::HashMap(_) | Value::Range(_) | Value::Enum(_) | Value::Struct(_) => 0,
            Value::ExecResult(code) => match code
                {
                    ExecResult::Value(v) => *v as i64,
                    ExecResult::Signaled => 0,
                },
            Value::Integer(i) => *i,
            Value::Float(f, _) => *f as i64,
            Value::Boolean(b) => if *b { 1 } else { 0 },
            Value::String(s, _) => s.parse::<i64>().unwrap_or(0),
            Value::Array(arr) => arr.iter().map(|v| v.as_integer()).sum(),
            Value::ArgumentExpansion(args) => args.iter().map(|v| v.as_integer()).sum()
        }
    }

    pub fn as_bool(&self) -> bool
    {
        match self
        {
            Value::None => false,
            Value::Enum(_) | Value::Struct(_) | Value::Terminal(_) => true,
            Value::Range(range) => !range.is_empty(),
            Value::ExecResult(ExecResult::Value(code)) => *code == 0,
            Value::ExecResult(ExecResult::Signaled) => false,
            Value::Integer(value) => *value != 0,
            Value::Float(value, _) => *value != 0.0,
            Value::Boolean(value) => *value,
            Value::String(value, _) => !value.is_empty()
                && value != "false" && value.parse::<f64>() != Ok(0.0),
            Value::Array(values) | Value::ArgumentExpansion(values) => !values.is_empty(),
            Value::HashMap(values) => !values.is_empty()
        }
    }

    // Compare language values without string execution flags or float source spelling.
    pub fn equals(&self, other: &Value) -> bool
    {
        match (self, other)
        {
            (Value::None, Value::None) => true,
            (Value::Terminal(left), Value::Terminal(right)) => left == right,
            (Value::Enum(left), Value::Enum(right)) => left == right,
            (Value::Struct(left), Value::Struct(right)) => left.definition.id == right.definition.id
                && left.fields.len() == right.fields.len()
                && left.fields.iter().zip(&right.fields).all(|(left, right)| left.equals(right)),
            (Value::Range(left), Value::Range(right)) => left == right,
            (Value::ExecResult(left), Value::ExecResult(right)) => left == right,
            (Value::Integer(left), Value::Integer(right)) => left == right,
            (Value::Float(left, _), Value::Float(right, _)) => left == right,
            (Value::Integer(integer), Value::Float(float, _))
            | (Value::Float(float, _), Value::Integer(integer)) =>
                {
                    // Avoid rounding large integers through f64, or saturating its cast.
                    float.fract() == 0.0 && *float >= i64::MIN as f64
                        && *float < -(i64::MIN as f64) && *float as i64 == *integer
                },
            (Value::Boolean(left), Value::Boolean(right)) => left == right,
            (Value::String(left, _), Value::String(right, _)) => left == right,
            (Value::HashMap(left), Value::HashMap(right)) => left.len() == right.len()
                && left.iter()
                    .all(|(key, value)| right.get(key).is_some_and(|other| value.equals(other))),
            (Value::Array(left), Value::Array(right))
            | (Value::ArgumentExpansion(left), Value::ArgumentExpansion(right)) =>
                left.len() == right.len()
                    && left.iter().zip(right.iter()).all(|(left, right)| left.equals(right)),
            _ => false
        }
    }

    pub fn arithmetic_error(&self) -> Option<&'static str>
    {
        match self
        {
            Value::Integer(_) | Value::Float(_, _) => None,
            Value::Terminal(_) => Some("Terminals cannot be used in arithmetic"),
            Value::Enum(_) => Some("Enums cannot be used in arithmetic"),
            Value::Struct(_) => Some("Structs cannot be used in arithmetic"),
            Value::String(_, _) => Some("Strings cannot be used in arithmetic"),
            Value::Boolean(_) => Some("Booleans cannot be used in arithmetic"),
            Value::Array(_) | Value::ArgumentExpansion(_) =>
                Some("Arrays cannot be used in arithmetic"),
            Value::HashMap(_) => Some("Maps cannot be used in arithmetic"),
            Value::Range(_) => Some("Ranges cannot be used in arithmetic"),
            Value::ExecResult(_) => Some("Command statuses cannot be used in arithmetic"),
            Value::None => Some("None cannot be used in arithmetic"),
        }
    }

    pub fn arithmetic_float(&self) -> Option<f64>
    {
        let number = match self
            {
                Value::Integer(number) => *number as f64,
                Value::Float(number, _) => *number,
                _ => return None,
            };
        number.is_finite().then_some(number)
    }
}
