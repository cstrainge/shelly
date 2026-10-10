
mod array;
mod string;
mod iteration;

use std::rc::Rc;

use crate::language::{ bytecode::FunctionRef,
                       data::{ value::Value, scoped_variables::ValueReference,
                               types::{ TypeId, TypeKind, TypeRegistry },
                               methods::{ iteration::next_item,
                                          array::{ sort, zip, count },
                                          string::{ chars, contains, starts_with, ends_with,
                                                    replace, split, trim, trim_start, trim_end } } } };

pub fn method_key(receiver: TypeId, name: &str) -> String
{
    format!("\0method:{}:{}", receiver.0, name)
}

pub type MethodBody = fn(&mut Value, &[Value]) -> Result<Value, String>;

#[derive(Debug)]
pub struct BuiltinMethod
{
    pub name: &'static str,
    pub mutates_receiver: bool,
    pub native_iterator: bool,
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
    let string = registry.builtin_id("String").unwrap();
    let boolean = registry.builtin_id("Boolean").unwrap();
    for (name, argument_count, return_type, body) in [
            ("chars", 0, array, chars as MethodBody),
            ("contains", 1, boolean, contains as MethodBody),
            ("starts_with", 1, boolean, starts_with as MethodBody),
            ("ends_with", 1, boolean, ends_with as MethodBody),
            ("replace", 2, string, replace as MethodBody),
            ("split", 1, array, split as MethodBody),
            ("trim", 0, string, trim as MethodBody),
            ("trim_start", 0, string, trim_start as MethodBody),
            ("trim_end", 0, string, trim_end as MethodBody),
        ]
    {
        registry.register_method(string, BuiltinMethod
            {
                name, argument_count, return_type, body,
                mutates_receiver: false, native_iterator: false,
            });
    }
    for receiver in ["Array", "ArgumentExpansion"]
    {
        let receiver = registry.builtin_id(receiver).unwrap();
        for (name, argument_count, return_type, body) in [
                ("sort", 0, array, sort as MethodBody),
                ("zip", 1, array, zip as MethodBody),
                ("count", 0, integer, count as MethodBody),
            ]
        {
            registry.register_method(receiver, BuiltinMethod
                {
                    name, argument_count, return_type, body,
                    mutates_receiver: false, native_iterator: false,
                });
        }
    }
    let any = registry.builtin_id("any").unwrap();
    let pair = registry.intern(TypeKind::FixedArray(vec![any, any]));
    for (name, item) in [("Array", any), ("ArgumentExpansion", any),
                         ("HashMap", pair), ("Range", integer)]
    {
        let receiver = registry.builtin_id(name).unwrap();
        let return_type = registry.intern(TypeKind::Optional(item));
        registry.register_method(receiver, BuiltinMethod
            {
                name: "next_item", argument_count: 0, return_type, body: next_item,
                mutates_receiver: true, native_iterator: true,
            });
    }
}
