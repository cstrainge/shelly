
use std::{ collections::{ HashMap, HashSet }, rc::Rc, mem::{ replace, take } };

use crate::language::{ ast::*,
                       compiler::{ CompileError, CompileResult, ErrorWhat },
                       data::{ types::{ EnumValue,
                                        EnumVariant,
                                        FieldDefinition,
                                        StructValue,
                                        TypeId,
                                        TypeKind,
                                        TypeRegistry },
                               value::{ Value, Executable }, scoped_variables::ScopedVariables,
                               map_key::MapKey },
                       text::location::Location };

type Names = HashMap<String, TypeId>;

fn error(location: &Location, message: String) -> CompileError
{
    CompileError { location: Some(location.clone()), what: ErrorWhat::TypeError(message) }
}


// Resolve declarations first, then propagate binding and collection item types.
pub fn check_ast(registry: &mut TypeRegistry, ast: &mut AstTopLevel,
                 variables: &ScopedVariables) -> CompileResult<()>
{
    let names = check_scope(registry, ast, &registry.names.clone())?;
    let bindings = variables.get_all_flattened().into_iter().map(|(name, value)|
        {
            let inferred = if value.reference.is_none()
                { Some(registry.inferred_value_type(&value.value)) } else { None };
            (name, Binding { constraint: value.type_id, inferred, refined: None,
                type_value: if let Value::Type(item) = &value.value
                    { Some(item.id) } else { None } })
        }).collect();
    check_binding_scope(registry, ast, &bindings, None, None)?;
    registry.names = names;
    Ok(())
}


fn check_scope(
    registry: &mut TypeRegistry, ast: &mut AstTopLevel, parent: &Names,
) -> CompileResult<Names>
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
                AstStatement::TypeDeclaration(item) => (&item.name, &item.location),
                _ => continue
            };
        if matches!(name.as_str(), "any" | "optional")
        { return Err(error(location, format!("'{}' is reserved in type annotations", name))); }
        if let Some(previous) = declarations.insert(name, location)
        {
            return Err(error(
                location,
                format!("Duplicate type '{}'; first declared at {}", name, previous),
            ));
        }
        let id = registry.register(name.clone(), TypeKind::Pending, Some(location.clone()));
        names.insert(name.clone(), id);
        local_ids.push(id);
    }
    for statement in ast.iter()
    {
        match statement
        {
            AstStatement::TypeDeclaration(declaration) =>
                {
                    let inner = resolve_type(registry, &declaration.annotation, &names,
                                             &declaration.location)?;
                    registry.finish(names[&declaration.name], TypeKind::Named(inner));
                },
            AstStatement::EnumDeclaration(declaration) =>
                {
                    let mut variants = Vec::<EnumVariant>::new();
                    for (name, location) in &declaration.variants
                    {
                        if let Some(previous) = variants.iter()
                            .find(|variant| variant.name == *name)
                        {
                            return Err(error(
                                location,
                                format!(
                                    "Duplicate variant '{}'; first declared at {}",
                                    name, previous.location
                                ),
                            ));
                        }
                        variants.push(EnumVariant
                            {
                                name: name.clone(),
                                location: location.clone(),
                            });
                    }
                    registry.finish(names[&declaration.name], TypeKind::Enum(variants));
                },
            AstStatement::StructDeclaration(declaration) =>
                {
                    let mut fields = Vec::<FieldDefinition>::new();
                    for field in &declaration.fields
                    {
                        if field.name == "type"
                        {
                            return Err(error(&field.location,
                                "The .type property is reserved".into()));
                        }
                        if let Some(previous) = fields.iter().find(|item| item.name == field.name)
                        {
                            return Err(error(
                                &field.location,
                                format!(
                                    "Duplicate field '{}'; first declared at {}",
                                    field.name, previous.location
                                ),
                            ));
                        }
                        let mut type_id =
                            resolve_type(registry, &field.annotation, &names, &field.location)?;
                        if field.optional
                        {
                            type_id = registry.intern(TypeKind::Optional(type_id));
                        }
                        fields.push(FieldDefinition { name: field.name.clone(), type_id,
                            optional: field.optional, location: field.location.clone() });
                    }
                    registry.finish(names[&declaration.name], TypeKind::Struct(fields));
                },
            _ => {}
        }
    }
    registry.check_named_cycles(&local_ids).map_err(|(id, message)|
        error(&registry.get(id).location.clone().unwrap(), message))?;
    if let Err(message) = registry.check_required_cycles(&local_ids)
    {
        let location = registry.get(local_ids[0]).location.clone().unwrap();
        return Err(error(&location, message));
    }

    // Resolve extension targets before checking uses, including forward declarations.
    for statement in ast.iter_mut()
    {
        if    let AstStatement::FunctionDefinition(function) = statement
           && let Some(receiver) = &function.receiver
        {
            let id = *names.get(receiver).ok_or_else(|| error(&function.location,
                format!("Unknown type '{}'", receiver)))?;
            let definition = registry.get(registry.underlying_type(id));
            let collision = function.name == "type" || match &definition.kind
                {
                    TypeKind::Struct(fields) => fields.iter()
                        .any(|field| field.name == function.name),
                    TypeKind::Enum(_) => function.name == "index",
                    _ => false,
                };
            if collision
            {
                return Err(error(&function.location,
                    format!("Method '{}::{}' conflicts with a data member", receiver,
                        function.name)));
            }
            function.receiver_type = Some(id);
            registry.declare_extension(id, &function.name);
        }
    }

    for statement in ast.iter_mut()
    {
        if let AstStatement::ExecuteStatement(call) = statement
            && call.arguments.is_empty()
            && let AstExpressionKind::Symbol(item) = &call.executable.kind
            && let Some(id) = names.get(&item.name)
        {
            *statement = AstStatement::ExpressionStatement(new_ast_literal(
                call.location.clone(), Value::Type(registry.get(*id)), None));
        }
        match statement
        {
            AstStatement::ImportStatement(import) => return Err(error(&import.location,
                "Imports are only allowed at module top level".into())),
            AstStatement::EnumDeclaration(_)
            | AstStatement::StructDeclaration(_)
            | AstStatement::TypeDeclaration(_)
            | AstStatement::NullStatement
            | AstStatement::AliasStatement(_)
            | AstStatement::BreakStatement(_)
            | AstStatement::ContinueStatement(_) => {},
            AstStatement::LetStatement(statement) =>
                {
                    if let Some(annotation) = &statement.annotation
                    {
                        let id = resolve_type(registry, annotation, &names, &statement.location)?;
                        statement.type_id = Some(id);
                        if statement.default_initialize
                        {
                            let value = registry
                                .default_value(id)
                                .map_err(|message| error(&statement.location, message))?;
                            statement.expression =
                                new_ast_literal(statement.location.clone(), value, None);
                        }
                    }
                    check_expression(registry, &mut statement.expression, &names)?;
                    if let Some(id) = statement.type_id
                    {
                        check_known_value(registry, id, &statement.expression)
                            .map_err(|message| error(&statement.location,
                                                     format!("Variable '{}': {}",
                                                             statement.identifier, message)))?;
                    }
                },
            AstStatement::SetStatement(statement) =>
                {
                    for access in &mut statement.indexes
                    {
                        if matches!(access, AstAccess::Field(name) if name == "type")
                        {
                            return Err(error(&statement.location,
                                "The .type property is read-only".into()));
                        }
                        if let AstAccess::Index(index) = access
                        {
                            check_expression(registry, index, &names)?;
                        }
                    }
                    check_expression(registry, &mut statement.expression, &names)?;
                },
            AstStatement::ExpressionStatement(expression)
            | AstStatement::DiscardStatement(expression) =>
                check_expression(registry, expression, &names)?,
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
                    check_function(registry, statement, &names)?;
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


fn check_function(registry: &mut TypeRegistry, function: &mut AstFunctionStatement,
                  names: &Names) -> CompileResult<()>
{
    let mut optional_seen = false;
    for parameter in &mut function.parameters
    {
        if let Some(annotation) = &parameter.annotation
        {
            let id = resolve_type(registry, annotation, names,
                                  &parameter.location)?;
            parameter.type_id = Some(id);
            let definition = registry.get(id);
            parameter.optional = matches!(definition.kind, TypeKind::Optional(_));
            if    parameter.variadic
               && !matches!(definition.kind, TypeKind::Array(_))
               && !(matches!(definition.kind, TypeKind::Builtin)
                    && matches!(definition.name.as_str(), "Array" | "any"))
            {
                return Err(error(
                    &parameter.location,
                    "A variadic parameter requires an array type or any"
                        .to_string(),
                ));
            }
        }
        if optional_seen && !parameter.optional && !parameter.variadic
        {
            return Err(error(
                &parameter.location,
                "Required parameters cannot follow optional parameters".to_string(),
            ));
        }
        optional_seen |= parameter.optional;
    }
    if let Some(annotation) = &function.return_annotation
    {
        function.return_type = Some(resolve_type(
            registry,
            annotation,
            names,
            &function.location,
        )?);
    }
    if function.receiver.is_some() && function.name == "next_item"
    {
        if function.parameters.len() != 1
        { return Err(error(&function.location,
            "next_item must accept no arguments beyond its receiver".into())); }
        if !function.return_type.is_some_and(|id|
            matches!(registry.get(id).kind, TypeKind::Optional(_)))
        { return Err(error(&function.location,
            "next_item must declare a return type T | ()".into())); }
    }
    check_scope(registry, &mut function.body, names)?;
    Ok(())
}


// Preserve ordinary shell argument splitting when a spaced empty call names
// no type. Declared types take precedence, including forward declarations.
fn split_spaced_call(expression: &mut AstExpression, names: &Names) -> Option<AstExpression>
{
    let receiver = spaced_call_receiver(expression)?;
    if    let AstExpressionKind::SpacedEmptyCall(name) = &receiver.kind
       && !names.contains_key(name)
    {
        let name = name.clone();
        let location = receiver.location.clone();
        *receiver = new_ast_literal(location.clone(), Value::None, None);
        return Some(replace(expression, new_ast_symbol(location, name, None)));
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


fn check_call(
    registry: &mut TypeRegistry, call: &mut AstExecuteStatement, names: &Names,
) -> CompileResult<()>
{
    if let Some(none) = split_spaced_call(&mut call.executable, names)
    { call.arguments.insert(0, none); }
    for mut argument in take(&mut call.arguments)
    {
        let none = split_spaced_call(&mut argument, names);
        call.arguments.push(argument);
        if let Some(none) = none { call.arguments.push(none); }
    }
    // Command words retain their shell meaning, including names of types.
    if !matches!(call.executable.kind, AstExpressionKind::Symbol(_))
    { check_expression(registry, &mut call.executable, names)?; }
    if matches!(
        &call.executable.kind,
        AstExpressionKind::Literal(AstLiteral
        {
            value: Value::Enum(_)
        })
    )
    {
        return Err(error(
            &call.executable.location,
            "Cannot execute an enum as a command".to_string(),
        ));
    }
    if matches!(
        &call.executable.kind,
        AstExpressionKind::StructConstructor(_)
    )
    {
        return Err(error(
            &call.executable.location,
            "Cannot execute a struct as a command".to_string(),
        ));
    }
    for argument in &mut call.arguments
    {
        if !matches!(argument.kind, AstExpressionKind::Symbol(_))
        { check_expression(registry, argument, names)?; }
    }
    Ok(())
}


fn check_expression(
    registry: &mut TypeRegistry, expression: &mut AstExpression, names: &Names,
) -> CompileResult<()>
{
    // A type name is a value in expression positions. Literal command words are
    // handled by check_call; quoted names remain ordinary strings.
    let type_name = match &expression.kind
        {
            AstExpressionKind::Symbol(item) => Some(item.name.clone()),
            AstExpressionKind::EnumVariant(owner, member) => Some(format!("{}::{}", owner, member)),
            AstExpressionKind::TryExecute(inner) =>
                if let AstExpressionKind::Symbol(item) = &inner.kind
                    { Some(item.name.clone()) } else { None },
            _ => None,
        };
    if let Some(id) = type_name.as_ref().and_then(|name| names.get(name))
    {
        expression.kind = AstExpressionKind::Literal(AstLiteral
            { value: Value::Type(registry.get(*id)) });
        expression.string_flag = None;
        return Ok(());
    }
    if let Some(argument) = split_spaced_call(expression, names)
    {
        let location = expression.location.clone();
        let executable = replace(expression, new_ast_literal(location.clone(), Value::None, None));
        expression.kind = AstExpressionKind::Execute(Box::new(AstExecuteStatement
            { location, executable, expand_path: false, arguments: vec![argument] }));
    }
    match &mut expression.kind
    {
        AstExpressionKind::AnonymousFunction(function) =>
            check_function(registry, function, names)?,
        AstExpressionKind::TypeConversion(name, value, type_id) =>
            {
                let id = *names.get(name).ok_or_else(|| error(&expression.location,
                    format!("Unknown type '{}'", name)))?;
                if !matches!(registry.get(id).kind,
                    TypeKind::Builtin | TypeKind::Named(_)
                    | TypeKind::Struct(_) | TypeKind::Enum(_))
                {
                    return Err(error(&expression.location,
                        format!("The shadowed type '{}' does not support positional conversion",
                                name)));
                }
                *type_id = Some(id);
                check_expression(registry, value, names)?;
                if    matches!(registry.get(id).kind, TypeKind::Struct(_) | TypeKind::Enum(_))
                   && constant_value(registry, value)
                        .is_some_and(|value| !matches!(value, Value::Named(_)))
                {
                    let message = if TypeRegistry::is_builtin_name(name)
                        { format!("The shadowed type '{}' does not support positional conversion",
                            name) }
                        else { format!("Type '{}' has no positional constructor", name) };
                    return Err(error(&expression.location, message));
                }
            },
        AstExpressionKind::SpacedEmptyCall(name) =>
            {
                expression.kind =
                    AstExpressionKind::StructConstructor(Box::new(AstStructConstructor
                        {
                            name: name.clone(),
                            fields: Vec::new(),
                            type_id: None,
                            field_indexes: Vec::new(),
                        }));
                check_expression(registry, expression, names)?;
            },
        AstExpressionKind::StructConstructor(constructor) =>
            {
                let id = *names.get(&constructor.name).ok_or_else(|| error(&expression.location,
                    format!("Unknown type '{}'", constructor.name)))?;
                let definition = registry.get(id);
                if matches!(definition.kind, TypeKind::Builtin)
                {
                    return Err(error(&expression.location,
                        format!("{} conversion requires exactly one positional value",
                                definition.name)));
                }
                let TypeKind::Struct(fields) = &definition.kind
                else
                {
                    return Err(error(
                        &expression.location,
                        format!("Type '{}' is not a struct", constructor.name),
                    ));
                };
                let mut supplied = HashSet::new();
                for (name, location, value) in &mut constructor.fields
                {
                    let index = fields.iter().position(|field| field.name == *name)
                        .ok_or_else(|| error(location,
                        format!("Unknown field '{}.{}'", constructor.name, name)))?;
                    if !supplied.insert(index)
                    {
                        return Err(error(
                            location,
                            format!("Duplicate constructor field '{}'", name),
                        ));
                    }
                    check_expression(registry, value, names)?;
                    check_known_value(registry, fields[index].type_id, value).map_err(|message|
                        error(location,
                              format!("Field '{}.{}' (declared at {}): {}", constructor.name, name,
                                      fields[index].location, message)))?;
                    constructor.field_indexes.push(index);
                }
                for (index, field) in fields.iter().enumerate()
                {
                    if !field.optional && !supplied.contains(&index)
                    {
                        return Err(error(
                            &expression.location,
                            format!(
                                "Missing required field '{}.{}'",
                                constructor.name, field.name
                            ),
                        ));
                    }
                }
                constructor.type_id = Some(id);
            },
        AstExpressionKind::Field(object, name, index) =>
            {
                check_expression(registry, object, names)?;
                if name == "type" { return Ok(()); }
                if let Some(id) = known_type(registry, object)
                {
                    if registry.method(id, name).is_some() { return Ok(()); }
                    let definition = registry.get(registry.underlying_type(id));
                    if matches!(definition.kind, TypeKind::Enum(_)) && name == "index"
                    { return Ok(()); }
                    if    registry.has_extension(id, name)
                       && !matches!(&definition.kind, TypeKind::Struct(fields)
                            if fields.iter().any(|field| field.name == *name))
                    { return Ok(()); }
                    if    matches!(definition.kind, TypeKind::Optional(_) | TypeKind::Union(_))
                       || matches!(definition.kind, TypeKind::Builtin)
                       && definition.name == "any"
                    { return Ok(()); }
                    let TypeKind::Struct(fields) = &definition.kind
                    else
                    {
                        return Err(error(
                            &expression.location,
                            format!("Cannot access field '{}' on {}", name, definition.name),
                        ));
                    };
                    *index =
                        Some(fields.iter().position(|field| field.name == *name)
                            .ok_or_else(|| error(&expression.location,
                        format!("Unknown field '{}.{}'", definition.name, name)))?);
                }
            },
        AstExpressionKind::EnumVariant(name, variant) =>
            {
                let qualified = format!("{}::{}", name, variant);
                if    !names.contains_key(name)
                   && (registry.qualified_functions.contains(&qualified)
                    || registry.module_names.contains(name.split("::").next().unwrap()))
                {
                    expression.kind = AstExpressionKind::Symbol(AstSymbol { name: qualified });
                    return Ok(());
                }
                let id = names.get(name).ok_or_else(|| error(&expression.location,
                    format!("Unknown type or module member '{}'", qualified)))?;
                let definition = registry.get(registry.underlying_type(*id));
                let TypeKind::Enum(variants) = &definition.kind else
                {
                    return Err(error(
                        &expression.location,
                        format!("Type '{}' is not an enum", name),
                    ));
                };
                let index = variants.iter().position(|item| item.name == *variant)
                    .ok_or_else(|| error(&expression.location,
                        format!("Unknown variant '{}::{}' (enum declared at {})", name, variant,
                            definition.location.as_ref().unwrap())))?;
                let value = Value::Enum(Rc::new(EnumValue { definition, variant: index }));
                let value = registry.coerce(*id, value)
                    .map_err(|message| error(&expression.location, message))?;
                expression.kind = AstExpressionKind::Literal(AstLiteral { value });
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
        AstExpressionKind::ExecutableReference(value) =>
            {
                check_expression(registry, value, names)?;
                if matches!(&value.kind, AstExpressionKind::Literal(literal)
                    if matches!(literal.value, Value::Enum(_)))
                {
                    return Err(error(&expression.location,
                        "Cannot execute an enum as a command".into()));
                }
            },
        AstExpressionKind::Splat(value) | AstExpressionKind::Grouped(value)
        | AstExpressionKind::TryExecute(value)
        | AstExpressionKind::MathNegate(value) | AstExpressionKind::BooleanNot(value) =>
            check_expression(registry, value, names)?,
        AstExpressionKind::Execute(call) => check_call(registry, call, names)?,
        AstExpressionKind::Redirect(source, redirects) =>
            {
                check_expression(registry, source, names)?;
                for redirect in redirects
                { check_expression(registry, &mut redirect.target, names)?; }
            },
        AstExpressionKind::IfExpression(conditional) =>
            {
                for branch in &mut conditional.branches
                {
                    check_expression(registry, &mut branch.condition, names)?;
                    check_scope(registry, &mut branch.body.body, names)?;
                }
                if let Some(block) = &mut conditional.else_body
                {
                    check_scope(registry, &mut block.body, names)?;
                }
            },
        AstExpressionKind::MatchExpression(matching) =>
            {
                check_expression(registry, &mut matching.value, names)?;
                for arm in &mut matching.arms
                {
                    if let Some(pattern) = &mut arm.pattern
                    { check_expression(registry, pattern, names)?; }
                    check_scope(registry, &mut arm.body.body, names)?;
                }
            },
        AstExpressionKind::Variable(_) | AstExpressionKind::VariableSplat(_)
        | AstExpressionKind::Symbol(_) | AstExpressionKind::Literal(_) => {}
    }
    Ok(())
}


fn resolve_type(
    registry: &mut TypeRegistry, annotation: &AstType, names: &Names, location: &Location,
) -> CompileResult<TypeId>
{
    let kind = match annotation
        {
            AstType::Function(parameters, result) => TypeKind::Function(
                parameters.iter().map(|item| resolve_type(registry, item, names, location))
                    .collect::<CompileResult<Vec<_>>>()?,
                resolve_type(registry, result, names, location)?),
            AstType::Named(name) =>
                return names.get(name).copied()
                    .ok_or_else(|| error(location, format!("Unknown type '{}'", name))),
            AstType::Array(inner) =>
                TypeKind::Array(resolve_type(registry, inner, names, location)?),
            AstType::FixedArray(items) => TypeKind::FixedArray(items.iter()
                .map(|item| resolve_type(registry, item, names, location))
                .collect::<CompileResult<Vec<_>>>()?),
            AstType::Optional(inner) =>
                TypeKind::Optional(resolve_type(registry, inner, names, location)?),
            AstType::Union(members) =>
                {
                    let members = members.iter()
                        .map(|item| resolve_type(registry, item, names, location))
                        .collect::<CompileResult<Vec<_>>>()?;
                    return Ok(registry.union_type(members));
                },
            AstType::Map(key, value) => TypeKind::Map(
                resolve_type(registry, key, names, location)?,
                resolve_type(registry, value, names, location)?,
            )
        };
    Ok(registry.intern(kind))
}

fn anonymous_type(registry: &TypeRegistry, function: &AstFunctionStatement) -> Option<TypeId>
{
    if function.parameters.iter().any(|item| item.optional || item.variadic) { return None; }
    let any = registry.builtin_id("any").unwrap();
    Some(registry.intern(TypeKind::Function(function.parameters.iter()
        .map(|item| item.type_id.unwrap_or(any)).collect(), function.return_type.unwrap_or(any))))
}


fn known_type(registry: &TypeRegistry, expression: &AstExpression) -> Option<TypeId>
{
    match &expression.kind
    {
        AstExpressionKind::AnonymousFunction(function) => anonymous_type(registry, function),
        AstExpressionKind::Array(_) => registry.builtin_id("Array"),
        AstExpressionKind::HashMap(_) => registry.builtin_id("HashMap"),
        AstExpressionKind::Symbol(symbol) if symbol.is_glob() =>
            registry.builtin_id("ArgumentExpansion"),
        AstExpressionKind::StructConstructor(item) => item.type_id,
        AstExpressionKind::TypeConversion(_, _, type_id) => *type_id,
        AstExpressionKind::Literal(AstLiteral
        {
            value: Value::Struct(item),
        }) => Some(item.definition.id),
        AstExpressionKind::Literal(AstLiteral
        {
            value: Value::Enum(item),
        }) => Some(item.definition.id),
        AstExpressionKind::Literal(AstLiteral
        {
            value: Value::Named(item),
        }) => Some(item.definition.id),
        AstExpressionKind::Grouped(inner) => known_type(registry, inner),
        AstExpressionKind::Field(object, name, _) =>
            {
                if name == "type" { return registry.builtin_id("Type"); }
                let id = known_type(registry, object)?;
                let definition = registry.get(registry.underlying_type(id));
                if matches!(definition.kind, TypeKind::Enum(_)) && name == "index"
                { return registry.builtin_id("Integer"); }
                if let TypeKind::Struct(fields) = &definition.kind
                {
                    if let Some(field) = fields.iter().find(|field| field.name == *name)
                    { return Some(field.type_id); }
                }
                // User definitions are versioned and scoped. Their runtime signature
                // validates the result; a builtin signature cannot describe an override.
                if registry.has_extension(id, name) { return None; }
                registry.method(id, name).map(|method| method.return_type)
            },
        AstExpressionKind::Index(object, _) =>
            match registry.get(known_type(registry, object)?).kind
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
        AstExpressionKind::Field(object, name, _) if name == "index" =>
            {
                let Value::Enum(item) = constant_value(registry, object)? else { return None; };
                Some(Value::Integer(item.variant as i64))
            },
        AstExpressionKind::TypeConversion(name, value, Some(id))
            if !matches!(name.as_str(), "Array" | "ArgumentExpansion") =>
            registry.convert(*id, &constant_value(registry, value)?).ok(),
        AstExpressionKind::Literal(literal) =>
            {
                // Names bind to a particular callable only after function compilation.
                if matches!(literal.value, Value::String(_, Executable::Yes)) { return None; }
                if    let Value::String(text, _) = &literal.value
                   && matches!(expression.string_flag, Some(AstStringFlag::Interpolated(_)))
                   && text.contains('$')
                { return None; }
                Some(literal.value.clone())
            },
        AstExpressionKind::Grouped(inner) => constant_value(registry, inner),
        AstExpressionKind::Array(values) => Some(Value::from_array(values.iter()
            .map(|value| constant_value(registry, value)).collect::<Option<Vec<_>>>()?)),
        AstExpressionKind::HashMap(entries) =>
            {
                let mut values = HashMap::new();
                for (key, value) in entries
                {
                    values.insert(
                        MapKey::from_value(&constant_value(registry, key)?),
                        constant_value(registry, value)?,
                    );
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

fn check_known_value(
    registry: &TypeRegistry, expected: TypeId, expression: &AstExpression,
) -> Result<(), String>
{
    if let Some(value) = constant_value(registry, expression)
    {
        return registry.coerce(expected, value).map(|_| ());
    }
    if    let Some(actual) = known_type(registry, expression)
       && matches!(registry.get(actual).kind, TypeKind::Struct(_) | TypeKind::Enum(_))
    {
        let definition = registry.get(expected);
        match definition.kind
        {
            TypeKind::Builtin if definition.name == "any" => {},
            TypeKind::Optional(inner) => return check_known_value(registry, inner, expression),
            _ if !registry.may_assign(expected, actual) =>
                return Err(format!("Expected {}, got {}", definition.name,
                                   registry.get(actual).name)),
            _ => {}
        }
    }
    Ok(())
}


// Binding constraints follow lexical declarations within a body. Named functions get
// fresh binding information because their free variables use dynamic scope at runtime.
// Closures retain capture constraints but discard mutable inferred types.
// Runtime guards remain authoritative for calls, writes, and all return paths.
#[derive(Clone, Copy)]
struct Binding
{
    constraint: Option<TypeId>,
    inferred: Option<TypeId>,
    refined: Option<TypeId>,
    type_value: Option<TypeId>,
}

impl Binding
{
    fn declared(constraint: Option<TypeId>) -> Self
    { Self { constraint, inferred: None, refined: None, type_value: None } }

    fn type_id(self) -> Option<TypeId>
    { self.refined.or(self.constraint).or(self.inferred) }
}

type Bindings = HashMap<String, Binding>;

fn forget_inferred(bindings: &mut Bindings)
{
    for binding in bindings.values_mut()
    { binding.inferred = None; binding.refined = None; binding.type_value = None; }
}

fn ungroup(expression: &AstExpression) -> &AstExpression
{
    if let AstExpressionKind::Grouped(inner) = &expression.kind { ungroup(inner) }
    else { expression }
}

fn type_value(expression: &AstExpression, bindings: &Bindings) -> Option<TypeId>
{
    match &ungroup(expression).kind
    {
        AstExpressionKind::Literal(AstLiteral { value: Value::Type(item) }) => Some(item.id),
        AstExpressionKind::Variable(item) => bindings.get(&item.name)?.type_value,
        _ => None,
    }
}

fn refined_receiver(expression: &AstExpression, bindings: &Bindings) -> bool
{
    match &ungroup(expression).kind
    {
        AstExpressionKind::Variable(item) =>
            bindings.get(&item.name).is_some_and(|binding| binding.refined.is_some()),
        AstExpressionKind::Field(value, _, _) | AstExpressionKind::Index(value, _) =>
            refined_receiver(value, bindings),
        _ => false,
    }
}

fn type_subject(expression: &AstExpression) -> Option<&str>
{
    if let AstExpressionKind::Field(value, name, _) = &ungroup(expression).kind
        && name == "type"
        && let AstExpressionKind::Variable(item) = &value.kind
    { Some(&item.name) }
    else { None }
}

fn runtime_alternatives(registry: &TypeRegistry, id: TypeId, items: &mut Vec<TypeId>)
{
    let definition = registry.get(id);
    match &definition.kind
    {
        TypeKind::Union(members) =>
            { for member in members { runtime_alternatives(registry, *member, items); } },
        TypeKind::Optional(inner) =>
            {
                runtime_alternatives(registry, *inner, items);
                items.push(registry.builtin_id("None").unwrap());
            },
        TypeKind::Builtin if definition.name == "Number" =>
            items.extend([registry.builtin_id("Integer").unwrap(),
                          registry.builtin_id("Float").unwrap()]),
        _ => items.push(id),
    }
}

fn exact_runtime_type(registry: &TypeRegistry, id: TypeId) -> TypeId
{
    match registry.get(id).kind
    {
        TypeKind::Array(_) | TypeKind::FixedArray(_) => registry.builtin_id("Array").unwrap(),
        TypeKind::Map(_, _) => registry.builtin_id("HashMap").unwrap(),
        _ => id,
    }
}

// Type equality preserves nominal identity. Refining Array/HashMap retains any
// stronger collection constraints already known about the same runtime value.
fn refine_type(registry: &TypeRegistry, bindings: &mut Bindings,
                name: &str, target: TypeId, matched: bool)
{
    let binding = bindings.entry(name.to_string()).or_insert(Binding::declared(None));
    let mut alternatives = Vec::new();
    if let Some(current) = binding.type_id()
    { runtime_alternatives(registry, current, &mut alternatives); }
    let retained: Vec<_> = alternatives.into_iter()
        .filter(|id| (exact_runtime_type(registry, *id) == target) == matched).collect();
    if matched
    {
        binding.refined = Some(if retained.is_empty() { target }
            else { registry.union_type(retained) });
    }
    else if !retained.is_empty() { binding.refined = Some(registry.union_type(retained)); }
}

fn common_bindings(mut left: Bindings, right: &Bindings) -> Bindings
{
    for (name, binding) in &mut left
    {
        let other = right.get(name).copied().unwrap_or(Binding::declared(None));
        if binding.refined != other.refined { binding.refined = None; }
        if binding.inferred != other.inferred { binding.inferred = None; }
        if binding.type_value != other.type_value { binding.type_value = None; }
    }
    left
}

fn condition_bindings(registry: &TypeRegistry, expression: &AstExpression,
                       bindings: &Bindings, truth: bool) -> Bindings
{
    match &ungroup(expression).kind
    {
        AstExpressionKind::BooleanNot(inner) =>
            condition_bindings(registry, inner, bindings, !truth),
        AstExpressionKind::BooleanExpression(operator, left, right)
            if matches!(operator, AstBooleanOperator::And | AstBooleanOperator::Or) =>
            {
                let first = matches!(operator, AstBooleanOperator::And);
                let continuing = condition_bindings(registry, left, bindings, first);
                let continued = condition_bindings(registry, right, &continuing, truth);
                if truth == first { return continued; }
                let stopped = condition_bindings(registry, left, bindings, truth);
                common_bindings(stopped, &continued)
            },
        AstExpressionKind::BooleanExpression(operator, left, right)
            if matches!(operator, AstBooleanOperator::Equal | AstBooleanOperator::NotEqual) =>
            {
                let mut selected = bindings.clone();
                if expression_may_mutate(registry, expression, bindings)
                { forget_inferred(&mut selected); }
                let subject = type_subject(left).zip(type_value(right, &selected))
                    .or_else(|| type_subject(right).zip(type_value(left, &selected)));
                if let Some((name, id)) = subject
                {
                    refine_type(registry, &mut selected, name, id,
                        truth == matches!(operator, AstBooleanOperator::Equal));
                }
                selected
            },
        _ =>
            {
                let mut selected = bindings.clone();
                if expression_may_mutate(registry, expression, bindings)
                { forget_inferred(&mut selected); }
                selected
            },
    }
}


fn check_binding_scope(registry: &TypeRegistry, ast: &AstTopLevel, parent: &Bindings,
                       return_type: Option<TypeId>, tail_type: Option<TypeId>) -> CompileResult<()>
{
    let mut bindings = parent.clone();
    let last = ast.iter().rposition(|statement| !matches!(statement, AstStatement::NullStatement));
    let mut reachable = true;
    for (index, statement) in ast.iter().enumerate()
    {
        let expected = if reachable && last == Some(index) { tail_type } else { None };
        match statement
        {
            AstStatement::LetStatement(item) =>
                {
                    check_binding_expression(
                        registry,
                        &item.expression,
                        &bindings,
                        return_type,
                        item.type_id,
                    )?;
                    check_stored_string(registry, &item.expression, &bindings, item.type_id)?;
                    let inferred = binding_type(registry, &item.expression, &bindings);
                    if expression_may_mutate(registry, &item.expression, &bindings)
                    { forget_inferred(&mut bindings); }
                    bindings.insert(item.identifier.clone(), Binding
                        { constraint: item.type_id, inferred, refined: None,
                          type_value: type_value(&item.expression, &bindings) });
                },
            AstStatement::SetStatement(item) =>
                {
                    for access in &item.indexes
                    {
                        if let AstAccess::Index(index) = access
                        {
                            if expression_may_mutate(registry, index, &bindings)
                            { forget_inferred(&mut bindings); }
                            check_binding_expression(registry, index, &bindings, return_type,
                                                     None)?;
                        }
                    }
                    let constraint = if item.indexes.is_empty()
                        { bindings.get(&item.identifier).and_then(|item| item.constraint) }
                        else { None };
                    check_binding_expression(
                        registry,
                        &item.expression,
                        &bindings,
                        return_type,
                        constraint,
                    )?;
                    check_stored_string(registry, &item.expression, &bindings, constraint)?;
                    let inferred = if item.indexes.is_empty()
                        { binding_type(registry, &item.expression, &bindings) } else { None };
                    let type_value = if item.indexes.is_empty()
                        { type_value(&item.expression, &bindings) } else { None };
                    // Imports can give one mutable binding multiple visible names.
                    forget_inferred(&mut bindings);
                    if let Some(binding) = bindings.get_mut(&item.identifier)
                    { binding.inferred = inferred; binding.type_value = type_value; }
                },
            AstStatement::ExpressionStatement(expression) =>
                {
                    let result = implicit_callable_result(registry, expression, &bindings);
                    check_binding_expression(registry, expression, &bindings, return_type,
                                             if result.is_some() { None } else { expected })?;
                    if let (Some(expected), Some(actual)) = (expected, result)
                        && !registry.may_assign(expected, actual)
                    {
                        return Err(error(&expression.location, format!("Expected {}, got {}",
                            registry.get(expected).name, registry.get(actual).name)));
                    }
                    if    expression_may_mutate(registry, expression, &bindings)
                       || matches!(expression.kind,
                            AstExpressionKind::Variable(_) | AstExpressionKind::Index(_, _))
                    { forget_inferred(&mut bindings); }
                },
            AstStatement::DiscardStatement(expression) =>
                {
                    check_binding_expression(registry, expression, &bindings, return_type,
                                             None)?;
                    if expression_may_mutate(registry, expression, &bindings)
                    { forget_inferred(&mut bindings); }
                },
            AstStatement::ExecuteStatement(call) =>
                {
                    check_binding_call(registry, call, &bindings, return_type)?;
                    forget_inferred(&mut bindings);
                },
            AstStatement::ReturnStatement(item) =>
                {
                    if let Some(expression) = &item.expression
                    {
                        check_binding_expression(registry, expression, &bindings, return_type,
                                                 return_type)?;
                    }
                    else if let Some(id) = return_type
                    {
                        registry.validate(id, &Value::None).map_err(|message|
                            error(&item.location, format!("Return value: {}", message)))?;
                    }
                    reachable = false;
                },
            AstStatement::FunctionDefinition(item) =>
                {
                    let parameters = item
                        .parameters
                        .iter()
                        .map(|parameter|
                            (parameter.name.clone(), Binding::declared(parameter.type_id)))
                        .collect();
                    check_binding_scope(
                        registry,
                        &item.body,
                        &parameters,
                        item.return_type,
                        item.return_type,
                    )?;
                    if    item.body.iter()
                        .all(|statement| matches!(statement, AstStatement::NullStatement))
                       && let Some(id) = item.return_type
                    {
                        registry.validate(id, &Value::None).map_err(|message|
                            error(&item.location,
                                  format!("Return value of '{}': {}", item.name, message)))?;
                    }
                },
            AstStatement::BlockStatement(block) =>
                {
                    check_binding_scope(registry, &block.body, &bindings, return_type, expected)?;
                    forget_inferred(&mut bindings);
                },
            AstStatement::LoopStatement(block) =>
                {
                    forget_inferred(&mut bindings);
                    check_binding_scope(registry, &block.body, &bindings, return_type, None)?;
                },
            AstStatement::ForStatement(item) =>
                {
                    check_binding_expression(registry, &item.iterable, &bindings, return_type,
                                             None)?;
                    let mut loop_bindings = bindings.clone();
                    // Outer inferred values may change on a previous loop iteration.
                    // The private iterator supplies fresh item bindings on every step.
                    forget_inferred(&mut loop_bindings);
                    let item_type = iterator_item_type(registry, &item.iterable, &bindings);
                    for (index, name) in item.bindings.iter().enumerate()
                    {
                        let inferred = if item.destructure || item.bindings.len() > 1
                            {
                                item_type.filter(|id| matches!(registry
                                    .get(registry.underlying_type(*id)).kind,
                                    TypeKind::Array(_) | TypeKind::FixedArray(_)))
                                    .and_then(|id| indexed_type(registry, id, Some(index)))
                            }
                            else { item_type };
                        loop_bindings.insert(name.clone(), Binding
                            { constraint: None, inferred, refined: None, type_value: None });
                    }
                    check_binding_scope(registry, &item.body.body, &loop_bindings, return_type,
                                        None)?;
                    forget_inferred(&mut bindings);
                },
            AstStatement::ConditionalLoopStatement(item) =>
                {
                    forget_inferred(&mut bindings);
                    check_binding_expression(registry, &item.condition, &bindings, return_type,
                                             None)?;
                    let selected = condition_bindings(registry, &item.condition,
                                                      &bindings, !item.until);
                    check_binding_scope(registry, &item.body.body, &selected, return_type, None)?;
                },
            _ => {}
        }
    }
    Ok(())
}


fn check_binding_call(registry: &TypeRegistry, call: &AstExecuteStatement, bindings: &Bindings,
                      return_type: Option<TypeId>) -> CompileResult<()>
{
    check_binding_expression(registry, &call.executable, bindings, return_type, None)?;
    let mut current = bindings.clone();
    if expression_may_mutate(registry, &call.executable, &current)
    { forget_inferred(&mut current); }
    for argument in &call.arguments
    {
        check_binding_expression(registry, argument, &current, return_type, None)?;
        if expression_may_mutate(registry, argument, &current)
        { forget_inferred(&mut current); }
    }
    Ok(())
}


// A stored string is data even when it happens to name an executable. Only
// expression statements have the implicit execution caveat used below.
fn check_stored_string(registry: &TypeRegistry, expression: &AstExpression,
                       bindings: &Bindings, expected: Option<TypeId>) -> CompileResult<()>
{
    if expression_may_mutate(registry, expression, bindings) { return Ok(()); }
    if    let Some(expected) = expected
       && let Some(actual) = binding_type(registry, expression, bindings)
       && registry.get(actual).name == "String"
       && !registry.may_assign(expected, actual)
    {
        return Err(error(&expression.location, format!("Expected {}, got String",
            registry.get(expected).name)));
    }
    Ok(())
}


fn check_binding_expression(
    registry: &TypeRegistry, expression: &AstExpression, bindings: &Bindings,
    return_type: Option<TypeId>, expected: Option<TypeId>,
) -> CompileResult<()>
{
    if let AstExpressionKind::Field(value, name, _) = &expression.kind
        && name != "type" && refined_receiver(value, bindings)
        && let Some(id) = binding_type(registry, value, bindings)
    {
        let definition = registry.get(registry.underlying_type(id));
        if    !matches!(definition.kind, TypeKind::Optional(_) | TypeKind::Union(_))
           && definition.name != "any"
           && registry.method(id, name).is_none()
           && !registry.has_extension(id, name)
           && !(matches!(definition.kind, TypeKind::Enum(_)) && name == "index")
           && !matches!(&definition.kind, TypeKind::Struct(fields)
                if fields.iter().any(|field| field.name == *name))
        {
            return Err(error(&expression.location,
                format!("Cannot access field '{}' on {}", name, definition.name)));
        }
    }
    let mut stable;
    let bindings = if expression_may_mutate(registry, expression, bindings)
        {
            stable = bindings.clone();
            // Preserve entry facts until the expression actually evaluates a call.
            // Sequential operands and short-circuit guards handle invalidation below.
            for binding in stable.values_mut() { binding.inferred = None; }
            &stable
        } else { bindings };
    if let Some(id) = expected
    {
        check_known_value(registry, id, expression)
            .map_err(|message| error(&expression.location, message))?;
        if    let Some(actual) = binding_type(registry, expression, bindings)
            // A string in an implicit return may execute and yield another type.
           && (refined_receiver(expression, bindings)
                || registry.validate(actual, &Value::from_string(String::new())).is_err())
           && !registry.may_assign(id, actual)
        {
            return Err(error(&expression.location, format!("Expected {}, got {}",
                registry.get(id).name, registry.get(actual).name)));
        }
    }
    match &expression.kind
    {
        AstExpressionKind::AnonymousFunction(function) =>
            {
                let mut captured = bindings.clone();
                forget_inferred(&mut captured);
                for parameter in &function.parameters
                {
                    captured.insert(parameter.name.clone(), Binding::declared(parameter.type_id));
                }
                check_binding_scope(registry, &function.body, &captured,
                                    function.return_type, function.return_type)?;
                if function.body.iter().all(|item| matches!(item, AstStatement::NullStatement))
                    && let Some(id) = function.return_type
                {
                    registry.validate(id, &Value::None)
                        .map_err(|message| error(&function.location, message))?;
                }
            },
        AstExpressionKind::IfExpression(item) =>
            {
                let mut remaining = bindings.clone();
                for branch in &item.branches
                {
                    check_binding_expression(registry, &branch.condition, &remaining, return_type,
                                             None)?;
                    let selected = condition_bindings(registry, &branch.condition,
                                                      &remaining, true);
                    check_binding_scope(registry, &branch.body.body, &selected, return_type,
                                        expected)?;
                    remaining = condition_bindings(registry, &branch.condition, &remaining, false);
                }
                if let Some(block) = &item.else_body
                { check_binding_scope(registry, &block.body, &remaining, return_type, expected)?; }
            },
        AstExpressionKind::MatchExpression(item) =>
            {
                check_binding_expression(registry, &item.value, bindings, return_type, None)?;
                let subject = type_subject(&item.value);
                let mut remaining = bindings.clone();
                let mut stable_subject = true;
                for arm in &item.arms
                {
                    let mut selected = remaining.clone();
                    if let Some(pattern) = &arm.pattern
                    {
                        check_binding_expression(registry, pattern, &remaining, return_type, None)?;
                        if expression_may_mutate(registry, pattern, &remaining)
                        {
                            stable_subject = false;
                            forget_inferred(&mut remaining);
                            selected = remaining.clone();
                        }
                        if stable_subject && let (Some(name), Some(id)) =
                            (subject, type_value(pattern, &remaining))
                        {
                            refine_type(registry, &mut selected, name, id, true);
                            refine_type(registry, &mut remaining, name, id, false);
                        }
                    }
                    check_binding_scope(registry, &arm.body.body,
                                        &selected, return_type, expected)?;
                }
            },
        AstExpressionKind::Grouped(inner) =>
            {
                let expected = if implicit_callable_result(registry, inner, bindings).is_some()
                    { None } else { expected };
                check_binding_expression(registry, inner, bindings, return_type, expected)?;
            },
        AstExpressionKind::Execute(call) =>
            check_binding_call(registry, call, bindings, return_type)?,
        AstExpressionKind::Redirect(source, redirects) =>
            {
                check_binding_expression(registry, source, bindings, return_type, None)?;
                for redirect in redirects
                {
                    check_binding_expression(registry, &redirect.target, bindings,
                                             return_type, None)?;
                }
            },
        AstExpressionKind::Array(values) =>
            {
                let mut current = bindings.clone();
                for value in values
                {
                    check_binding_expression(registry, value, &current, return_type, None)?;
                    if expression_may_mutate(registry, value, &current)
                    { forget_inferred(&mut current); }
                }
            },
        AstExpressionKind::HashMap(entries) =>
            {
                let mut current = bindings.clone();
                for (key, value) in entries
                {
                    for expression in [key, value]
                    {
                        check_binding_expression(registry, expression,
                                                 &current, return_type, None)?;
                        if expression_may_mutate(registry, expression, &current)
                        { forget_inferred(&mut current); }
                    }
                }
            },
        AstExpressionKind::StructConstructor(item) =>
            {
                let mut current = bindings.clone();
                for (_, _, value) in &item.fields
                {
                    check_binding_expression(registry, value, &current, return_type, None)?;
                    if expression_may_mutate(registry, value, &current)
                    { forget_inferred(&mut current); }
                }
            },
        AstExpressionKind::Range(start, end, _) =>
            {
                let mut current = bindings.clone();
                for value in [start, end].into_iter().flatten()
                {
                    check_binding_expression(registry, value, &current, return_type, None)?;
                    if expression_may_mutate(registry, value, &current)
                    { forget_inferred(&mut current); }
                }
            },
        AstExpressionKind::BooleanExpression(operator, left, right) =>
            {
                check_binding_expression(registry, left, bindings, return_type, None)?;
                let right_bindings = match operator
                    {
                        AstBooleanOperator::And =>
                            condition_bindings(registry, left, bindings, true),
                        AstBooleanOperator::Or =>
                            condition_bindings(registry, left, bindings, false),
                        _ =>
                            {
                                let mut current = bindings.clone();
                                if expression_may_mutate(registry, left, bindings)
                                { forget_inferred(&mut current); }
                                current
                            },
                    };
                check_binding_expression(registry, right, &right_bindings, return_type, None)?;
            },
        AstExpressionKind::Field(value, _, _) =>
            check_binding_expression(registry, value, bindings, return_type, None)?,
        AstExpressionKind::MathExpression(_, left, right)
        | AstExpressionKind::Index(left, right) =>
            {
                check_binding_expression(registry, left, bindings, return_type, None)?;
                let mut current = bindings.clone();
                if expression_may_mutate(registry, left, bindings)
                { forget_inferred(&mut current); }
                check_binding_expression(registry, right, &current, return_type, None)?;
            },
        AstExpressionKind::Splat(value)
        | AstExpressionKind::MathNegate(value) | AstExpressionKind::BooleanNot(value)
        | AstExpressionKind::TypeConversion(_, value, _)
        | AstExpressionKind::ExecutableReference(value)
        | AstExpressionKind::TryExecute(value) =>
            { check_binding_expression(registry, value, bindings, return_type, None)?; },
        _ => {}
    }
    Ok(())
}


fn binding_type(
    registry: &TypeRegistry, expression: &AstExpression, bindings: &Bindings,
) -> Option<TypeId>
{
    let mut stable;
    let bindings = if expression_may_mutate(registry, expression, bindings)
        && !matches!(expression.kind, AstExpressionKind::Field(_, _, _))
        {
            stable = bindings.clone();
            forget_inferred(&mut stable);
            &stable
        } else { bindings };
    binding_type_inner(registry, expression, bindings)
}

fn binding_type_inner(
    registry: &TypeRegistry, expression: &AstExpression, bindings: &Bindings,
) -> Option<TypeId>
{
    match &expression.kind
    {
        AstExpressionKind::Variable(variable) =>
            bindings.get(&variable.name).and_then(|binding| binding.type_id()),
        AstExpressionKind::Literal(item) => Some(registry.inferred_value_type(&item.value)),
        AstExpressionKind::Array(values) =>
            {
                let any = registry.builtin_id("any").unwrap();
                let element = registry.common_type(values.iter().map(|value|
                    {
                        let expanded = match &value.kind
                            {
                                AstExpressionKind::Splat(inner) =>
                                    binding_type(registry, inner, bindings),
                                AstExpressionKind::VariableSplat(variable) => bindings
                                    .get(&variable.name).and_then(|binding| binding.type_id()),
                                _ => return binding_type(registry, value, bindings).unwrap_or(any),
                            };
                        if let Some(id) = expanded
                        {
                            return match &registry.get(id).kind
                                {
                                    TypeKind::Array(item) => *item,
                                    TypeKind::FixedArray(items) =>
                                        registry.common_type(items.iter().copied()),
                                    _ if registry.get(id).name == "Range" =>
                                        registry.builtin_id("Integer").unwrap(),
                                    _ => any,
                                };
                        }
                        any
                    }));
                Some(registry.intern(TypeKind::Array(element)))
            },
        AstExpressionKind::HashMap(entries) =>
            {
                let any = registry.builtin_id("any").unwrap();
                let keys = registry.common_type(entries.iter().map(|(key, _)|
                    binding_type(registry, key, bindings).unwrap_or(any)));
                let values = registry.common_type(entries.iter().map(|(_, value)|
                    binding_type(registry, value, bindings).unwrap_or(any)));
                Some(registry.intern(TypeKind::Map(keys, values)))
            },
        AstExpressionKind::Range(_, _, _) => registry.builtin_id("Range"),
        AstExpressionKind::Grouped(inner) => implicit_callable_result(registry, inner, bindings)
            .or_else(|| binding_type(registry, inner, bindings)),
        AstExpressionKind::Execute(call) =>
            {
                let id = binding_type(registry, &call.executable, bindings)?;
                match registry.get(registry.underlying_type(id)).kind
                {
                    TypeKind::Function(_, result) => Some(result),
                    _ => None,
                }
            },
        AstExpressionKind::Field(object, name, _) =>
            {
                if name == "type" { return registry.builtin_id("Type"); }
                let id = binding_type(registry, object, bindings)?;
                let definition = registry.get(registry.underlying_type(id));
                if matches!(definition.kind, TypeKind::Enum(_)) && name == "index"
                { return registry.builtin_id("Integer"); }
                if let TypeKind::Struct(fields) = &definition.kind
                {
                    if let Some(field) = fields.iter().find(|field| field.name == *name)
                    { return Some(field.type_id); }
                }
                // User definitions are versioned and scoped. Their runtime signature
                // validates the result; a builtin signature cannot describe an override.
                if registry.has_extension(id, name) { return None; }
                registry.method(id, name).map(|method| method.return_type)
            },
        AstExpressionKind::Index(object, index) =>
            {
                let index = match constant_value(registry, index)
                    {
                        Some(Value::Integer(index)) => usize::try_from(index).ok(),
                        _ => None,
                    };
                indexed_type(registry, binding_type(registry, object, bindings)?, index)
            },
        _ => known_type(registry, expression)
    }
}


// These reference expressions are implicitly called in groups and statement position.
fn implicit_callable_result(registry: &TypeRegistry, expression: &AstExpression,
                            bindings: &Bindings) -> Option<TypeId>
{
    if !matches!(expression.kind, AstExpressionKind::Variable(_)
        | AstExpressionKind::Index(_, _) | AstExpressionKind::Field(_, _, _)
        | AstExpressionKind::ExecutableReference(_)) { return None; }
    let id = binding_type(registry, expression, bindings)?;
    match registry.get(registry.underlying_type(id)).kind
    {
        TypeKind::Function(_, result) => Some(result),
        _ => None,
    }
}


fn indexed_type(registry: &TypeRegistry, id: TypeId, index: Option<usize>) -> Option<TypeId>
{
    let id = registry.underlying_type(id);
    match &registry.get(id).kind
    {
        TypeKind::Array(element) => Some(*element),
        TypeKind::Map(_, element) => Some(registry.intern(TypeKind::Optional(*element))),
        TypeKind::FixedArray(items) => match index
            {
                Some(index) => items.get(index).copied(),
                None => Some(registry.common_type(items.iter().copied())),
            },
        _ => None,
    }
}


fn iterator_item_type(registry: &TypeRegistry, expression: &AstExpression,
                      bindings: &Bindings) -> Option<TypeId>
{
    let receiver = binding_type(registry, expression, bindings)?;
    // An override has its own scoped/versioned signature, not the native one.
    if registry.has_extension(receiver, "next_item") { return None; }
    let method = registry.method(receiver, "next_item")?;
    match registry.get(method.return_type).kind
    {
        TypeKind::Optional(item) => match registry.get(item).kind
            {
                TypeKind::Optional(inner) => Some(inner),
                _ if registry.get(item).name == "None" => None,
                _ => Some(item),
            },
        _ => None,
    }
}


// Calls can write dynamically scoped variables. Inferred contents are snapshots,
// not constraints: discard them across calls and control-flow joins.
fn expression_may_mutate(registry: &TypeRegistry, expression: &AstExpression,
                          bindings: &Bindings) -> bool
{
    let mutates = |item: &AstExpression| expression_may_mutate(registry, item, bindings);
    match &expression.kind
    {
        AstExpressionKind::Field(value, name, _) =>
            {
                if mutates(value) { return true; }
                if name == "type" { return false; }
                if let Some(id) = binding_type(registry, value, bindings)
                {
                    match &registry.get(registry.underlying_type(id)).kind
                    {
                        TypeKind::Struct(fields)
                            if fields.iter().any(|field| field.name == *name) =>
                            return false,
                        TypeKind::Enum(_) if name == "index" => return false,
                        _ => {},
                    }
                }
                true
            },
        AstExpressionKind::Execute(_)
        | AstExpressionKind::IfExpression(_) | AstExpressionKind::MatchExpression(_)
        | AstExpressionKind::Redirect(_, _) | AstExpressionKind::TryExecute(_) => true,
        AstExpressionKind::Grouped(value) =>
            {
                let callable = binding_type(registry, value, bindings).is_some_and(|id|
                    matches!(registry.get(registry.underlying_type(id)).kind,
                             TypeKind::Function(_, _)));
                callable || matches!(value.kind, AstExpressionKind::Variable(_)
                    | AstExpressionKind::Index(_, _) | AstExpressionKind::ExecutableReference(_))
                    || mutates(value)
            },
        AstExpressionKind::Splat(value)
        | AstExpressionKind::TypeConversion(_, value, _)
        | AstExpressionKind::ExecutableReference(value)
        | AstExpressionKind::MathNegate(value) | AstExpressionKind::BooleanNot(value) =>
            mutates(value),
        AstExpressionKind::Array(values) => values.iter().any(mutates),
        AstExpressionKind::HashMap(entries) => entries.iter().any(|(key, value)|
            mutates(key) || mutates(value)),
        AstExpressionKind::StructConstructor(item) => item.fields.iter()
            .any(|(_, _, value)| mutates(value)),
        AstExpressionKind::MathExpression(_, left, right)
        | AstExpressionKind::BooleanExpression(_, left, right)
        | AstExpressionKind::Index(left, right) => mutates(left) || mutates(right),
        AstExpressionKind::Range(start, end, _) =>
            start.iter().chain(end).any(|value| mutates(value)),
        _ => false,
    }
}
