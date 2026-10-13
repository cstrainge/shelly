
use std::mem::replace;

use crate::language::{
    data::{scoped_variables::ValueVisibility, value::Value},
    interpreter::{ErrorWhat, Interpreter, InterpreterError, InterpreterResult},
    text::location::Location,
};

impl Interpreter
{
    // Use the same representation for lookup and child-process inheritance.
    fn exported_variable_text(&self, key: &str, location: &Location) -> InterpreterResult<String>
    {
        let text = self.read_raw_variable(key, location)?.as_text();
        Ok(if key == "$PATH"
            {
                self.eval_path_list_from(&text)
            }
            else
            {
                self.eval_path_from(&text)
            })
    }

    pub(super) fn exported_environment(&self, location: &Location)
        -> InterpreterResult<Vec<(String, String)>>
    {
        if let Some(environment) = &self.widget_environment { return Ok(environment.clone()); }
        self.scope().variables.get_all_flattened()
            .iter()
            .filter(|(_, value)| value.exported == ValueVisibility::Exported)
            .map(|(key, _)|
                {
                    let text = self.exported_variable_text(key, location)?;
                    Ok((key.strip_prefix('$').unwrap_or(key.as_str()).to_string(), text))
                })
            .collect()
    }

    fn environment_variable(&self, name: &str, location: &Location) -> InterpreterResult<Value>
    {
        if let Some(environment) = &self.widget_environment
        {
            return Ok(environment.iter().find(|(key, _)| key == name)
                .map_or(Value::None, |(_, text)| Value::from_string(text.clone())));
        }
        let key = format!("${name}");
        let exported = self.scope().variables.get(&key)
            .is_some_and(|value| value.exported == ValueVisibility::Exported);
        if !exported { return Ok(Value::None); }
        self.exported_variable_text(&key, location).map(Value::from_string)
    }

    pub(super) fn handle_env(&mut self, location: &Location, args: &[Value])
        -> InterpreterResult<()>
    {
        let (name, fallback) = match args
        {
            [Value::String(name, _)] => (name, Value::None),
            [Value::String(name, _), fallback @ (Value::String(_, _) | Value::None)] =>
                (name, fallback.clone()),
            _ =>
            {
                return Err(InterpreterError::new(location.clone(), ErrorWhat::ArgumentMismatch(
                    "__env expects a String name and an optional String fallback".into())));
            }
        };
        if name.is_empty() || name.contains(['=', '\0'])
        {
            return Err(InterpreterError::new(location.clone(), ErrorWhat::InvalidOperand(
                "Environment names must be nonempty and contain neither '=' nor NUL".into())));
        }

        // This private bridge reads the caller of its Shelly wrapper, rather than
        // the standard library's separate module environment. Direct top-level
        // native calls use the current scope instead.
        let caller = self.exceptions.callers.last().cloned()
            .unwrap_or_else(|| self.current_scope.clone());
        let home = replace(&mut self.current_scope, caller);
        let result = self.environment_variable(name, location);
        self.current_scope = home;
        self.last_result = Some(match result?
            {
                Value::None => fallback,
                value => value,
            });
        Ok(())
    }
}
