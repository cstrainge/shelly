
use std::{ cell::RefCell, collections::HashMap, rc::Rc };

use super::{ data::value::Value, text::location::Location };



pub enum Code
{
    Push,
    Execute,
    NewVariable,
    SetVariable,
    GetVariable,
    ExportVariable,
    GlobFiles,
    ExpandArray,
    InterpolateString,
    EnterScope,
    ExitScope,
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


pub struct Function
{
    pub functions: FunctionBlockRef,

    pub arguments: Vec<String>,
    pub code: Vec<Instruction>
}


pub struct FunctionBlock
{
    pub parent: Option<FunctionBlockRef>,
    pub functions: HashMap<String, FunctionRef>
}


pub type FunctionBlockRef = Rc<RefCell<FunctionBlock>>;

pub type FunctionRef = Rc<Function>;
