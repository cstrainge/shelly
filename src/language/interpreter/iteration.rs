
use std::{ rc::Rc, collections::hash_map::IntoIter };

use crate::language::data::{ map_key::MapKey, range::RangeIterator, value::Value };

// Iterator state belongs to one execution frame, never to a user-visible Value.
// Arrays retain their shared snapshot; maps own an entry iterator. Range iteration
// stays lazy, including ranges too large to materialize as argument expansions.
pub(super) enum Iteration
{
    Array { values: Rc<Vec<Value>>, index: usize },
    Map(IntoIter<MapKey, Value>),
    Range(RangeIterator)
}


impl Iteration
{
    pub fn new(value: Value, bindings: i64) -> Result<Self, &'static str>
    {
        match (value, bindings)
        {
            (Value::Array(values) | Value::ArgumentExpansion(values), 1) =>
                Ok(Self::Array { values, index: 0 }),
            (Value::HashMap(values), 2) => Ok(Self::Map(Rc::unwrap_or_clone(values).into_iter())),
            (Value::Range(range), 1) =>
                {
                    if range.start.is_none() || range.end.is_none()
                    {
                        return Err("For loops require a range with both bounds");
                    }
                    Ok(Self::Range(range.iter().unwrap()))
                },
            (Value::Array(_) | Value::ArgumentExpansion(_) | Value::Range(_), _) =>
                Err("Arrays and ranges require one loop binding"),
            (Value::HashMap(_), _) => Err("Hash maps require two loop bindings: key, value"),
            _ => Err("For loops require an array, hash map, or range")
        }
    }

    pub fn next(&mut self) -> Option<(Value, Option<Value>)>
    {
        match self
        {
            Self::Array { values, index } =>
                {
                    let value = values.get(*index)?.clone();
                    *index += 1;
                    Some((value, None))
                },
            Self::Map(values) => values.next().map(|(key, value)| (key.to_value(), Some(value))),
            Self::Range(values) => values.next().map(|value| (Value::Integer(value), None))
        }
    }
}
