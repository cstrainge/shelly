
#[derive(Clone, Debug, PartialEq)]
pub enum ExecResult
{
    Value(u8),
    Signaled
}


#[derive(Clone, Debug, PartialEq)]
pub enum Executable
{
    Yes,
    No
}


#[derive(Clone, Debug, PartialEq)]
pub enum Value
{
    None,
    ExecResult(ExecResult),
    Integer(i64),
    Float(f64, Option<String>),
    Boolean(bool),
    String(String, Executable),
    Array(Vec<Value>),
    ArgumentExpansion(Vec<Value>)
}


impl Value
{
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
            Value::ArgumentExpansion(args) => args.iter().map(|v| v.as_text()).collect::<Vec<String>>().join(":"),
        }
    }

    pub fn as_integer(&self) -> i64
    {
        match self
        {
            Value::None => 0,
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

    pub fn checked_integer(&self) -> Option<i64>
    {
        match self
        {
            Value::Array(values) | Value::ArgumentExpansion(values) => values.iter()
                .try_fold(0i64, |sum, value| sum.checked_add(value.checked_integer()?)),
            _ => Some(self.as_integer())
        }
    }
}
