
use crate::language::{ data::{ scoped_variables::{ ScopedValue, ValueVisibility }, value::Value },
                       interpreter::{ ErrorWhat, Interpreter, InterpreterError, InterpreterResult },
                       text::location::Location };


impl Interpreter
{
    fn render_widgets(&mut self, location: &Location) -> InterpreterResult<String>
    {
        let Value::Array(widgets) = self.read_raw_variable("$widgets", location)? else
        {
            return Err(InterpreterError { location: location.clone(),
                what: ErrorWhat::InvalidOperand("Widgets require an array".into()) });
        };
        let widget_type = self.scope().types.native_types["WidgetFn"].id;
        let mut rendered = String::new();
        // Snapshot the callback list; changes made by widgets apply to the next prompt.
        for widget in widgets.iter()
        {
            let widget = self.scope().types.coerce(widget_type, widget.clone())
                .map_err(|message| InterpreterError { location: location.clone(),
                    what: ErrorWhat::InvalidOperand(message) })?;
            self.last_result = None;
            self.execute_value(location, widget, Vec::new())?;
            if self.halted { break; }
            let Value::String(text, _) = self.last_result.take().unwrap_or(Value::None) else
            {
                return Err(InterpreterError { location: location.clone(),
                    what: ErrorWhat::InvalidOperand("Widget must return a String".into()) });
            };
            if text.is_empty() { continue; }
            rendered.push('[');
            rendered.push_str(&text);
            rendered.push(']');
        }
        Ok(rendered)
    }

    pub fn evaluate_prompt(&mut self, location: Location) -> InterpreterResult<Option<String>>
    {
        let previous_result = self.last_result.take();
        let result = (||
            {
                // Only return values contribute widget text, not printed stdout.
                let environment = self.exported_environment(&location)?;
                let previous_environment = self.widget_environment.replace(environment);
                let (rendered, _) = self.capture_stdout(|interpreter|
                    interpreter.render_widgets(&location));
                self.widget_environment = previous_environment;
                let rendered = rendered?;
                if self.halted || !self.has_command("prompt") { return Ok(None); }

                let home = self.scope().base_function("prompt")
                    .map(|function| function.functions.borrow().scope.clone())
                    .unwrap_or_else(|| self.current_scope.clone());
                let scope = self.scopes.get_mut(&home).expect("Prompt module must exist");
                let initial_scope = scope.variables.current_scope();
                scope.variables.push_scope();
                let _ = scope.variables.create("$rendered_widgets".into(), ScopedValue
                    {
                        value: Value::from_string(rendered),
                        type_id: scope.types.builtin_id("String"),
                        exported: ValueVisibility::Private,
                        reference: None,
                    });
                let (result, bytes) = self.capture_stdout(|interpreter|
                    interpreter.execute_command(location, "prompt", Vec::new()));
                self.scopes.get_mut(&home).unwrap().variables.reset_to_scope(initial_scope);
                result?;
                Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
            })();
        self.last_result = previous_result;
        result
    }
}
