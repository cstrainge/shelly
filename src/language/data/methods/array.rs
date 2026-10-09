
use std::cmp::Ordering;

use crate::language::data::value::Value;



fn elements(receiver: &Value) -> Result<&[Value], String>
{
    match receiver
    {
        Value::Array(values) | Value::ArgumentExpansion(values) => Ok(values),
        _ => Err("Array methods require an array or argument expansion".to_string()),
    }
}

pub(super) fn count(receiver: &Value, _: &[Value]) -> Result<Value, String>
{
    let length = i64::try_from(elements(receiver)?.len())
        .map_err(|_| "Array length exceeds Integer range".to_string())?;
    Ok(Value::Integer(length))
}

pub(super) fn zip(receiver: &Value, arguments: &[Value]) -> Result<Value, String>
{
    let left = elements(receiver)?;
    let right = elements(&arguments[0])?;
    Ok(Value::from_array(left.iter().zip(right).map(|(left, right)|
        Value::from_array(vec![left.clone(), right.clone()])).collect()))
}

// Compare mixed numeric types without rounding a large integer to f64 first.
fn integer_float(integer: i64, float: f64) -> Ordering
{
    if float >= 9_223_372_036_854_775_808.0 { return Ordering::Less; }
    if float < i64::MIN as f64 { return Ordering::Greater; }
    integer.cmp(&(float as i64)).then_with(||
        {
            if float.fract() > 0.0 { Ordering::Less }
            else if float.fract() < 0.0 { Ordering::Greater }
            else { Ordering::Equal }
        })
}

fn compare(left: &Value, right: &Value) -> Option<Ordering>
{
    match (left, right)
    {
        (Value::Integer(left), Value::Integer(right)) => Some(left.cmp(right)),
        (Value::Float(left, _), Value::Float(right, _))
            if left.is_finite() && right.is_finite() => left.partial_cmp(right),
        (Value::Integer(left), Value::Float(right, _)) if right.is_finite() =>
            Some(integer_float(*left, *right)),
        (Value::Float(left, _), Value::Integer(right)) if left.is_finite() =>
            Some(integer_float(*right, *left).reverse()),
        (Value::String(left, _), Value::String(right, _)) => Some(left.cmp(right)),
        (Value::Boolean(left), Value::Boolean(right)) => Some(left.cmp(right)),
        (Value::Enum(left), Value::Enum(right)) if left.definition.id == right.definition.id =>
            Some(left.variant.cmp(&right.variant)),
        _ => None,
    }
}

pub(super) fn sort(receiver: &Value, _: &[Value]) -> Result<Value, String>
{
    let values = elements(receiver)?;
    // Validate even singleton inputs. Comparison must be total within the chosen category.
    if let Some(first) = values.first()
    {
        for value in values
        {
            if compare(first, value).is_none()
            {
                return Err("sort requires numbers, strings, booleans, or variants of one enum; \
                    values must be mutually comparable and numbers finite".to_string());
            }
        }
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| compare(left, right).unwrap());
    Ok(Value::from_array(sorted))
}
