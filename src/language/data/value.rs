
#[derive(Clone, Debug, PartialEq)]
pub enum Value
{
    Integer(i64),
    Float(f64, Option<String>),
    Boolean(bool),
    String(String)
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
            Value::String(s) => s.clone()
        }
    }

    pub fn as_int(&self) -> i64
    {
        match self
        {
            Value::Integer(i) => *i,
            Value::Float(f, _) => *f as i64,
            Value::Boolean(b) => if *b { 1 } else { 0 },
            Value::String(s) => s.parse::<i64>().unwrap_or(0)
        }
    }
}
