
use std::{ cell::RefCell, collections::HashMap, rc::Rc };

use super::{ data::value::Value, text::location::Location };



pub enum Code
{
    Push,
    Execute,
    // Pop a command name; leave its result, or the unresolved string, in last_result.
    TryExecute,
    // Execute last_result only when it is a string marked executable; preserve other values.
    // Boolean(true) rejects arrays in a standalone variable statement whose value is discarded.
    ExecuteIfExecutable,
    MakeExecutable,
    ExitFunction,
    NewVariable,
    SetVariable,
    GetVariable,
    PushResult,
    PopResult,
    CheckResult,
    NewAlias,
    ExportVariable,
    GlobFiles,
    ExpandArray,
    // Stack operations: collect values, read an element, or update a variable's element.
    MakeArray,
    GetArrayElement,
    SetArrayElement,
    ExpandPath,
    InterpolateString,
    InterpolateGlob,
    EnterScope,
    ExitScope,
    MathAdd,
    MathSubtract,
    MathMultiply,
    MathDivide,
    MathModulo,
    CompareEqual,
    CompareNotEqual,
    // Convert last_result in place; these instructions do not touch the value stack.
    ToBoolean,
    BooleanNot,
    // Before linking: label IDs local to this code vector. After linking: direct
    // instruction indexes pointing at JumpTarget. Preserve last_result and the stack.
    // Unconditional branching is reserved for subsequent control-flow constructs.
    Jump,
    JumpIfFalse,
    JumpIfTrue,
    // A labeled landing point during compilation; linking removes its operand.
    JumpTarget
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
