
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


fn compile_expression(instructions: &mut Vec<Instruction>, expression: &AstExpression)
{
    match &expression.kind
    {
        AstExpressionKind::Grouped(inner) =>
            {
                compile_expression(instructions, inner);

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

                return;
            },

        AstExpressionKind::Execute(execute) =>
            {
                compile_execute_statement(instructions, execute);
                return;
            },

        AstExpressionKind::ExecutableReference(inner) =>
            {
                compile_expression(instructions, inner);
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::MakeExecutable,
                        operand: None
                    });
                return;
            },

        AstExpressionKind::TryExecute(command) =>
            {
                compile_expression(instructions, command);
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
                return;
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

        AstExpressionKind::MathExpression(operator, lhs, rhs) =>
            {
                compile_expression(instructions, lhs);

                instructions.push(Instruction
                    {
                        location: None,
                        code: Code::PushResult,
                        operand: None
                    });

                compile_expression(instructions, rhs);

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
}


fn compile_return_statement(instructions: &mut Vec<Instruction>, return_statement: &AstReturnStatement)
{
    if let Some(expression) = &return_statement.expression
    {
        compile_expression(instructions, expression);
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
}


fn compile_let_statement(instructions: &mut Vec<Instruction>, let_statement: &AstLetStatement)
{
    // Evaluate before changing the binding so self-reference and failed initializers are safe.
    compile_expression(instructions, &let_statement.expression);
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
}

fn compile_set_statement(instructions: &mut Vec<Instruction>, set_statement: &AstSetStatement)
{
    compile_expression(instructions, &set_statement.expression);

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
                             execute_statement: &AstExecuteStatement)
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
        compile_expression(instructions, argument);

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


pub fn compile_ast(function_block: &FunctionBlockRef,
                   ast: &AstTopLevel,
                   target: CompileTarget) -> CompileResult<Vec<Instruction>>
{
    let mut instructions = Vec::new();
    let last_statement = ast.iter()
        .rposition(|statement| !matches!(statement, AstStatement::NullStatement));

    for (index, ast_item) in ast.iter().enumerate()
    {
        let mut add_check = true;

        match ast_item
        {
            AstStatement::NullStatement => { add_check = false; },

            AstStatement::LetStatement(let_statement) =>
                {
                    compile_let_statement(&mut instructions, let_statement);
                    add_check = false;
                },

            AstStatement::SetStatement(set_statement) =>
                {
                    compile_set_statement(&mut instructions, set_statement);
                    add_check = false;
                },

            AstStatement::AliasStatement(alias_statement) =>
                {
                    compile_alias_statement(&mut instructions, alias_statement);
                    add_check = false;
                },

            AstStatement::ExecuteStatement(execute_statement) =>
                {
                    compile_execute_statement(&mut instructions, execute_statement);
                }

            AstStatement::ExpressionStatement(expression) =>
                {
                    compile_expression(&mut instructions, expression);

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
                    compile_return_statement(&mut instructions, return_statement);
                    add_check = false;
                }

            AstStatement::FunctionDefinition(function_statement) =>
                {
                    compile_function_definition(&function_block, function_statement)?;
                    add_check = false;
                }
        }

        let implicit_return =    target == CompileTarget::Function
                              && last_statement == Some(index);

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
            | AstStatement::ExecuteStatement(_) | AstStatement::ReturnStatement(_)))
    {
        instructions.push(Instruction
            {
                location: None,
                code: Code::Push,
                operand: Some(Value::None)
            });
        instructions.push(Instruction { location: None, code: Code::PopResult, operand: None });
    }

    Ok(instructions)
}
