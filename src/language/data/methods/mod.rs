
mod array;

use std::rc::Rc;

use crate::language::{ bytecode::FunctionRef,
                       data::{ value::Value, scoped_variables::ValueReference,
                               types::{ TypeId, TypeRegistry },
                               methods::array::{ sort, zip, count } } };

pub fn method_key(receiver: TypeId, name: &str) -> String
{
    format!("\0method:{}:{}", receiver.0, name)
}

pub type MethodBody = fn(&Value, &[Value]) -> Result<Value, String>;

#[derive(Debug)]
pub struct BuiltinMethod
{
    pub name: &'static str,
    pub argument_count: usize,
    pub return_type: TypeId,
    pub body: MethodBody,
}

pub enum MethodDefinition
{
    Builtin(Rc<BuiltinMethod>),
    User(FunctionRef),
}

pub struct BoundMethod
{
    pub receiver: ValueReference,
    pub name: String,
    pub definition: MethodDefinition,
}

pub fn register_methods(registry: &mut TypeRegistry)
{
    let array = registry.builtin_id("Array").unwrap();
    let integer = registry.builtin_id("Integer").unwrap();
    for receiver in ["Array", "ArgumentExpansion"]
    {
        let receiver = registry.builtin_id(receiver).unwrap();
        for (name, argument_count, return_type, body) in [
                ("sort", 0, array, sort as MethodBody),
                ("zip", 1, array, zip as MethodBody),
                ("count", 0, integer, count as MethodBody),
            ]
        {
            registry.register_method(receiver,
                BuiltinMethod { name, argument_count, return_type, body });
        }
    }
}
