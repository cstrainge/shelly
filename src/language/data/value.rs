
#[derive(Clone, Debug, PartialEq)]
pub enum Value
{
    Integer(i64),
    Float(f64, Option<String>),
    Boolean(bool),
    String(String),
    Array(Vec<Value>),
    ArgumentExpansion(Vec<Value>)
}


impl Value
{
    pub fn as_text(&self) -> String
    {
        match self
        {
            Value::Integer(i) => i.to_string(),
            Value::Float(_, Some(s)) => s.clone(),
            Value::Float(f, None) => f.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::String(s) => s.clone(),
            Value::Array(arr) => arr.iter().map(|v| v.as_text()).collect::<Vec<String>>().join(":"),
            Value::ArgumentExpansion(args) => args.iter().map(|v| v.as_text()).collect::<Vec<String>>().join(":"),
        }
    }

    pub fn as_integer(&self) -> i64
    {
        match self
        {
            Value::Integer(i) => *i,
            Value::Float(f, _) => *f as i64,
            Value::Boolean(b) => if *b { 1 } else { 0 },
            Value::String(s) => s.parse::<i64>().unwrap_or(0),
            Value::Array(arr) => arr.iter().map(|v| v.as_integer()).sum(),
            Value::ArgumentExpansion(args) => args.iter().map(|v| v.as_integer()).sum()
        }
    }
}
