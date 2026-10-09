use std::{ collections::HashMap, rc::Rc };

use super::{ ast::*, compiler::{ CompileError, CompileResult, ErrorWhat },
             data::{ types::{ EnumValue, EnumVariant, FieldDefinition, StructValue, TypeId, TypeKind, TypeRegistry }, value::Value },
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

    // Reserve every local name before resolving any field annotations.
    let mut local_ids = Vec::new();
    for statement in ast.iter()
    {
        let (name, location) = match statement
            {
                AstStatement::EnumDeclaration(item) => (&item.name, &item.location),
                AstStatement::StructDeclaration(item) => (&item.name, &item.location),
                _ => continue
            };
        if matches!(name.as_str(), "any" | "optional")
        { return Err(error(location, format!("'{}' is reserved in type annotations", name))); }
        if let Some(previous) = declarations.insert(name, location)
        { return Err(error(location, format!("Duplicate type '{}'; first declared at {}", name, previous))); }
        let id = registry.register(name.clone(), TypeKind::Pending, Some(location.clone()));
        names.insert(name.clone(), id);
        local_ids.push(id);
    }
    for statement in ast.iter()
    {
        match statement
        {
            AstStatement::EnumDeclaration(declaration) =>
                {
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
                    registry.finish(names[&declaration.name], TypeKind::Enum(variants));
                },
            AstStatement::StructDeclaration(declaration) =>
                {
                    let mut fields = Vec::<FieldDefinition>::new();
                    for field in &declaration.fields
                    {
                        if let Some(previous) = fields.iter().find(|item| item.name == field.name)
                        { return Err(error(&field.location, format!("Duplicate field '{}'; first declared at {}", field.name, previous.location))); }
                        let mut type_id = resolve_type(registry, &field.annotation, &names, &field.location)?;
                        if field.optional { type_id = registry.intern(TypeKind::Optional(type_id)); }
                        fields.push(FieldDefinition { name: field.name.clone(), type_id,
                            optional: field.optional, location: field.location.clone() });
                    }
                    registry.finish(names[&declaration.name], TypeKind::Struct(fields));
                },
            _ => {}
        }
    }
    if let Err(message) = registry.check_required_cycles(&local_ids)
    {
        let location = registry.get(local_ids[0]).location.clone().unwrap();
        return Err(error(&location, message));
    }

    for statement in ast.iter_mut()
    {
        match statement
        {
            AstStatement::EnumDeclaration(_) | AstStatement::StructDeclaration(_) | AstStatement::NullStatement
            | AstStatement::AliasStatement(_) | AstStatement::BreakStatement(_)
            | AstStatement::ContinueStatement(_) => {},
            AstStatement::LetStatement(statement) => check_expression(registry, &mut statement.expression, &names)?,
            AstStatement::SetStatement(statement) =>
                {
                    for access in &mut statement.indexes
                    { if let AstAccess::Index(index) = access { check_expression(registry, index, &names)?; } }
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


// Preserve ordinary shell argument splitting when a spaced empty call names
// no type. Declared types take precedence, including forward declarations.
fn split_spaced_call(expression: &mut AstExpression, names: &Names) -> Option<AstExpression>
{
    let receiver = spaced_call_receiver(expression)?;
    if let AstExpressionKind::SpacedEmptyCall(name) = &receiver.kind
        && !names.contains_key(name)
    {
        let name = name.clone();
        let location = receiver.location.clone();
        *receiver = new_ast_literal(location.clone(), Value::None, None);
        return Some(std::mem::replace(expression, new_ast_symbol(location, name, None)));
    }
    None
}


fn spaced_call_receiver(expression: &mut AstExpression) -> Option<&mut AstExpression>
{
    match &mut expression.kind
    {
        AstExpressionKind::SpacedEmptyCall(_) => Some(expression),
        AstExpressionKind::Field(value, _, _) | AstExpressionKind::Index(value, _)
        | AstExpressionKind::Splat(value) | AstExpressionKind::MathExpression(_, value, _)
        | AstExpressionKind::BooleanExpression(_, value, _)
        | AstExpressionKind::Range(Some(value), _, _) => spaced_call_receiver(value),
        _ => None
    }
}


fn check_call(registry: &mut TypeRegistry, call: &mut AstExecuteStatement, names: &Names) -> CompileResult<()>
{
    if let Some(none) = split_spaced_call(&mut call.executable, names)
    { call.arguments.insert(0, none); }
    for mut argument in std::mem::take(&mut call.arguments)
    {
        let none = split_spaced_call(&mut argument, names);
        call.arguments.push(argument);
        if let Some(none) = none { call.arguments.push(none); }
    }
    check_expression(registry, &mut call.executable, names)?;
    if matches!(&call.executable.kind, AstExpressionKind::Literal(AstLiteral { value: Value::Enum(_) }))
    {
        return Err(error(&call.executable.location, "Cannot execute an enum as a command".to_string()));
    }
    if matches!(&call.executable.kind, AstExpressionKind::StructConstructor(_))
    { return Err(error(&call.executable.location, "Cannot execute a struct as a command".to_string())); }
    for argument in &mut call.arguments { check_expression(registry, argument, names)?; }
    Ok(())
}


fn check_expression(registry: &mut TypeRegistry, expression: &mut AstExpression, names: &Names) -> CompileResult<()>
{
    if let Some(argument) = split_spaced_call(expression, names)
    {
        let location = expression.location.clone();
        let executable = std::mem::replace(expression, new_ast_literal(location.clone(), Value::None, None));
        expression.kind = AstExpressionKind::Execute(Box::new(AstExecuteStatement
            { location, executable, expand_path: false, arguments: vec![argument] }));
    }
    match &mut expression.kind
    {
        AstExpressionKind::SpacedEmptyCall(name) =>
            {
                expression.kind = AstExpressionKind::StructConstructor(Box::new(AstStructConstructor
                    { name: name.clone(), fields: Vec::new(), type_id: None, field_indexes: Vec::new() }));
                check_expression(registry, expression, names)?;
            },
        AstExpressionKind::StructConstructor(constructor) =>
            {
                let id = *names.get(&constructor.name).ok_or_else(|| error(&expression.location,
                    format!("Unknown type '{}'", constructor.name)))?;
                let definition = registry.get(id);
                let TypeKind::Struct(fields) = &definition.kind else
                { return Err(error(&expression.location, format!("Type '{}' is not a struct", constructor.name))); };
                let mut supplied = std::collections::HashSet::new();
                for (name, location, value) in &mut constructor.fields
                {
                    let index = fields.iter().position(|field| field.name == *name).ok_or_else(|| error(location,
                        format!("Unknown field '{}.{}'", constructor.name, name)))?;
                    if !supplied.insert(index) { return Err(error(location, format!("Duplicate constructor field '{}'", name))); }
                    check_expression(registry, value, names)?;
                    check_known_value(registry, fields[index].type_id, value).map_err(|message|
                        error(location, format!("Field '{}.{}' (declared at {}): {}", constructor.name, name, fields[index].location, message)))?;
                    constructor.field_indexes.push(index);
                }
                for (index, field) in fields.iter().enumerate()
                {
                    if !field.optional && !supplied.contains(&index)
                    { return Err(error(&expression.location, format!("Missing required field '{}.{}'", constructor.name, field.name))); }
                }
                constructor.type_id = Some(id);
            },
        AstExpressionKind::Field(object, name, index) =>
            {
                check_expression(registry, object, names)?;
                if let Some(id) = known_type(registry, object)
                {
                    let definition = registry.get(id);
                    if matches!(definition.kind, TypeKind::Optional(_))
                        || matches!(definition.kind, TypeKind::Builtin) && definition.name == "any"
                    { return Ok(()); }
                    let TypeKind::Struct(fields) = &definition.kind else
                    { return Err(error(&expression.location, format!("Cannot access field '{}' on {}", name, definition.name))); };
                    *index = Some(fields.iter().position(|field| field.name == *name).ok_or_else(|| error(&expression.location,
                        format!("Unknown field '{}.{}'", definition.name, name)))?);
                }
            },
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


fn resolve_type(registry: &mut TypeRegistry, annotation: &AstType, names: &Names, location: &Location) -> CompileResult<TypeId>
{
    let kind = match annotation
        {
            AstType::Named(name) => return names.get(name).copied().ok_or_else(|| error(location, format!("Unknown type '{}'", name))),
            AstType::Array(inner) => TypeKind::Array(resolve_type(registry, inner, names, location)?),
            AstType::Optional(inner) => TypeKind::Optional(resolve_type(registry, inner, names, location)?),
            AstType::Map(key, value) => TypeKind::Map(resolve_type(registry, key, names, location)?, resolve_type(registry, value, names, location)?)
        };
    Ok(registry.intern(kind))
}

fn known_type(registry: &TypeRegistry, expression: &AstExpression) -> Option<TypeId>
{
    match &expression.kind
    {
        AstExpressionKind::StructConstructor(item) => item.type_id,
        AstExpressionKind::Literal(AstLiteral { value: Value::Struct(item) }) => Some(item.definition.id),
        AstExpressionKind::Literal(AstLiteral { value: Value::Enum(item) }) => Some(item.definition.id),
        AstExpressionKind::Grouped(inner) => known_type(registry, inner),
        AstExpressionKind::Field(object, _, Some(index)) =>
            {
                let definition = registry.get(known_type(registry, object)?);
                let TypeKind::Struct(fields) = &definition.kind else { return None; };
                Some(fields[*index].type_id)
            },
        AstExpressionKind::Index(object, _) => match registry.get(known_type(registry, object)?).kind
            {
                TypeKind::Array(element) => Some(element),
                _ => None
            },
        _ => None
    }
}

fn constant_value(registry: &TypeRegistry, expression: &AstExpression) -> Option<Value>
{
    match &expression.kind
    {
        AstExpressionKind::Literal(literal) =>
            {
                if let Value::String(text, _) = &literal.value
                    && matches!(expression.string_flag, Some(AstStringFlag::Interpolated(_))) && text.contains('$')
                { return None; }
                Some(literal.value.clone())
            },
        AstExpressionKind::Grouped(inner) => constant_value(registry, inner),
        AstExpressionKind::Array(values) => Some(Value::from_array(values.iter()
            .map(|value| constant_value(registry, value)).collect::<Option<Vec<_>>>()?)),
        AstExpressionKind::HashMap(entries) =>
            {
                let mut values = std::collections::HashMap::new();
                for (key, value) in entries
                {
                    values.insert(super::data::map_key::MapKey::from_value(&constant_value(registry, key)?), constant_value(registry, value)?);
                }
                Some(Value::from_hash_map(values))
            },
        AstExpressionKind::StructConstructor(item) =>
            {
                let definition = registry.get(item.type_id?);
                let TypeKind::Struct(fields) = &definition.kind else { return None; };
                let mut values = vec![Value::None; fields.len()];
                for ((_, _, expression), index) in item.fields.iter().zip(&item.field_indexes)
                { values[*index] = constant_value(registry, expression)?; }
                Some(Value::Struct(Rc::new(StructValue { definition, fields: values })))
            },
        _ => None
    }
}

fn check_known_value(registry: &TypeRegistry, expected: TypeId, expression: &AstExpression) -> Result<(), String>
{
    if let Some(value) = constant_value(registry, expression) { return registry.validate(expected, &value); }
    if let Some(actual) = known_type(registry, expression)
        && matches!(registry.get(actual).kind, TypeKind::Struct(_) | TypeKind::Enum(_))
    {
        let definition = registry.get(expected);
        match definition.kind
        {
            TypeKind::Builtin if definition.name == "any" => {},
            TypeKind::Optional(inner) => return check_known_value(registry, inner, expression),
            _ if actual != expected => return Err(format!("Expected {}, got {}", definition.name, registry.get(actual).name)),
            _ => {}
        }
    }
    Ok(())
}
