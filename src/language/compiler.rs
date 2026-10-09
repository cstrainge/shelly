
use std::{ cell::RefCell, collections::HashMap, fmt::{ self, Display, Formatter }, rc::Rc };

use crate::language::{ ast::*,
                       bytecode::{ Code, Instruction, Function, FunctionBlock, FunctionBlockRef },
                       data::{ value::{ Value, Executable }, types::TypeRegistry,
                               methods::method_key, map_key::MapKey },
                       text::location::Location,
                       parser::ParserError,
                       typecheck::check_ast };

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
    TypeError(String),
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
            ErrorWhat::TypeError(message) =>
                {
                    if let Some(location) = &self.location
                    {
                        write!(f, "Type error: {}: {}", location, message)
                    }
                    else { write!(f, "Type error: {}", message) }
                },

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


fn bind_known_function(block: &FunctionBlockRef, value: Value) -> Value
{
    let Value::String(name, _) = &value else { return value; };
    if block.borrow().builtins.contains(name.as_str()) { return value; }
    let mut scope = Some(block.clone());
    while let Some(current) = scope
    {
        let current = current.borrow();
        if let Some(function) = current.functions.get(name)
        { return Value::String(name.clone(), Executable::Function(function.clone())); }
        // The current definition is installed after compiling its body. Do not
        // accidentally bind recursion to a previous definition of the same name.
        if current.function_name.as_ref() == Some(name) || current.declared_functions.contains(name)
        {
            break;
        }
        scope = current.parent.clone();
    }
    value
}


fn compile_expression(instructions: &mut Vec<Instruction>,
                      function_block: &FunctionBlockRef,
                      expression: &AstExpression) -> CompileResult<()>
{
    compile_expression_mode(instructions, function_block, expression, false)
}

fn function_binding(function: &AstFunctionStatement) -> String
{
    function.receiver_type.map_or_else(|| function.name.clone(),
        |id| method_key(id, &function.name))
}

// Preserve visible versions for every possible receiver. Unresolved local declarations
// mask outer versions and resolve through the frozen function scope at execution time.
fn method_snapshot(block: &FunctionBlockRef, name: &str) -> Value
{
    let mut methods = HashMap::new();
    let suffix = format!(":{}", name);
    let mut scope = Some(block.clone());
    while let Some(current) = scope
    {
        let current = current.borrow();
        for (key, function) in &current.functions
        {
            if key.starts_with("\0method:") && key.ends_with(&suffix)
            {
                methods.entry(MapKey::String(key.clone())).or_insert_with(||
                    Value::String(name.to_string(), Executable::Function(function.clone())));
            }
        }
        for key in current.declared_functions.iter().chain(current.function_name.iter())
        {
            if key.starts_with("\0method:") && key.ends_with(&suffix)
            { methods.entry(MapKey::String(key.clone())).or_insert(Value::None); }
        }
        scope = current.parent.clone();
    }
    Value::from_hash_map(methods)
}

// Preserve addressable receivers while evaluating each index and intermediate method once.
fn compile_receiver(instructions: &mut Vec<Instruction>, block: &FunctionBlockRef,
                    expression: &AstExpression) -> CompileResult<()>
{
    let (code, operand) = match &expression.kind
        {
            AstExpressionKind::Variable(variable) =>
                (Code::ReferenceVariable, Some(Value::from_string(variable.name.clone()))),
            AstExpressionKind::Field(object, name, _) =>
                {
                    compile_receiver(instructions, block, object)?;
                    (Code::ReferenceField, Some(Value::from_array(vec![
                            Value::from_string(name.clone()), method_snapshot(block, name),
                        ])))
                },
            AstExpressionKind::Index(object, index) =>
                {
                    compile_receiver(instructions, block, object)?;
                    compile_expression(instructions, block, index)?;
                    instructions.push(Instruction
                        { location: None, code: Code::PushResult, operand: None });
                    (Code::ReferenceIndex, None)
                },
            AstExpressionKind::Grouped(inner) =>
                {
                    compile_receiver(instructions, block, inner)?;
                    let mode = if matches!(inner.kind, AstExpressionKind::Field(_, _, _))
                        { Value::Integer(1) }
                        else { Value::Boolean(matches!(inner.kind,
                            AstExpressionKind::Variable(_) | AstExpressionKind::Index(_, _)
                            | AstExpressionKind::ExecutableReference(_)
                            | AstExpressionKind::Literal(AstLiteral
                                { value: Value::String(_, _), .. }))) };
                    (Code::ReferenceGroup, Some(mode))
                },
            _ =>
                {
                    compile_expression(instructions, block, expression)?;
                    instructions.push(Instruction
                        { location: None, code: Code::PushResult, operand: None });
                    (Code::ReferenceValue, None)
                },
        };
    instructions.push(Instruction { location: Some(expression.location.clone()), code, operand });
    Ok(())
}

fn compile_expression_mode(instructions: &mut Vec<Instruction>,
                           function_block: &FunctionBlockRef,
                           expression: &AstExpression, bind_method: bool) -> CompileResult<()>
{
    match &expression.kind
    {
        AstExpressionKind::SpacedEmptyCall(_) => return Err(CompileError
            { location: Some(expression.location.clone()),
              what: ErrorWhat::TypeError(
                  "Unresolved spaced call reached code generation".to_string()) }),
        AstExpressionKind::StructConstructor(item) =>
            {
                let id = item.type_id.ok_or_else(|| CompileError
                    {
                        location: Some(expression.location.clone()),
                        what: ErrorWhat::TypeError("Unresolved struct constructor".to_string()),
                    })?;
                for (_, _, value) in &item.fields
                {
                    compile_expression(instructions, function_block, value)?;
                    instructions.push(Instruction
                        {
                            location: None,
                            code: Code::PushResult,
                            operand: None,
                        });
                }
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::MakeStruct,
                        operand: Some(Value::from_array(vec![
                                Value::Integer(id.0 as i64),
                                Value::from_array(
                                    item.field_indexes
                                        .iter()
                                        .map(|index| Value::Integer(*index as i64))
                                        .collect(),
                                ),
                            ])),
                    });
            },
        AstExpressionKind::Field(object, name, index) =>
            {
                compile_receiver(instructions, function_block, object)?;
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: if bind_method { Code::BindField } else { Code::GetField },
                        operand: Some(Value::from_array(vec![
                                index.map_or_else(
                                    || Value::from_string(name.clone()),
                                    |index| Value::Integer(index as i64)),
                                method_snapshot(function_block, name),
                            ])),
                    });
            },
        AstExpressionKind::EnumVariant(_, _) => return Err(CompileError
            { location: Some(expression.location.clone()),
              what: ErrorWhat::TypeError(
                  "Unresolved enum reference reached code generation".to_string()) }),
        AstExpressionKind::Range(start, end, inclusive) =>
            {
                for bound in [start, end].into_iter().flatten()
                {
                    compile_expression(instructions, function_block, bound)?;
                    instructions.push(Instruction
                        {
                            location: None,
                            code: Code::PushResult,
                            operand: None,
                        });
                }
                let flags = i64::from(start.is_some()) | (i64::from(end.is_some()) << 1)
                    | (i64::from(*inclusive) << 2);
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::MakeRange,
                        operand: Some(Value::Integer(flags))
                    });
            },
        AstExpressionKind::HashMap(pairs) =>
            {
                for (key, value) in pairs
                {
                    compile_expression(instructions, function_block, key)?;
                    instructions.push(Instruction
                        {
                            location: None,
                            code: Code::PushResult,
                            operand: None,
                        });
                    compile_expression(instructions, function_block, value)?;
                    instructions.push(Instruction
                        {
                            location: None,
                            code: Code::PushResult,
                            operand: None,
                        });
                }
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::MakeHashMap,
                        operand: Some(Value::Integer(pairs.len() as i64))
                    });
            },

        AstExpressionKind::Array(elements) =>
            {
                for element in elements
                {
                    compile_expression(instructions, function_block, element)?;
                    instructions.push(Instruction
                        {
                            location: None,
                            code: Code::PushResult,
                            operand: None,
                        });
                }
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::MakeArray,
                        operand: Some(Value::Integer(elements.len() as i64))
                    });
            },

        AstExpressionKind::Index(array, index) =>
            {
                compile_expression(instructions, function_block, array)?;
                instructions.push(Instruction
                    {
                        location: None,
                        code: Code::PushResult,
                        operand: None,
                    });
                compile_expression(instructions, function_block, index)?;
                instructions.push(Instruction
                    {
                        location: None,
                        code: Code::PushResult,
                        operand: None,
                    });
                instructions.push(Instruction
                    {
                        location: Some(index.location.clone()),
                        code: Code::GetElement,
                        operand: None
                    });
            },

        AstExpressionKind::Splat(inner) =>
            {
                compile_expression(instructions, function_block, inner)?;
                instructions.push(Instruction
                    {
                        location: None,
                        code: Code::PushResult,
                        operand: None,
                    });
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::ExpandArray,
                        operand: None
                    });
            },

        AstExpressionKind::IfExpression(conditional) =>
            {
                compile_if_expression(instructions, function_block, conditional)?;
                return Ok(());
            },

        AstExpressionKind::MatchExpression(matching) =>
            {
                compile_match_expression(instructions, function_block, matching)?;
                return Ok(());
            },

        AstExpressionKind::Grouped(inner) =>
            {
                compile_expression_mode(instructions, function_block, inner,
                    matches!(inner.kind, AstExpressionKind::Field(_, _, _)))?;

                // Calls already execute, and nested groups handle their own value. Only a
                // variable or string literal can supply an unevaluated executable reference.
                if matches!(&inner.kind,
                    AstExpressionKind::Variable(_)
                    | AstExpressionKind::Index(_, _) | AstExpressionKind::Field(_, _, _)
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

        AstExpressionKind::Redirect(source, redirects) =>
            {
                // Evaluate all endpoints before installing any of the stream overrides.
                for redirect in redirects
                {
                    let variable = matches!(redirect.target.kind, AstExpressionKind::Variable(_));
                    if variable
                    {
                        instructions.push(Instruction
                            {
                                location: Some(redirect.target.location.clone()),
                                code: Code::Push,
                                operand: Some(Value::from_string(
                                    redirect.target.resolve_as_text().map_err(ParserError::from)?)),
                            });
                    }
                    else
                    {
                        compile_expression(instructions, function_block, &redirect.target)?;
                        instructions.push(Instruction
                            {
                                location: None,
                                code: Code::PushResult,
                                operand: None,
                            });
                    }
                }
                for (index, redirect) in redirects.iter().enumerate()
                {
                    let stream = match redirect.stream
                        {
                            RedirectStream::Output => 1,
                            RedirectStream::Error => 2,
                            RedirectStream::Both => 3,
                        };
                    instructions.push(Instruction
                        {
                            location: Some(redirect.location.clone()),
                            code: Code::BeginRedirect,
                            operand: Some(Value::from_array(vec![
                                    Value::Integer(stream),
                                    Value::Boolean(matches!(redirect.target.kind,
                                        AstExpressionKind::Variable(_))),
                                    Value::Integer((redirects.len() - index) as i64),
                                ])),
                        });
                }
                compile_expression(instructions, function_block, source)?;
                if matches!(source.kind, AstExpressionKind::Symbol(_)
                    | AstExpressionKind::Variable(_) | AstExpressionKind::Literal(_)
                    | AstExpressionKind::ExecutableReference(_))
                {
                    instructions.push(Instruction
                        {
                            location: Some(source.location.clone()),
                            code: Code::RedirectSource,
                            // Only bare command words have implicit command resolution.
                            operand: Some(Value::Boolean(matches!(source.kind,
                                AstExpressionKind::Symbol(_)))),
                        });
                }
                for _ in redirects
                {
                    instructions.push(Instruction
                        {
                            location: Some(expression.location.clone()),
                            code: Code::EndRedirect,
                            operand: None,
                        });
                }
                return Ok(());
            },

        AstExpressionKind::ExecutableReference(inner) =>
            {
                compile_expression_mode(instructions, function_block, inner, true)?;
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
                        operand: Some(bind_known_function(
                            function_block,
                            Value::from_string(symbol.name.clone()),
                        )),
                    });
            },

        AstExpressionKind::Literal(value) =>
            {
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::Push,
                        operand: Some(if matches!(value.value, Value::String(_, Executable::Yes))
                            { bind_known_function(function_block, value.value.clone()) }
                            else { value.value.clone() })
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

        AstExpressionKind::TypeConversion(_, inner, type_id) =>
            {
                let id = type_id.ok_or_else(|| CompileError
                    {
                        location: Some(expression.location.clone()),
                        what: ErrorWhat::TypeError("Unresolved conversion target".to_string()),
                    })?;
                compile_expression(instructions, function_block, inner)?;
                instructions.push(Instruction
                    {
                        location: Some(expression.location.clone()),
                        code: Code::ConvertType,
                        operand: Some(Value::Integer(id.0 as i64)),
                    });
                return Ok(());
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

                instructions.push(Instruction
                    {
                        location: None,
                        code: Code::PushResult,
                        operand: None,
                    });
                compile_expression(instructions, function_block, rhs)?;
                instructions.push(Instruction
                    {
                        location: None,
                        code: Code::PushResult,
                        operand: None,
                    });
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
                    code: if matches!(&expression.kind,
                                      AstExpressionKind::Symbol(symbol) if symbol.is_glob())
                        { Code::InterpolateGlob } else { Code::InterpolateString },
                    operand: Some(Value::from_array(escaped_dollars.iter()
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

    if let Some(id) = let_statement.type_id
    {
        instructions.push(Instruction { location: Some(let_statement.location.clone()),
            code: Code::ValidateType, operand: Some(Value::Integer(id.0 as i64)) });
    }
    instructions.push(Instruction
        {
            location: Some(let_statement.location.clone()),
            code: Code::NewVariable,
            operand: Some(match let_statement.type_id
                {
                    Some(id) => Value::from_array(vec![
                            Value::from_string(let_statement.identifier.clone()),
                            Value::Integer(id.0 as i64),
                        ]),
                    None => Value::from_string(let_statement.identifier.clone())
                })
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
    for access in &set_statement.indexes
    {
        match access
        {
            AstAccess::Index(index) =>
                {
                    compile_expression(instructions, function_block, index)?;
                    instructions.push(Instruction
                        {
                            location: None,
                            code: Code::PushResult,
                            operand: None,
                        });
                },
            AstAccess::Field(name) => instructions.push(Instruction { location: None,
                code: Code::Push, operand: Some(Value::from_string(name.clone())) })
        }
    }
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
            code: if set_statement.indexes.is_empty()
            {
                Code::SetVariable
            }
            else
            {
                Code::SetElement
            },
            operand: Some(
                if set_statement.indexes.is_empty()
                {
                    Value::from_string(set_statement.identifier.clone())
                }
                else
                {
                    Value::from_array(vec![
                            Value::from_string(set_statement.identifier.clone()),
                            Value::from_array(
                                set_statement
                                    .indexes
                                    .iter()
                                    .map(|access|
                                        Value::Boolean(matches!(access, AstAccess::Field(_))))
                                    .collect(),
                            ),
                        ])
                },
            ),
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
            operand: Some(Value::from_array(vec![Value::from_string(alias_statement.alias.clone()),
                                           Value::Integer(alias_statement.arguments.len() as i64)]))
        });
}


fn compile_execute_statement(instructions: &mut Vec<Instruction>,
                             function_block: &FunctionBlockRef,
                             execute_statement: &AstExecuteStatement) -> CompileResult<()>
{
    if matches!(
        execute_statement.executable.kind,
        AstExpressionKind::Index(_, _) | AstExpressionKind::Field(_, _, _)
    )
    {
        compile_expression_mode(instructions, function_block, &execute_statement.executable, true)?;
        instructions.push(Instruction
            {
                location: None,
                code: Code::PushResult,
                operand: None,
            });
    }
    else
    {
        instructions.push(Instruction
            {
                location: None,
                code: Code::Push,
                operand: Some(bind_known_function(
                    function_block,
                    Value::from_executable_string(
                        execute_statement
                            .executable
                            .resolve_as_text()
                            .map_err(ParserError::from)?,
                    ),
                )),
            });
    }

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
        // Bind a member before the implicit argument call, so a method result is
        // never executed a second time. Callable data fields follow the same path.
        compile_expression_mode(instructions, function_block, argument,
            matches!(argument.kind, AstExpressionKind::Field(_, _, _)))?;

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

            AstExpressionKind::Variable(_) | AstExpressionKind::Index(_, _)
                | AstExpressionKind::Field(_, _, _) =>
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
    let binding = function_binding(function_statement);
    // Keep versions already visible at this definition. The submission scope
    // remains a fallback for forward declarations and is frozen at publication.
    let captured = Rc::new(RefCell::new(FunctionBlock
        {
            parent: Some(parent_block.clone()),
            function_name: None,
            declared_functions: parent_block.borrow().declared_functions.clone(),
            builtins: parent_block.borrow().builtins.clone(),
            functions: parent_block.borrow().functions.clone(),
        }));
    let function_block = Rc::new(RefCell::new(FunctionBlock
        {
            parent: Some(captured.clone()),
            function_name: Some(binding.clone()),
            declared_functions: Default::default(),
            builtins: parent_block.borrow().builtins.clone(),
            functions: HashMap::new()
        }));

    let mut prologue = Vec::new();
    for (index, parameter) in function_statement.parameters.iter().enumerate()
    {
        let mut operand = vec![
                Value::from_string(parameter.name.clone()),
                parameter
                    .type_id
                    .map_or(Value::None, |id| Value::Integer(id.0 as i64)),
                Value::Integer(index as i64),
            ];
        if parameter.optional { operand.push(Value::None); }
        prologue.push(Instruction { location: Some(parameter.location.clone()),
            code: if parameter.variadic { Code::BindRestParameter } else { Code::BindParameter },
            operand: Some(Value::from_array(operand)) });
    }
    let instructions = compile_with_prologue(&function_block, &function_statement.body,
        CompileTarget::Function, prologue)?;

    let new_function = Rc::new(Function
        {
            functions: function_block,
            arguments: function_statement
                .parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect(),
            minimum_arguments: function_statement
                .parameters
                .iter()
                .take_while(|parameter| !parameter.optional && !parameter.variadic)
                .count(),
            variadic: function_statement
                .parameters
                .last()
                .is_some_and(|parameter| parameter.variadic),
            return_type: function_statement.return_type,
            code: instructions,
        });

    captured.borrow_mut().functions.insert(binding.clone(), new_function.clone());
    parent_block.borrow_mut().functions.insert(binding, new_function);

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
        if    matches!(instruction.code, Code::PopResult)
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
                    Code::PushResult | Code::CheckResult | Code::EndIteration => true,

                    Code::PopResult | Code::Execute | Code::TryExecute
                    | Code::RedirectSource
                    | Code::ExecuteIfExecutable | Code::MakeExecutable
                    | Code::ToBoolean | Code::ConvertType | Code::BooleanNot
                    | Code::NextIteration | Code::MatchPattern | Code::MatchFail => false,

                    // Do not carry a proof across control-flow boundaries.
                    Code::Jump | Code::JumpIfFalse | Code::JumpIfTrue
                    | Code::JumpTarget | Code::ExitFunction
                    | Code::EnterLoop | Code::ExitLoop | Code::Break | Code::Continue => false,

                    // These instructions operate on the value stack or other VM state.
                    Code::Push
                    | Code::Discard
                    | Code::BeginRedirect | Code::EndRedirect
                    | Code::NewVariable
                    | Code::SetVariable
                    | Code::GetVariable
                    | Code::ValidateType
                    | Code::StartIteration
                    | Code::BindIteration
                    | Code::BindParameter
                    | Code::BindRestParameter
                    | Code::NewAlias
                    | Code::ExportVariable
                    | Code::GlobFiles
                    | Code::ExpandArray
                    | Code::ExpandPath
                    | Code::InterpolateString
                    | Code::MakeArray
                    | Code::UnpackArray
                    | Code::MakeHashMap
                    | Code::MakeStruct
                    | Code::GetField
                    | Code::BindField
                    | Code::ReferenceVariable
                    | Code::ReferenceValue
                    | Code::ReferenceField
                    | Code::ReferenceIndex
                    | Code::ReferenceGroup
                    | Code::MakeRange
                    | Code::GetElement
                    | Code::SetElement
                    | Code::InterpolateGlob
                    | Code::EnterScope
                    | Code::ExitScope
                    | Code::MathAdd
                    | Code::MathSubtract
                    | Code::MathMultiply
                    | Code::MathDivide
                    | Code::MathModulo
                    | Code::CompareEqual
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
            resolved.push((index, Value::Integer(*target as i64)));
        }
        if matches!(instruction.code, Code::EnterLoop)
        {
            let invalid = || CompileError
                {
                    location: instruction.location.clone(),
                    what: ErrorWhat::InvalidJump("Expected continue and break labels".to_string())
                };
            let Some(Value::Array(labels)) = &instruction.operand else { return Err(invalid()); };
            if labels.len() != 2 { return Err(invalid()); }
            let mut addresses = Vec::new();
            for label in labels.iter()
            {
                let Value::Integer(id) = label else { return Err(invalid()); };
                let target = targets.get(id).ok_or_else(|| CompileError
                    {
                        location: instruction.location.clone(),
                        what: ErrorWhat::InvalidJump(format!("Missing jump label {}", id))
                    })?;
                addresses.push(Value::Integer(*target as i64));
            }
            resolved.push((index, Value::from_array(addresses)));
        }
    }
    for (index, target) in resolved
    {
        instructions[index].operand = Some(target);
    }
    for instruction in instructions
    {
        if matches!(instruction.code, Code::JumpTarget) { instruction.operand = None; }
    }
    Ok(())
}


fn compile_for_statement(instructions: &mut Vec<Instruction>,
                          function_block: &FunctionBlockRef,
                          statement: &AstForStatement) -> CompileResult<()>
{
    let location = Some(statement.location.clone());
    compile_expression(instructions, function_block, &statement.iterable)?;
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::PushResult,
            operand: None,
        });
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::StartIteration,
            operand: Some(Value::Integer(if statement.destructure
                { 1 } else { statement.bindings.len() as i64 }))
        });

    // Register once. Continue targets the next iteration, not EnterLoop itself.
    let enter = instructions.len();
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::EnterLoop,
            operand: None,
        });
    // Reserve labels using distinct emitted positions, just as conditionals do.
    let start = Value::Integer(instructions.len() as i64);
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::JumpTarget,
            operand: Some(start.clone()),
        });
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::NextIteration,
            operand: None,
        });
    let end = Value::Integer(instructions.len() as i64);
    instructions[enter].operand = Some(Value::from_array(vec![start.clone(), end.clone()]));
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::JumpIfFalse,
            operand: Some(end.clone()),
        });
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::CheckResult,
            operand: None,
        });
    if statement.destructure
    {
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::UnpackArray,
                operand: Some(Value::Integer(statement.bindings.len() as i64)),
            });
    }
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::EnterScope,
            operand: None,
        });
    // Iteration and unpacking push values in source order; bind in reverse stack order.
    for name in statement.bindings.iter().rev()
    {
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::BindIteration,
                operand: Some(Value::from_string(name.clone()))
            });
    }
    compile_statements(
        instructions,
        function_block,
        &statement.body.body,
        CompileTarget::Toplevel,
    )?;
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::ExitScope,
            operand: None,
        });
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::Jump,
            operand: Some(start),
        });
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::JumpTarget,
            operand: Some(end),
        });
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::ExitLoop,
            operand: None,
        });
    instructions.push(Instruction { location, code: Code::EndIteration, operand: None });
    Ok(())
}


fn compile_loop_statement(instructions: &mut Vec<Instruction>,
                           function_block: &FunctionBlockRef,
                           body: &AstBlockStatement,
                           condition: Option<&AstExpression>,
                           until: bool) -> CompileResult<()>
{
    let location = Some(condition.map_or(&body.location, |condition| &condition.location).clone());
    // Reserve distinct labels from EnterLoop and the following JumpTarget.
    // Continue rechecks the condition (if any), without pushing another loop frame.
    let end = Value::Integer(instructions.len() as i64);
    let start = Value::Integer(instructions.len() as i64 + 1);
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::EnterLoop,
            operand: Some(Value::from_array(vec![start.clone(), end.clone()]))
        });
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::JumpTarget,
            operand: Some(start.clone()),
        });
    if let Some(condition) = condition
    {
        compile_expression(instructions, function_block, condition)?;
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::ToBoolean,
                operand: None,
            });
        instructions.push(Instruction
            {
                location: location.clone(),
                code: if until { Code::JumpIfTrue } else { Code::JumpIfFalse },
                operand: Some(end.clone())
            });
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::CheckResult,
                operand: None,
            });
    }
    compile_block(instructions, function_block, body, CompileTarget::Toplevel)?;
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::Jump,
            operand: Some(start),
        });
    instructions.push(Instruction
        {
            location: location.clone(),
            code: Code::JumpTarget,
            operand: Some(end),
        });
    if condition.is_some()
    {
        // Consume the stopping condition; a loop has no expression result.
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::CheckResult,
                operand: None,
            });
    }
    instructions.push(Instruction { location, code: Code::ExitLoop, operand: None });
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


fn compile_match_expression(instructions: &mut Vec<Instruction>,
                             function_block: &FunctionBlockRef,
                             matching: &AstMatchExpression) -> CompileResult<()>
{
    compile_expression(instructions, function_block, &matching.value)?;
    instructions.push(Instruction { location: None, code: Code::PushResult, operand: None });
    let mut end_jumps = Vec::new();
    let mut fallback = false;
    for arm in &matching.arms
    {
        let location = Some(arm.body.location.clone());
        let next_arm = if let Some(pattern) = &arm.pattern
            {
                compile_expression(instructions, function_block, pattern)?;
                instructions.push(Instruction
                    { location: None, code: Code::PushResult, operand: None });
                instructions.push(Instruction
                    {
                        location: Some(pattern.location.clone()),
                        code: Code::MatchPattern,
                        operand: None,
                    });
                let label = Value::Integer(instructions.len() as i64);
                instructions.push(Instruction
                    { location: None, code: Code::JumpIfFalse, operand: Some(label.clone()) });
                instructions.push(Instruction
                    { location: None, code: Code::CheckResult, operand: None });
                Some(label)
            }
            else
            {
                fallback = true;
                instructions.push(Instruction
                    { location: location.clone(), code: Code::Discard, operand: None });
                None
            };
        compile_block(instructions, function_block, &arm.body, CompileTarget::Function)?;
        end_jumps.push(instructions.len());
        instructions.push(Instruction
            { location: location.clone(), code: Code::Jump, operand: None });
        if let Some(label) = next_arm
        {
            instructions.push(Instruction
                { location: location.clone(), code: Code::JumpTarget, operand: Some(label) });
            instructions.push(Instruction { location, code: Code::CheckResult, operand: None });
        }
    }
    if !fallback
    {
        instructions.push(Instruction
            {
                location: Some(matching.location.clone()),
                code: Code::MatchFail,
                operand: None,
            });
    }
    let end_label = Value::Integer(end_jumps[0] as i64);
    for index in end_jumps { instructions[index].operand = Some(end_label.clone()); }
    instructions.push(Instruction
        { location: None, code: Code::JumpTarget, operand: Some(end_label) });
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
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::ToBoolean,
                operand: None,
            });
        let next_branch = Value::Integer(instructions.len() as i64);
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::JumpIfFalse,
                operand: Some(next_branch.clone())
            });
        // Consume the condition on both paths. Only the selected block supplies
        // the expression's result; conditions must not leak into empty branches.
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::CheckResult,
                operand: None,
            });
        compile_block(instructions, function_block, &branch.body, CompileTarget::Function)?;
        end_jumps.push(instructions.len());
        instructions.push(Instruction
            {
                location: location.clone(),
                code: Code::Jump,
                operand: None,
            });
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
    // A forward local declaration shadows an outer function even before its
    // body is compiled. Once a local version exists, calls bind that version.
    function_block.borrow_mut().declared_functions.extend(ast.iter().filter_map(|statement|
        if let AstStatement::FunctionDefinition(item) = statement { Some(function_binding(item)) }
        else { None }));
    let last_statement = ast.iter()
        .rposition(|statement| !matches!(statement, AstStatement::NullStatement));

    for (index, ast_item) in ast.iter().enumerate()
    {
        let mut add_check = true;
        let implicit_return = target == CompileTarget::Function && last_statement == Some(index);

        match ast_item
        {
            AstStatement::EnumDeclaration(_) | AstStatement::StructDeclaration(_)
                | AstStatement::NullStatement => { add_check = false; },

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
                    compile_expression_mode(instructions, function_block, expression,
                        matches!(expression.kind, AstExpressionKind::Field(_, _, _)))?;

                    if matches!(
                        &expression.kind,
                        AstExpressionKind::Variable(_)
                            | AstExpressionKind::Index(_, _)
                            | AstExpressionKind::Field(_, _, _)
                    )
                    {
                        instructions.push(Instruction
                            {
                                location: Some(expression.location.clone()),
                                code: Code::ExecuteIfExecutable,
                                operand: Some(Value::Boolean(!implicit_return)),
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
                    compile_block(
                        instructions,
                        function_block,
                        block,
                        if implicit_return
                        {
                            CompileTarget::Function
                        }
                        else
                        {
                            CompileTarget::Toplevel
                        },
                    )?;
                    // The block's statements already checked or preserved their results.
                    add_check = false;
                }

            AstStatement::ForStatement(statement) =>
                {
                    compile_for_statement(instructions, function_block, statement)?;
                    add_check = false;
                }

            AstStatement::LoopStatement(body) =>
                {
                    compile_loop_statement(instructions, function_block, body, None, false)?;
                    add_check = false;
                }

            AstStatement::ConditionalLoopStatement(statement) =>
                {
                    compile_loop_statement(instructions, function_block, &statement.body,
                        Some(&statement.condition), statement.until)?;
                    add_check = false;
                }

            AstStatement::BreakStatement(location) | AstStatement::ContinueStatement(location) =>
                {
                    instructions.push(Instruction
                        {
                            location: Some(location.clone()),
                            code: if matches!(ast_item, AstStatement::BreakStatement(_))
                                { Code::Break } else { Code::Continue },
                            operand: None
                        });
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

    if    target == CompileTarget::Function
       && last_statement.is_none_or(|index|
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


fn compile_checked_ast(function_block: &FunctionBlockRef,
                   ast: &AstTopLevel,
                   target: CompileTarget) -> CompileResult<Vec<Instruction>>
{
    compile_with_prologue(function_block, ast, target, Vec::new())
}


fn compile_with_prologue(function_block: &FunctionBlockRef,
                         ast: &AstTopLevel,
                         target: CompileTarget,
                         mut instructions: Vec<Instruction>) -> CompileResult<Vec<Instruction>>
{
    compile_statements(&mut instructions, function_block, ast, target)?;

    optimize_instructions(&mut instructions);
    remove_empty_result_checks(&mut instructions);
    link_instructions(&mut instructions)?;

    Ok(instructions)
}


// Type names and definitions are committed together only after the complete
// submission has passed AST checking and bytecode generation/linking.
pub fn compile_ast(registry: &mut TypeRegistry,
                   function_block: &FunctionBlockRef,
                   ast: &mut AstTopLevel,
                   target: CompileTarget) -> CompileResult<Vec<Instruction>>
{
    let mut staged = registry.clone();
    check_ast(&mut staged, ast)?;
    // Each submission gets a private function namespace. Function bodies retain
    // this snapshot as their parent; later submissions publish into a new one.
    let functions = Rc::new(RefCell::new(FunctionBlock
        {
            parent: function_block.borrow().parent.clone(),
            function_name: function_block.borrow().function_name.clone(),
            declared_functions: function_block.borrow().declared_functions.clone(),
            builtins: function_block.borrow().builtins.clone(),
            functions: function_block.borrow().functions.clone(),
        }));
    match compile_checked_ast(&functions, ast, target)
    {
        Ok(code) =>
            {
                *registry = staged;
                function_block.borrow_mut().functions = functions.borrow().functions.clone();
                Ok(code)
            },
        Err(error) => Err(error)
    }
}
