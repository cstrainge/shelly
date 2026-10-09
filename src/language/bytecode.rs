
use std::{ cell::RefCell, collections::HashMap, rc::Rc };

use super::{ data::value::Value, text::location::Location };



pub enum Code
{
    Push,
    Execute,
    // Pop a command name; leave its result, or the unresolved string, in last_result.
    TryExecute,
    // Execute last_result only when it is a string marked executable; preserve other values.
    // Boolean(true) rejects collections in a standalone variable statement whose value is discarded.
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
    MakeHashMap,
    // Operand bits indicate a supplied start (1), end (2), and inclusive end (4).
    MakeRange,
    // Start consumes the iterable; operand is the binding count. Next pushes
    // yielded bindings and sets a boolean result for the following linked jump.
    StartIteration,
    NextIteration,
    // Pop a yielded value into a fresh scoped binding without assignment coercions.
    BindIteration,
    EndIteration,
    // Operand is [continue target, break target]: labels before linking, numeric
    // JumpTarget indexes afterwards. Enter pushes a loop frame; Exit pops it.
    EnterLoop,
    ExitLoop,
    Break,
    Continue,
    GetElement,
    SetElement,
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
    // Unconditional jumps also form loop back edges.
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
