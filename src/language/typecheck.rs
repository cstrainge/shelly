use std::{ collections::HashMap, rc::Rc };

use super::{ ast::*, compiler::{ CompileError, CompileResult, ErrorWhat },
             data::{ types::{ EnumValue, EnumVariant, TypeId, TypeKind, TypeRegistry }, value::Value },
             text::location::Location };


type Names = HashMap<String, TypeId>;

fn error(location: &Location, message: String) -> CompileError
{
    CompileError { location: Some(location.clone()), what: ErrorWhat::TypeError(message) }
}


// The initial checking pass resolves declared types and variants. Variable type
// inference and annotations can extend this pass without changing enum identity.
pub fn check_ast(registry: &mut TypeRegistry, ast: &mut AstTopLevel) -> CompileResult<()>
{
    let names = check_scope(registry, ast, &registry.names.clone())?;
    registry.names = names;
    Ok(())
}


fn check_scope(registry: &mut TypeRegistry, ast: &mut AstTopLevel, parent: &Names) -> CompileResult<Names>
{
    let mut names = parent.clone();
    let mut declarations = HashMap::new();

    // Register this scope before visiting any expressions, including function bodies.
    for statement in ast.iter()
    {
        if let AstStatement::EnumDeclaration(declaration) = statement
        {
            if let Some(previous) = declarations.insert(&declaration.name, &declaration.location)
            {
                return Err(error(&declaration.location,
                    format!("Duplicate type '{}'; first declared at {}", declaration.name, previous)));
            }
            let mut variants = Vec::<EnumVariant>::new();
            for (name, location) in &declaration.variants
            {
                if let Some(previous) = variants.iter().find(|variant| variant.name == *name)
                {
                    return Err(error(location,
                        format!("Duplicate variant '{}'; first declared at {}", name, previous.location)));
                }
                variants.push(EnumVariant { name: name.clone(), location: location.clone() });
            }
            let id = registry.register(declaration.name.clone(), TypeKind::Enum(variants),
                Some(declaration.location.clone()));
            names.insert(declaration.name.clone(), id);
        }
    }

    for statement in ast.iter_mut()
    {
        match statement
        {
            AstStatement::EnumDeclaration(_) | AstStatement::NullStatement
            | AstStatement::AliasStatement(_) | AstStatement::BreakStatement(_)
            | AstStatement::ContinueStatement(_) => {},
            AstStatement::LetStatement(statement) => check_expression(registry, &mut statement.expression, &names)?,
            AstStatement::SetStatement(statement) =>
                {
                    for index in &mut statement.indexes { check_expression(registry, index, &names)?; }
                    check_expression(registry, &mut statement.expression, &names)?;
                },
            AstStatement::ExpressionStatement(expression) => check_expression(registry, expression, &names)?,
            AstStatement::ExecuteStatement(statement) => check_call(registry, statement, &names)?,
            AstStatement::ReturnStatement(statement) =>
                {
                    if let Some(expression) = &mut statement.expression
                    {
                        check_expression(registry, expression, &names)?;
                    }
                },
            AstStatement::FunctionDefinition(statement) =>
                {
                    check_scope(registry, &mut statement.body, &names)?;
                },
            AstStatement::BlockStatement(block) | AstStatement::LoopStatement(block) =>
                {
                    check_scope(registry, &mut block.body, &names)?;
                },
            AstStatement::ForStatement(statement) =>
                {
                    check_expression(registry, &mut statement.iterable, &names)?;
                    check_scope(registry, &mut statement.body.body, &names)?;
                },
            AstStatement::ConditionalLoopStatement(statement) =>
                {
                    check_expression(registry, &mut statement.condition, &names)?;
                    check_scope(registry, &mut statement.body.body, &names)?;
                }
        }
    }
    Ok(names)
}


fn check_call(registry: &mut TypeRegistry, call: &mut AstExecuteStatement, names: &Names) -> CompileResult<()>
{
    check_expression(registry, &mut call.executable, names)?;
    if matches!(&call.executable.kind, AstExpressionKind::Literal(AstLiteral { value: Value::Enum(_) }))
    {
        return Err(error(&call.executable.location, "Cannot execute an enum as a command".to_string()));
    }
    for argument in &mut call.arguments { check_expression(registry, argument, names)?; }
    Ok(())
}


fn check_expression(registry: &mut TypeRegistry, expression: &mut AstExpression, names: &Names) -> CompileResult<()>
{
    match &mut expression.kind
    {
        AstExpressionKind::EnumVariant(name, variant) =>
            {
                let id = names.get(name).ok_or_else(|| error(&expression.location,
                    format!("Unknown type '{}'", name)))?;
                let definition = registry.get(*id);
                let TypeKind::Enum(variants) = &definition.kind else
                {
                    return Err(error(&expression.location, format!("Type '{}' is not an enum", name)));
                };
                let index = variants.iter().position(|item| item.name == *variant)
                    .ok_or_else(|| error(&expression.location,
                        format!("Unknown variant '{}::{}' (enum declared at {})", name, variant,
                            definition.location.as_ref().unwrap())))?;
                expression.kind = AstExpressionKind::Literal(AstLiteral
                    { value: Value::Enum(Rc::new(EnumValue { definition, variant: index })) });
            },
        AstExpressionKind::Array(values) =>
            {
                for value in values { check_expression(registry, value, names)?; }
            },
        AstExpressionKind::HashMap(entries) =>
            {
                for (key, value) in entries
                {
                    check_expression(registry, key, names)?;
                    check_expression(registry, value, names)?;
                }
            },
        AstExpressionKind::Range(start, end, _) =>
            {
                for bound in [start, end].into_iter().flatten()
                {
                    check_expression(registry, bound, names)?;
                }
            },
        AstExpressionKind::Index(left, right) | AstExpressionKind::MathExpression(_, left, right)
        | AstExpressionKind::BooleanExpression(_, left, right) =>
            {
                check_expression(registry, left, names)?;
                check_expression(registry, right, names)?;
            },
        AstExpressionKind::Splat(value) | AstExpressionKind::Grouped(value)
        | AstExpressionKind::ExecutableReference(value) | AstExpressionKind::TryExecute(value)
        | AstExpressionKind::BooleanNot(value) => check_expression(registry, value, names)?,
        AstExpressionKind::Execute(call) => check_call(registry, call, names)?,
        AstExpressionKind::IfExpression(conditional) =>
            {
                for branch in &mut conditional.branches
                {
                    check_expression(registry, &mut branch.condition, names)?;
                    check_scope(registry, &mut branch.body.body, names)?;
                }
                if let Some(block) = &mut conditional.else_body { check_scope(registry, &mut block.body, names)?; }
            },
        AstExpressionKind::Variable(_) | AstExpressionKind::VariableSplat(_)
        | AstExpressionKind::Symbol(_) | AstExpressionKind::Literal(_) => {}
    }
    Ok(())
}
