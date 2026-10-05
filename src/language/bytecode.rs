
use super::{ data::value::Value, text::location::Location };



pub enum Code
{
    Push,
    Execute,
    NewVariable,
    SetVariable,
    GetVariable,
    GlobFiles,
    ExpandArray,
    InterpolateString,
    MathAdd,
    MathSubtract,
    MathMultiply,
    MathDivide,
    MathModulo
}


pub struct Instruction
{
    pub location: Option<Location>,
    pub code: Code,
    pub operand: Option<Value>
}
