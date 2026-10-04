
use super::{ data::value::Value, text::location::Location };



pub enum Code
{
    Push,
    Execute,
    NewVariable,
    SetVariable,
    GetVariable,
    GlobFiles,
    ExpandArray
}


pub struct Instruction
{
    pub location: Option<Location>,
    pub code: Code,
    pub operand: Option<Value>
}
