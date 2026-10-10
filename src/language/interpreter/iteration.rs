
use std::{ rc::Rc, collections::hash_map::IntoIter };

use crate::language::{ data::{ map_key::MapKey, range::RangeIterator, value::Value,
                               scoped_variables::ValueReference,
                               methods::{ BoundMethod, MethodDefinition }, types::TypeKind },
                       interpreter::{ Interpreter, InterpreterResult, InterpreterError, ErrorWhat },
                       text::location::Location };


fn iteration_error(location: &Location, message: impl Into<String>) -> InterpreterError
{
    InterpreterError { location: location.clone(), what: ErrorWhat::IterationError(message.into()) }
}


// Native cursors implement the same next_item contract without consuming a Vec
// from the front on every step. User overrides always use normal method dispatch.
enum NativeIteration
{
    Array { values: Rc<Vec<Value>>, index: usize },
    Map(IntoIter<MapKey, Value>),
    Range(RangeIterator),
}

impl NativeIteration
{
    fn new(value: Value, location: &Location) -> InterpreterResult<Self>
    {
        Ok(match value
        {
            Value::Array(values) | Value::ArgumentExpansion(values) =>
                Self::Array { values, index: 0 },
            Value::HashMap(values) => Self::Map(Rc::unwrap_or_clone(values).into_iter()),
            Value::Range(range) =>
                {
                    if range.start.is_none() || range.end.is_none()
                    { return Err(iteration_error(location,
                        "For loops require a range with both bounds")); }
                    Self::Range(range.iter().unwrap())
                },
            _ => return Err(iteration_error(location, "Invalid native iterator receiver")),
        })
    }

    fn next(&mut self) -> Value
    {
        match self
        {
            Self::Array { values, index } =>
                {
                    let value = values.get(*index).cloned().unwrap_or(Value::None);
                    *index += 1;
                    value
                },
            Self::Map(values) => values.next().map(|(key, value)|
                Value::from_array(vec![key.to_value(), value])).unwrap_or(Value::None),
            Self::Range(values) => values.next().map(Value::Integer).unwrap_or(Value::None),
        }
    }
}


pub(super) struct Iteration
{
    method: BoundMethod,
    native: Option<NativeIteration>,
    ended: bool,
}

impl Iteration
{
    pub fn new(interpreter: &Interpreter, location: &Location, value: Value, snapshot: &Value)
        -> InterpreterResult<Self>
    {
        let reference = ValueReference::temporary(value.clone());
        let method = interpreter.resolve_method(&value, reference, "next_item", snapshot)
            .ok_or_else(|| iteration_error(location,
                format!("Type '{}' has no next_item method", value.type_name())))?;
        let (return_type, native) = match &method.definition
            {
                MethodDefinition::Builtin(definition) =>
                    (Some(definition.return_type), definition.native_iterator),
                MethodDefinition::User(function) =>
                    {
                        if function.variadic || function.arguments.len() != 1
                        { return Err(iteration_error(location,
                            "next_item must accept no arguments beyond its receiver")); }
                        (function.return_type, false)
                    },
            };
        if !return_type.is_some_and(|id|
            matches!(interpreter.scope().types.get(id).kind, TypeKind::Optional(_)))
        { return Err(iteration_error(location, "next_item must declare a return type T | ()")); }
        let native = if native
            { Some(NativeIteration::new(value.underlying().clone(), location)?) } else { None };
        Ok(Self { method, native, ended: false })
    }

    pub fn next(&mut self, interpreter: &mut Interpreter, location: &Location)
        -> InterpreterResult<Option<Value>>
    {
        if self.ended { return Ok(None); }
        let value = if let Some(native) = &mut self.native { native.next() }
            else
            {
                interpreter.execute_method(location, &self.method, &[])?;
                if interpreter.halted { self.ended = true; return Ok(None); }
                interpreter.last_result.take().unwrap_or(Value::None)
            };
        if matches!(value, Value::None) { self.ended = true; return Ok(None); }
        Ok(Some(value))
    }
}
