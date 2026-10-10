
use std::collections::HashSet;

use crate::language::{ ast::*, data::value::Value };

pub fn free_variables(function: &AstFunctionStatement) -> Vec<String>
{
    // Capture used bindings, retaining their Rc rather than their current value.
    // Nested literals contribute outer references needed when they are created later.
    let mut free = HashSet::new();
    let bound = function.parameters.iter().map(|item| item.name.clone()).collect();
    statements(&function.body, &bound, &mut free);
    let mut names: Vec<_> = free.into_iter().collect();
    names.sort();
    names
}

fn name(value: &str, bound: &HashSet<String>, free: &mut HashSet<String>)
{
    if !bound.contains(value) { free.insert(value.to_string()); }
}

fn text(value: &str, escaped: &[usize], bound: &HashSet<String>, free: &mut HashSet<String>)
{
    let mut chars = value.char_indices().peekable();
    while let Some((offset, character)) = chars.next()
    {
        if character != '$' || escaped.contains(&offset) { continue; }
        let mut variable = "$".to_string();
        if chars.peek().is_some_and(|(_, c)| *c == '{')
        {
            chars.next();
            for (_, c) in chars.by_ref()
            {
                if c == '}' { break; }
                variable.push(c);
            }
        }
        else
        {
            while let Some(&(_, c)) = chars.peek()
            {
                if c == ':'
                {
                    let mut probe = chars.clone();
                    probe.next();
                    if probe.next().is_some_and(|(_, c)| c == ':')
                        && probe.peek().is_some_and(|(_, c)| c.is_alphanumeric() || *c == '_')
                    {
                        chars.next(); chars.next(); variable.push_str("::");
                        continue;
                    }
                }
                if !c.is_alphanumeric() && c != '_' { break; }
                chars.next(); variable.push(c);
            }
        }
        if variable.len() > 1 { name(&variable, bound, free); }
    }
}

fn statements(items: &AstTopLevel, parent: &HashSet<String>, free: &mut HashSet<String>)
{
    let mut bound = parent.clone();
    for item in items
    {
        match item
        {
            AstStatement::LetStatement(item) =>
                {
                    expression(&item.expression, &bound, free);
                    bound.insert(item.identifier.clone());
                },
            AstStatement::SetStatement(item) =>
                {
                    name(&item.identifier, &bound, free);
                    for access in &item.indexes
                    {
                        if let AstAccess::Index(index) = access { expression(index, &bound, free); }
                    }
                    expression(&item.expression, &bound, free);
                },
            AstStatement::ExpressionStatement(item) | AstStatement::DiscardStatement(item) =>
                expression(item, &bound, free),
            AstStatement::ExecuteStatement(item) => call(item, &bound, free),
            AstStatement::ReturnStatement(item) =>
                { if let Some(item) = &item.expression { expression(item, &bound, free); } },
            AstStatement::FunctionDefinition(item) =>
                {
                    let mut nested = bound.clone();
                    nested.extend(item.parameters.iter().map(|item| item.name.clone()));
                    statements(&item.body, &nested, free);
                },
            AstStatement::BlockStatement(item) | AstStatement::LoopStatement(item) =>
                statements(&item.body, &bound, free),
            AstStatement::ForStatement(item) =>
                {
                    expression(&item.iterable, &bound, free);
                    let mut nested = bound.clone();
                    nested.extend(item.bindings.iter().cloned());
                    statements(&item.body.body, &nested, free);
                },
            AstStatement::ConditionalLoopStatement(item) =>
                {
                    expression(&item.condition, &bound, free);
                    statements(&item.body.body, &bound, free);
                },
            AstStatement::ImportStatement(_) | AstStatement::EnumDeclaration(_)
            | AstStatement::StructDeclaration(_) | AstStatement::TypeDeclaration(_)
            | AstStatement::AliasStatement(_) | AstStatement::BreakStatement(_)
            | AstStatement::ContinueStatement(_) | AstStatement::NullStatement => {},
        }
    }
}

fn call(item: &AstExecuteStatement, bound: &HashSet<String>, free: &mut HashSet<String>)
{
    expression(&item.executable, bound, free);
    for item in &item.arguments { expression(item, bound, free); }
}

fn expression(item: &AstExpression, bound: &HashSet<String>, free: &mut HashSet<String>)
{
    match &item.kind
    {
        AstExpressionKind::Variable(item) | AstExpressionKind::VariableSplat(item) =>
            name(&item.name, bound, free),
        AstExpressionKind::Literal(item_value) =>
            {
                if let Value::String(value, _) = &item_value.value
                    && let Some(AstStringFlag::Interpolated(escaped)) = &item.string_flag
                { text(value, escaped, bound, free); }
            },
        AstExpressionKind::Symbol(symbol) =>
            {
                match &item.string_flag
                {
                    Some(AstStringFlag::NonInterpolated) => {},
                    Some(AstStringFlag::Interpolated(escaped)) =>
                        text(&symbol.name, escaped, bound, free),
                    None => text(&symbol.name, &[], bound, free),
                }
            },
        AstExpressionKind::AnonymousFunction(function) =>
            {
                let mut nested = bound.clone();
                nested.extend(function.parameters.iter().map(|item| item.name.clone()));
                statements(&function.body, &nested, free);
            },
        AstExpressionKind::Execute(item) => call(item, bound, free),
        AstExpressionKind::Array(items) =>
            { for item in items { expression(item, bound, free); } },
        AstExpressionKind::HashMap(items) =>
            {
                for (key, value) in items
                { expression(key, bound, free); expression(value, bound, free); }
            },
        AstExpressionKind::StructConstructor(item) =>
            { for (_, _, item) in &item.fields { expression(item, bound, free); } },
        AstExpressionKind::Range(start, end, _) =>
            { for item in start.iter().chain(end) { expression(item, bound, free); } },
        AstExpressionKind::Index(a, b) | AstExpressionKind::MathExpression(_, a, b)
        | AstExpressionKind::BooleanExpression(_, a, b) =>
            { expression(a, bound, free); expression(b, bound, free); },
        AstExpressionKind::Field(item, _, _) | AstExpressionKind::Splat(item)
        | AstExpressionKind::Grouped(item) | AstExpressionKind::ExecutableReference(item)
        | AstExpressionKind::TryExecute(item) | AstExpressionKind::MathNegate(item)
        | AstExpressionKind::BooleanNot(item) | AstExpressionKind::TypeConversion(_, item, _) =>
            expression(item, bound, free),
        AstExpressionKind::Redirect(item, redirects) =>
            {
                expression(item, bound, free);
                for item in redirects { expression(&item.target, bound, free); }
            },
        AstExpressionKind::IfExpression(item) =>
            {
                for branch in &item.branches
                {
                    expression(&branch.condition, bound, free);
                    statements(&branch.body.body, bound, free);
                }
                if let Some(body) = &item.else_body { statements(&body.body, bound, free); }
            },
        AstExpressionKind::MatchExpression(item) =>
            {
                expression(&item.value, bound, free);
                for arm in &item.arms
                {
                    if let Some(pattern) = &arm.pattern { expression(pattern, bound, free); }
                    statements(&arm.body.body, bound, free);
                }
            },
        AstExpressionKind::SpacedEmptyCall(_) | AstExpressionKind::EnumVariant(_, _) => {},
    }
}
