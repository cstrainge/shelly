
use std::{ cell::RefCell, collections::HashMap, fmt::{ self, Display, Formatter }, rc::Rc };


use crate::language::{ ast::*,
                       bytecode::{ Code, Instruction, Function, FunctionBlock, FunctionBlockRef },
                       data::value::Value,
                       text::location::Location,
                       parser::ParserError };



#[derive(PartialEq, Eq)]
pub enum CompileTarget
{
    Toplevel,
    Function
}


pub enum ErrorWhat
{
    ParserError(ParserError),
    InvalidJump(String),
}


pub struct CompileError
{
    pub location: Option<Location>,
    pub what: ErrorWhat
}


impl Display for CompileError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match &self.what
        {
            ErrorWhat::InvalidJump(message) =>
                {
                    if let Some(location) = &self.location
                    {
                        write!(f, "Link error: {}: {}", location, message)
                    }
                    else
                    {
                        write!(f, "Link error: {}", message)
                    }
                },

            ErrorWhat::ParserError(error) =>
                {
                    if let Some(location) = &self.location
                    {
                        write!(f, "Parser error: {}: {}", location, error)
                    }
                    else
                    {
                        write!(f, "Parser error: {}", error)
                    }
                }
        }
    }
}


impl From<ParserError> for CompileError
{
    fn from(error: ParserError) -> Self
    {
        CompileError
        {
            location: None,
            what: ErrorWhat::ParserError(error)
        }
    }
}


pub type CompileResult<T> = Result<T, CompileError>;


fn compile_expression(instructions: &mut Vec<Instruction>,
                      function_block: &FunctionBlockRef,
                      expression: &AstExpression) -> CompileResult<()>
{
    match &expression.kind
    {
        AstExpressionKind::IfExpression(conditional) =>
            {
                compile_if_expression(instructions, function_block, conditional)?;
                return Ok(());
            },

        AstExpressionKind::Grouped(inner) =>
            {
                compile_expression(instructions, function_block, inner)?;

                // Calls already execute, and nested groups handle their own value. Only a
                // variable or string literal can supply an unevaluated executable reference.
                if matches!(&inner.kind,
                    AstExpressionKind::Variable(_)
                    | AstExpressionKind::ExecutableReference(_)
                    | AstExpressionKind::Literal(AstLiteral { value: Value::String(_, _), .. }))
                {
                    instructions.push(Instruction
                        {
                            location: Some(expression.location.clone()),
                            code: Code::ExecuteIfExecutable,
                            operand: None
                        });
                }

                return Ok(());
            },

        AstExpressionKind::Execute(execute) =>
            {
                compile_execute_statement(instructions, function_block, execute)?;
                return Ok(());
            },

        AstExpressionKind::ExecutableReference(inner) =>
            {
                compile_expression(instructions, function_block, inner)?;
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::MakeExecutable,
                        operand: None
                    });
                return Ok(());
            },

        AstExpressionKind::TryExecute(command) =>
            {
                compile_expression(instructions, function_block, command)?;
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::PushResult,
                        operand: None
                    });
                instructions.push(Instruction
                    {
                        location: None,
                        code: Code::TryExecute,
                        operand: None
                    });
                return Ok(());
            },

        AstExpressionKind::Symbol(symbol) =>
            {
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::Push,
                        operand: Some(Value::from_string(symbol.name.clone()))
                    });
            },

        AstExpressionKind::Literal(value) =>
            {
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::Push,
                        operand: Some(value.value.clone())
                    });
            },

        AstExpressionKind::Variable(variable) =>
            {
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::GetVariable,
                        operand: Some(Value::from_string(variable.name.clone()))
                    });
            }

        AstExpressionKind::VariableSplat(variable) =>
            {
                instructions.push(Instruction
                        {
                            location: Some(expression.location.clone()),
                            code: Code::GetVariable,
                            operand: Some(Value::from_string(variable.name.clone()))
                        });

                    instructions.push(Instruction
                        {
                            location: Some(expression.location.clone()),
                            code: Code::ExpandArray,
                            operand: None
                        });
            },

        AstExpressionKind::BooleanNot(inner) =>
            {
                compile_expression(instructions, function_block, inner)?;
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::BooleanNot,
                        operand: None
                    });
                return Ok(());
            },

        AstExpressionKind::BooleanExpression(operator, lhs, rhs) =>
            {
                compile_expression(instructions, function_block, lhs)?;

                if matches!(operator, AstBooleanOperator::And | AstBooleanOperator::Or)
                {
                    instructions.push(Instruction
                        {
                            location: Some(expression.location.clone()),
                            code: Code::ToBoolean,
                            operand: None
                        });

                    // The jump's current position supplies a unique label ID. It is not
                    // a destination index: the matching JumpTarget defines that position.
                    let target = Value::Integer(instructions.len() as i64);
                    instructions.push(Instruction
                        {
                            location: Some(expression.location.clone()),
                            code: if matches!(operator, AstBooleanOperator::And)
                                { Code::JumpIfFalse } else { Code::JumpIfTrue },
                            operand: Some(target.clone())
                        });
                    compile_expression(instructions, function_block, rhs)?;
                    instructions.push(Instruction
                        {
                            location: Some(expression.location.clone()),
                            code: Code::ToBoolean,
                            operand: None
                        });
                    instructions.push(Instruction
                        {
                            location: Some(expression.location.clone()),
                            code: Code::JumpTarget,
                            operand: Some(target)
                        });
                    return Ok(());
                }

                instructions.push(Instruction { location: None, code: Code::PushResult, operand: None });
                compile_expression(instructions, function_block, rhs)?;
                instructions.push(Instruction { location: None, code: Code::PushResult, operand: None });
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: if matches!(operator, AstBooleanOperator::Equal)
                            { Code::CompareEqual } else { Code::CompareNotEqual },
                        operand: None
                    });
            },

        AstExpressionKind::MathExpression(operator, lhs, rhs) =>
            {
                compile_expression(instructions, function_block, lhs)?;

                instructions.push(Instruction
                    {
                        location: None,
                        code: Code::PushResult,
                        operand: None
                    });

                compile_expression(instructions, function_block, rhs)?;

                instructions.push(Instruction
                    {
                        location: None,
                        code: Code::PushResult,
                        operand: None
                    });

                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: match operator
                            {
                                AstMathOperator::Add      => Code::MathAdd,
                                AstMathOperator::Subtract => Code::MathSubtract,
                                AstMathOperator::Multiply => Code::MathMultiply,
                                AstMathOperator::Divide   => Code::MathDivide,
                                AstMathOperator::Modulo   => Code::MathModulo,
                            },
                        operand: None
                    });
            }
    }

    if let Some(string_flag) = &expression.string_flag
    {
        if let AstStringFlag::Interpolated(escaped_dollars) = string_flag
        {
            instructions.push(Instruction
                {
                    location: Some(expression.location.clone()),
                    code: if matches!(&expression.kind, AstExpressionKind::Symbol(symbol) if symbol.is_glob())
                        { Code::InterpolateGlob } else { Code::InterpolateString },
                    operand: Some(Value::Array(escaped_dollars.iter()
                        .map(|offset| Value::Integer(*offset as i64)).collect()))
                });
        }
    }

    // Only an unquoted source word can request home expansion. Variable values and quoted literals
    // retain their spelling. GlobFiles expands its own prefix.
    if    let AstExpressionKind::Symbol(symbol) = &expression.kind
       && (symbol.name == "~" || symbol.name.starts_with("~/"))
       && !symbol.is_glob()
    {
        instructions.push(Instruction
            {
                location: Some(expression.location.clone()),
                code: Code::ExpandPath,
                operand: None
            });
    }

    if let AstExpressionKind::Symbol(symbol) = &expression.kind && symbol.is_glob()
    {
        instructions.push(Instruction
            {
                location: Some(expression.location.clone()),
                code: Code::GlobFiles,
                operand: Some(Value::Boolean(symbol.name.starts_with('~')))
            });
    }

    instructions.push(Instruction
        {
            location: None,
            code: Code::PopResult,
            operand: None
        });
    Ok(())
}


fn compile_return_statement(instructions: &mut Vec<Instruction>,
                            function_block: &FunctionBlockRef,
                            return_statement: &AstReturnStatement) -> CompileResult<()>
{
    if let Some(expression) = &return_statement.expression
    {
        compile_expression(instructions, function_block, expression)?;
    }
    else
    {
        instructions.push(Instruction
            {
                location: Some(return_statement.location.clone()),
                code: Code::Push,
                operand: Some(Value::None)
            });
        instructions.push(Instruction
            {
                location: None,
                code: Code::PopResult,
                operand: None
            });
    }

    instructions.push(Instruction
        {
            location: Some(return_statement.location.clone()),
            code: Code::ExitFunction,
            operand: None
        });
    Ok(())
}


fn compile_let_statement(instructions: &mut Vec<Instruction>,
                         function_block: &FunctionBlockRef,
                         let_statement: &AstLetStatement) -> CompileResult<()>
{
    // Evaluate before changing the binding so self-reference and failed initializers are safe.
    compile_expression(instructions, function_block, &let_statement.expression)?;
    instructions.push(Instruction
        {
            location: None,
            code: Code::PushResult,
            operand: None
        });

    instructions.push(Instruction
        {
            location: Some(let_statement.location.clone()),
            code: Code::NewVariable,
            operand: Some(Value::from_string(let_statement.identifier.clone()))
        });

    if let_statement.export_flag == AstExportFlag::Exported
    {
        instructions.push(Instruction
            {
                location: None,
                code: Code::ExportVariable,
                operand: Some(Value::from_string(let_statement.identifier.clone()))
            });
    }

    instructions.push(Instruction
        {
            location: Some(let_statement.location.clone()),
            code: Code::SetVariable,
            operand: Some(Value::from_string(let_statement.identifier.clone()))
        });
    Ok(())
}

fn compile_set_statement(instructions: &mut Vec<Instruction>,
                         function_block: &FunctionBlockRef,
                         set_statement: &AstSetStatement) -> CompileResult<()>
{
    compile_expression(instructions, function_block, &set_statement.expression)?;

    instructions.push(Instruction
        {
            location: None,
            code: Code::PushResult,
            operand: None
        });

    instructions.push(Instruction
        {
            location: Some(set_statement.location.clone()),
            code: Code::SetVariable,
            operand: Some(Value::from_string(set_statement.identifier.clone()))
        });
    Ok(())
}


fn compile_alias_statement(instructions: &mut Vec<Instruction>, alias_statement: &AstAliasStatement)
{
    // Alias targets are unquoted symbols. Arguments retain their quoting and
    // otherwise remain stored values, without interpolation or glob expansion.
    instructions.push(Instruction
        {
            location: Some(alias_statement.location.clone()),
            code: Code::Push,
            operand: Some(Value::from_executable_string(alias_statement.target.clone()))
        });
    instructions.push(Instruction
        {
            location: Some(alias_statement.location.clone()),
            code: Code::ExpandPath,
            operand: None
        });

    for argument in &alias_statement.arguments
    {
        instructions.push(Instruction
            {
                location: Some(alias_statement.location.clone()),
                code: Code::Push,
                operand: Some(argument.value.clone())
            });
        if argument.expand_path
        {
            instructions.push(Instruction
                {
                    location: Some(alias_statement.location.clone()),
                    code: Code::ExpandPath,
                    operand: None
                });
        }
    }

    instructions.push(Instruction
        {
            location: Some(alias_statement.location.clone()),
            code: Code::NewAlias,
            operand: Some(Value::Array(vec![Value::from_string(alias_statement.alias.clone()),
                                           Value::Integer(alias_statement.arguments.len() as i64)]))
        });
}


fn compile_execute_statement(instructions: &mut Vec<Instruction>,
                             function_block: &FunctionBlockRef,
                             execute_statement: &AstExecuteStatement) -> CompileResult<()>
{
    instructions.push(Instruction
        {
            location: None,
            code: Code::Push,
            operand: Some(Value::from_executable_string(execute_statement.executable_name.clone()))
        });

    if execute_statement.expand_path
    {
        instructions.push(Instruction
            {
                location: Some(execute_statement.location.clone()),
                code: Code::ExpandPath,
                operand: None
            });
    }

    for argument in &execute_statement.arguments
    {
        compile_expression(instructions, function_block, argument)?;

        match &argument.kind
        {
            AstExpressionKind::Symbol(symbol) if !symbol.is_glob() =>
                {
                    instructions.push(Instruction
                        {
                            location: Some(argument.location.clone()),
                            code: Code::PushResult,
                            operand: None
                        });
                    instructions.push(Instruction
                        {
                            location: None,
                            code: Code::TryExecute,
                            operand: None
                        });
                },

            AstExpressionKind::Variable(_) =>
                {
                    instructions.push(Instruction
                        {
                            location: Some(argument.location.clone()),
                            code: Code::ExecuteIfExecutable,
                            operand: None
                        });
                },

            // Backtick references are literals: pass their values without calling them.
            // Grouped expressions and explicit calls already supplied their result.
            _ => {}
        }

        instructions.push(Instruction
            {
                location: None,
                code: Code::PushResult,
                operand: None
            });
    }

    instructions.push(Instruction
        {
            location: Some(execute_statement.location.clone()),
            code: Code::Execute,
            operand: Some(Value::Integer(execute_statement.arguments.len() as i64))
        });
    Ok(())
}


fn compile_function_definition(parent_block: &FunctionBlockRef,
                               function_statement: &AstFunctionStatement) -> CompileResult<()>
{
    let function_block = Rc::new(RefCell::new(FunctionBlock
        {
            parent: Some(parent_block.clone()),
            functions: HashMap::new()
        }));

    let instructions = compile_ast(&function_block,
                                   &function_statement.body,
                                   CompileTarget::Function)?;

    let new_function = Rc::new(Function
        {
            functions: function_block,
            arguments: function_statement.parameters.clone(),
            code: instructions
        });

    parent_block.borrow_mut().functions.insert(function_statement.name.clone(), new_function);

    Ok(())
}


// Remove redundant round trips through last_result before jump indexes are linked.
// JumpTarget instructions are barriers: a jump cannot land inside a removed pair.
fn optimize_instructions(instructions: &mut Vec<Instruction>)
{
    let mut optimized = Vec::with_capacity(instructions.len());
    let mut input = instructions.drain(..).peekable();
    let mut location = None;

    while let Some(mut instruction) = input.next()
    {
        if matches!(instruction.code, Code::PopResult)
            && input.peek().is_some_and(|next| matches!(next.code, Code::PushResult))
        {
            // Carry any removed source location to the next surviving instruction,
            // unless that instruction already establishes its own location.
            location = input.next().unwrap().location.or(instruction.location).or(location);
            continue;
        }

        instruction.location = instruction.location.or(location.take());
        optimized.push(instruction);
    }

    drop(input);
    *instructions = optimized;
}


// Prove emptiness only along straight-line execution. Entry state and incoming
// jumps are unknown; calls can replace last_result even without a PopResult.
fn remove_empty_result_checks(instructions: &mut Vec<Instruction>)
{
    let mut result_is_empty = false;
    let mut location = None;

    instructions.retain_mut(|instruction|
        {
            if matches!(instruction.code, Code::CheckResult) && result_is_empty
            {
                location = instruction.location.clone().or(location.take());
                return false;
            }

            instruction.location = instruction.location.take().or(location.take());
            result_is_empty = match instruction.code
                {
                    // On successful continuation, both instructions consume the result.
                    Code::PushResult | Code::CheckResult => true,

                    Code::PopResult | Code::Execute | Code::TryExecute
                    | Code::ExecuteIfExecutable | Code::MakeExecutable
                    | Code::ToBoolean | Code::BooleanNot => false,

                    // Do not carry a proof across control-flow boundaries.
                    Code::Jump | Code::JumpIfFalse | Code::JumpIfTrue
                    | Code::JumpTarget | Code::ExitFunction => false,

                    // These instructions operate on the value stack or other VM state.
                    Code::Push | Code::NewVariable | Code::SetVariable | Code::GetVariable
                    | Code::NewAlias | Code::ExportVariable | Code::GlobFiles
                    | Code::ExpandArray | Code::ExpandPath | Code::InterpolateString
                    | Code::InterpolateGlob | Code::EnterScope | Code::ExitScope
                    | Code::MathAdd | Code::MathSubtract | Code::MathMultiply
                    | Code::MathDivide | Code::MathModulo | Code::CompareEqual
                    | Code::CompareNotEqual => result_is_empty
                };
            true
        });
}


// Link one complete code vector after optimization. Function bodies are linked separately.
// Resolve every label before mutating operands, so missing/duplicate labels cannot
// leave a partially linked result. JumpTarget instructions remain as landing points.
fn link_instructions(instructions: &mut [Instruction]) -> CompileResult<()>
{
    let label = |instruction: &Instruction| -> CompileResult<i64>
        {
            match instruction.operand
            {
                Some(Value::Integer(label)) if label >= 0 => Ok(label),
                _ => Err(CompileError
                    {
                        location: instruction.location.clone(),
                        what: ErrorWhat::InvalidJump("Expected a nonnegative label ID".to_string())
                    })
            }
        };
    let mut targets = HashMap::new();
    for (index, instruction) in instructions.iter().enumerate()
    {
        if matches!(instruction.code, Code::JumpTarget)
        {
            let id = label(instruction)?;
            if targets.insert(id, index).is_some()
            {
                return Err(CompileError
                    {
                        location: instruction.location.clone(),
                        what: ErrorWhat::InvalidJump(format!("Duplicate jump label {}", id))
                    });
            }
        }
    }

    let mut resolved = Vec::new();
    for (index, instruction) in instructions.iter().enumerate()
    {
        if matches!(instruction.code, Code::Jump | Code::JumpIfFalse | Code::JumpIfTrue)
        {
            let id = label(instruction)?;
            let target = targets.get(&id).ok_or_else(|| CompileError
                {
                    location: instruction.location.clone(),
                    what: ErrorWhat::InvalidJump(format!("Missing jump label {}", id))
                })?;
            resolved.push((index, *target));
        }
    }
    for (index, target) in resolved
    {
        instructions[index].operand = Some(Value::Integer(target as i64));
    }
    for instruction in instructions
    {
        if matches!(instruction.code, Code::JumpTarget) { instruction.operand = None; }
    }
    Ok(())
}


fn compile_block(instructions: &mut Vec<Instruction>,
                  function_block: &FunctionBlockRef,
                  block: &AstBlockStatement,
                  target: CompileTarget) -> CompileResult<()>
{
    instructions.push(Instruction
        {
            location: Some(block.location.clone()),
            code: Code::EnterScope,
            operand: None
        });
    compile_statements(instructions, function_block, &block.body, target)?;
    instructions.push(Instruction
        {
            location: Some(block.location.clone()),
            code: Code::ExitScope,
            operand: None
        });
    Ok(())
}


fn compile_if_expression(instructions: &mut Vec<Instruction>,
                          function_block: &FunctionBlockRef,
                          conditional: &AstIfExpression) -> CompileResult<()>
{
    let mut end_jumps = Vec::new();

    for branch in &conditional.branches
    {
        let location = Some(branch.condition.location.clone());
        compile_expression(instructions, function_block, &branch.condition)?;
        instructions.push(Instruction { location: location.clone(), code: Code::ToBoolean, operand: None });
        let next_branch = Value::Integer(instructions.len() as i64);
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::JumpIfFalse,
                operand: Some(next_branch.clone())
            });
        // Consume the condition on both paths. Only the selected block supplies
        // the expression's result; conditions must not leak into empty branches.
        instructions.push(Instruction { location: location.clone(), code: Code::CheckResult, operand: None });
        compile_block(instructions, function_block, &branch.body, CompileTarget::Function)?;
        end_jumps.push(instructions.len());
        instructions.push(Instruction { location: location.clone(), code: Code::Jump, operand: None });
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::JumpTarget,
                operand: Some(next_branch)
            });
        instructions.push(Instruction { location, code: Code::CheckResult, operand: None });
    }

    if let Some(block) = &conditional.else_body
    {
        compile_block(instructions, function_block, block, CompileTarget::Function)?;
    }
    else
    {
        instructions.push(Instruction
            {
                location: Some(conditional.location.clone()),
                code: Code::Push,
                operand: Some(Value::None)
            });
        instructions.push(Instruction { location: None, code: Code::PopResult, operand: None });
    }

    // Reserve a label from an emitted jump's unique position, as with boolean
    // expressions. The linker, not this emitter, resolves its destination index.
    let end_label = Value::Integer(end_jumps[0] as i64);
    for index in end_jumps
    {
        instructions[index].operand = Some(end_label.clone());
    }
    instructions.push(Instruction
        {
            location: Some(conditional.location.clone()),
            code: Code::JumpTarget,
            operand: Some(end_label)
        });
    Ok(())
}


// Inline scoped blocks into their enclosing code vector. Optimize and link only
// after the entire vector has been emitted, so nested labels remain unique.
fn compile_statements(instructions: &mut Vec<Instruction>,
                       function_block: &FunctionBlockRef,
                       ast: &AstTopLevel,
                       target: CompileTarget) -> CompileResult<()>
{
    let last_statement = ast.iter()
        .rposition(|statement| !matches!(statement, AstStatement::NullStatement));

    for (index, ast_item) in ast.iter().enumerate()
    {
        let mut add_check = true;
        let implicit_return = target == CompileTarget::Function && last_statement == Some(index);

        match ast_item
        {
            AstStatement::NullStatement => { add_check = false; },

            AstStatement::LetStatement(let_statement) =>
                {
                    compile_let_statement(instructions, function_block, let_statement)?;
                    add_check = false;
                },

            AstStatement::SetStatement(set_statement) =>
                {
                    compile_set_statement(instructions, function_block, set_statement)?;
                    add_check = false;
                },

            AstStatement::AliasStatement(alias_statement) =>
                {
                    compile_alias_statement(instructions, alias_statement);
                    add_check = false;
                },

            AstStatement::ExecuteStatement(execute_statement) =>
                {
                    compile_execute_statement(instructions, function_block, execute_statement)?;
                }

            AstStatement::ExpressionStatement(expression) =>
                {
                    compile_expression(instructions, function_block, expression)?;

                    if matches!(&expression.kind, AstExpressionKind::Variable(_))
                    {
                        instructions.push(Instruction
                            {
                                location: Some(expression.location.clone()),
                                code: Code::ExecuteIfExecutable,
                                operand: None
                            });
                    }
                }

            AstStatement::ReturnStatement(return_statement) =>
                {
                    compile_return_statement(instructions, function_block, return_statement)?;
                    add_check = false;
                }

            AstStatement::FunctionDefinition(function_statement) =>
                {
                    compile_function_definition(&function_block, function_statement)?;
                    add_check = false;
                }

            AstStatement::BlockStatement(block) =>
                {
                    compile_block(instructions, function_block, block,
                        if implicit_return { CompileTarget::Function } else { CompileTarget::Toplevel })?;
                    // The block's statements already checked or preserved their results.
                    add_check = false;
                }
        }

        if add_check && !implicit_return
        {
            instructions.push(Instruction
                {
                    location: None,
                    code: Code::CheckResult,
                    operand: None
                });
        }
    }

    if target == CompileTarget::Function && last_statement.is_none_or(|index|
        !matches!(ast[index], AstStatement::ExpressionStatement(_)
            | AstStatement::ExecuteStatement(_) | AstStatement::ReturnStatement(_)
            | AstStatement::BlockStatement(_)))
    {
        instructions.push(Instruction
            {
                location: None,
                code: Code::Push,
                operand: Some(Value::None)
            });
        instructions.push(Instruction { location: None, code: Code::PopResult, operand: None });
    }

    Ok(())
}


pub fn compile_ast(function_block: &FunctionBlockRef,
                   ast: &AstTopLevel,
                   target: CompileTarget) -> CompileResult<Vec<Instruction>>
{
    let mut instructions = Vec::new();
    compile_statements(&mut instructions, function_block, ast, target)?;

    optimize_instructions(&mut instructions);
    remove_empty_result_checks(&mut instructions);
    link_instructions(&mut instructions)?;

    Ok(instructions)
}
