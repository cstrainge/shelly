
use crate::language::{ data::{ map_key::MapKey,
                               methods::method_key,
                               types::{ TypeId, TypeKind },
                               value::{ Executable, Value } },
                       interpreter::{ ErrorWhat, Interpreter, InterpreterError, InterpreterResult },
                       text::location::Location };


impl Interpreter
{
    pub(super) fn convert_value(&mut self, location: &Location, target: TypeId,
                               value: Value, snapshot: &Value, hint: &Value)
        -> InterpreterResult<()>
    {
        let declared = match hint
        {
            Value::Integer(id) => Some(TypeId(*id as usize)),
            Value::String(_, Executable::Function(function)) => function.return_type,
            Value::String(name, Executable::Yes) =>
                self.module_function(name).and_then(|function| function.return_type),
            Value::String(name, Executable::No) => self.variable_binding(name)
                .and_then(|binding| binding.borrow().type_id),
            _ => None,
        };
        let declared = declared.map(|id| match self.scope().types
            .get(self.scope().types.underlying_type(id)).kind
        {
            TypeKind::Function(_, result) => result,
            _ => id,
        });
        let source = declared.and_then(|id| self.scope().types.collection_source_type(id, &value))
            .unwrap_or_else(|| self.scope().types.inferred_value_type(&value));
        let name = self.scope().types.conversion_method_name(source);
        let key = method_key(target, &name);
        let bound = match snapshot
            {
                Value::HashMap(methods) => methods.get(&MapKey::String(key.clone())),
                _ => None,
            };
        let function = match bound
            {
                Some(Value::String(_, Executable::Function(function))) => Some(function.clone()),
                // A declaration without a compiled body is resolved in the captured
                // lexical scope. An absent entry must not acquire a later REPL definition.
                Some(Value::None) => self.scope().lexical_function(&key)
                    .or_else(|| self.scope().imported_methods.get(&key).cloned())
                    .or_else(|| self.module_method(target, &key)),
                _ => None,
            };
        if matches!(bound, Some(Value::None)) && function.is_none()
        {
            return Err(InterpreterError::new(
                location.clone(),
                ErrorWhat::InvalidOperand(
                    format!(
                        "Conversion method '{}::{}' is unavailable during module initialization",
                        self.scope().types.get(target).name,
                        name
                    )
                )
            ));
        }
        let converted = if let Some(function) = function
        {
            let display = format!("{}::{}", self.scope().types.get(target).name, name);
            self.execute_function(location, &display, &function, &[value])?;
            if self.halted { return Ok(()); }
            let result = self.last_result.take().unwrap_or(Value::None);
            self.scope().types.coerce(target, result)
        }
        else
        {
            self.scope().types.convert(target, &value)
        };
        self.last_result = Some(converted.map_err(|message| InterpreterError::new(
            location.clone(),
            ErrorWhat::InvalidOperand(format!("Type conversion error: {}", message))
        ))?);
        Ok(())
    }
}
