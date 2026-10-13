
use std::{ cell::Cell, rc::Rc };

use crate::language::{ data::{ map_key::MapKey,
                               scoped_variables::{ EnvironmentConversion, ValueVisibility },
                               types::{ TypeId, TypeKind }, value::{ Executable, Value } },
                       interpreter::{ ErrorWhat, Interpreter, InterpreterError, InterpreterResult },
                       text::location::Location };


impl Interpreter
{
    fn environment_converter_required(&self, source: TypeId) -> bool
    {
        let definition = self.scope().types.get(source);
        !matches!(definition.kind, TypeKind::Builtin)
            || !matches!(definition.name.as_str(),
                "any" | "String" | "Integer" | "Float" | "Number" | "Boolean"
                | "ExecResult" | "None")
    }

    pub(super) fn retype_variable(&mut self, location: &Location, name: &str,
                                 target: TypeId, methods: &Value) -> InterpreterResult<()>
    {
        let invalid = |message| InterpreterError::new(location.clone(),
            ErrorWhat::InvalidOperand(message));
        let binding = self.variable_binding(name).ok_or_else(||
            invalid(format!("Cannot retype undefined variable '{}'", name)))?;
        if binding.borrow().reference.is_some()
        { return Err(invalid("Cannot retype a projected receiver binding".into())); }
        let current = self.read_raw_variable(name, location)?;
        let source = binding.borrow().type_id
            .and_then(|id| self.scope().types.collection_source_type(id, &current))
            .unwrap_or_else(|| self.scope().types.inferred_value_type(&current));
        let original = binding.borrow().environment_conversion.as_ref()
            .map(|conversion| conversion.original_type).unwrap_or(source);
        // Resolve forward declarations now, while their defining lexical scope is available.
        let mut methods = methods.clone();
        if let Value::HashMap(values) = &mut methods
        {
            let functions = self.scope().conversion_snapshot();
            for (key, value) in Rc::make_mut(values).iter_mut()
            {
                if matches!(value, Value::None)
                    && let MapKey::String(key) = key
                    && let Some(function) = functions.get(key)
                { *value = Value::String(key.clone(), Executable::Function(function.clone())); }
            }
        }
        // A collection's display text is not a reversible environment encoding.
        if    !self.has_captured_conversion(location, original, target, &methods)?
           && (self.environment_converter_required(target)
                || !matches!(self.scope().types.get(original).kind, TypeKind::Builtin))
        {
            return Err(invalid(format!("No reverse conversion from {} to {} for '{}'",
                self.scope().types.get(target).name, self.scope().types.get(original).name, name)));
        }
        let previous_result = self.last_result.take();
        let converted = self.convert_value(location, target, current, &methods,
            &Value::Integer(source.0 as i64));
        let value = self.last_result.take().unwrap_or(Value::None);
        self.last_result = previous_result;
        converted?;
        self.consume_success(location, &value)?;
        if self.halted { return Ok(()); }
        let mut binding = binding.borrow_mut();
        binding.value = value;
        binding.type_id = Some(target);
        binding.environment_conversion = Some(Rc::new(EnvironmentConversion
            {
                original_type: original,
                value_type: target,
                methods,
                active: Cell::new(false),
            }));
        Ok(())
    }

    pub(super) fn environment_value(&mut self, location: &Location, name: &str)
        -> InterpreterResult<Value>
    {
        let conversion = self.variable_binding(name)
            .and_then(|binding| binding.borrow().environment_conversion.clone());
        let value = self.read_raw_variable(name, location)?;
        let Some(conversion) = conversion else { return Ok(value); };
        let source = self.scope().types.collection_source_type(conversion.value_type, &value)
            .unwrap_or_else(|| self.scope().types.inferred_value_type(&value));
        if    self.environment_converter_required(source)
           && !self.has_captured_conversion(location, conversion.original_type, source,
                &conversion.methods)?
        {
            return Err(InterpreterError::new(location.clone(), ErrorWhat::InvalidOperand(
                format!("No reverse conversion from {} to {} for '{}'",
                    self.scope().types.get(source).name,
                    self.scope().types.get(conversion.original_type).name, name))));
        }
        if conversion.active.replace(true)
        {
            return Err(InterpreterError::new(location.clone(), ErrorWhat::InvalidOperand(
                format!("Recursive environment conversion for '{}'", name))));
        }
        let previous_result = self.last_result.take();
        let result = self.convert_value(location, conversion.original_type, value,
            &conversion.methods, &Value::Integer(conversion.value_type.0 as i64));
        let value = self.last_result.take().unwrap_or(Value::None);
        self.last_result = previous_result;
        conversion.active.set(false);
        result.map_err(|error|
            {
                let context = InterpreterError::new(location.clone(), ErrorWhat::InvalidOperand(
                    format!("Environment conversion for '{}' from {} to {} failed: {}", name,
                        self.scope().types.get(conversion.value_type).name,
                        self.scope().types.get(conversion.original_type).name, error.what)));
                error.with_context(context)
            })?;
        self.consume_success(location, &value)?;
        Ok(value)
    }

    pub(super) fn exported_environment(&mut self, location: &Location)
        -> InterpreterResult<Vec<(String, String)>>
    {
        if let Some(environment) = &self.widget_environment { return Ok(environment.clone()); }
        let bindings = self.scope().variables.get_all_flattened();
        let mut environment = Vec::new();
        for (key, binding) in bindings
        {
            if binding.exported != ValueVisibility::Exported { continue; }
            let text = self.environment_value(location, &key)?.as_text();
            if self.halted { break; }
            let text = if key == "$PATH" { self.eval_path_list_from(&text) }
                else { self.eval_path_from(&text) };
            environment.push((key.strip_prefix('$').unwrap_or(&key).to_string(), text));
        }
        Ok(environment)
    }
}
