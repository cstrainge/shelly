
use std::{ collections::HashMap, rc::Rc };

use crate::language::{ interpreter::BuiltIn, data::types::TypeId };


#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NativeVisibility
{
    Hidden,
    Visible,
}


pub struct NativeFunction
{
    pub name: &'static str,
    pub visibility: NativeVisibility,
    pub body: BuiltIn<'static>,
}


impl NativeFunction
{
    pub fn new(name: &'static str, visibility: NativeVisibility, body: BuiltIn<'static>) -> Rc<Self>
    {
        Rc::new(Self { name, visibility, body })
    }
}


pub type NativeFunctions = HashMap<&'static str, Rc<NativeFunction>>;


#[derive(Clone)]
pub struct NativeType
{
    pub id: TypeId,
    pub visibility: NativeVisibility,
}
