
use std::mem::take;

use crate::language::{ bytecode::PipelineStage,
                       data::{ types::{ TypeId, TypeKind, TypeRegistry },
                               value::{ ExecResult, Executable, Value },
                               methods::MethodDefinition },
                       interpreter::{ ErrorWhat, Interpreter, InterpreterError, InterpreterResult },
                       interpreter::iteration::Iteration,
                       interpreter::pipeline::{ Context, Data, Step, Stream, process::External } };


fn array_type(types: &TypeRegistry, id: TypeId) -> Option<TypeId>
{
    let id = types.underlying_type(id);
    match types.get(id).kind
    {
        TypeKind::Array(_) | TypeKind::FixedArray(_) => Some(id),
        TypeKind::Optional(inner) => array_type(types, inner),
        _ => None,
    }
}


pub(super) struct StageStream<'a>
{
    stage: &'a PipelineStage,
    index: usize,
    input: Option<Box<dyn Stream + 'a>>,
    external: Option<External>,
    bytes: Vec<u8>,
    pending: Option<Data>,
    source_done: bool,
    ended: bool,
    destination: Option<Vec<Value>>,
    destination_ready: bool,
    buffering: bool,
    flushed: bool,
    prepared: Option<(Value, Vec<Value>)>,
    per_item_external: bool,
    external_started: bool,
    iterate_source: bool,
    iteration: Option<Iteration>,
}


impl<'a> StageStream<'a>
{
    pub fn new(stage: &'a PipelineStage, index: usize, input: Option<Box<dyn Stream + 'a>>,
               iterate_source: bool) -> Self
    {
        Self { stage, index, input, external: None, bytes: Vec::new(), pending: None,
            source_done: false, ended: false, destination: None, destination_ready: false,
            buffering: false, flushed: false, prepared: None, per_item_external: false,
            external_started: false, iterate_source, iteration: None }
    }

    fn error(&self, message: impl Into<String>) -> InterpreterError
    {
        InterpreterError { location: self.stage.location.clone(),
            what: ErrorWhat::InvalidOperand(message.into()) }
    }

    fn prepare(&self, interpreter: &mut Interpreter, input: Option<&Value>)
        -> InterpreterResult<(Value, Vec<Value>)>
    {
        let depth = interpreter.pipeline_inputs.len();
        if let Some(input) = input { interpreter.pipeline_inputs.push(input.clone()); }
        interpreter.last_result = None;
        let result = interpreter.execute_instructions(&self.stage.code);
        interpreter.pipeline_inputs.truncate(depth);
        result?;
        let value = interpreter.last_result.take().unwrap_or(Value::None);
        if self.stage.explicit_arguments
        {
            let Value::Array(parts) = value else { unreachable!(); };
            let Value::Array(arguments) = &parts[1] else { unreachable!(); };
            Ok((parts[0].clone(), arguments.to_vec()))
        }
        else { Ok((value, Vec::new())) }
    }

    fn invoke(&mut self, interpreter: &mut Interpreter, context: &mut Context,
              data: Option<Data>) -> InterpreterResult<Step>
    {
        if self.stage.destination.is_some()
        {
            let Some(Data::Value(value)) = data else { unreachable!(); };
            self.assign(interpreter, value.clone())?;
            return Ok(Step::Data(Data::Value(value)));
        }
        let input = match &data { Some(Data::Value(value)) => Some(value.clone()), _ => None };
        let (mut executable, mut arguments) = if let Some(prepared) = self.prepared.take()
            { prepared } else { self.prepare(interpreter, input.as_ref())? };
        if    self.stage.call && !executable.is_callable()
           && (self.stage.command_word || self.stage.explicit_arguments
                || executable.is_executable())
        {
            let name = Interpreter::command_name(&self.stage.location, executable.clone())?;
            let external_only = executable.is_external();
            let (name, defaults, home) = if external_only
                { (name, Vec::new(), interpreter.current_scope.clone()) }
                else { interpreter.resolve_alias(&self.stage.location, &name)? };
            let mut resolved = defaults.into_iter().map(Value::from_string).collect::<Vec<_>>();
            resolved.extend(arguments);
            arguments = resolved;
            let scope = interpreter.scopes.get(&home).unwrap();
            if !external_only && let Some(function) = scope.function(&name)
            { executable = Value::String(name, Executable::Function(function)); }
            else if !external_only && let Some(function) = interpreter.module_native_function(&name)
            { executable = Value::String(name, Executable::Native(function)); }
            else
            {
                let mut external = External::new(interpreter, self.stage, &name, &arguments,
                    self.index == 0)?;
                self.per_item_external = self.index > 0 && self.stage.input_used;
                if self.per_item_external { external.close_input(); }
                else { self.pending = data; }
                if self.index == 0 { external.close_input(); }
                self.external = Some(external);
                self.external_started = true;
                return Ok(Step::Pending);
            }
        }
        if let Some(Data::Bytes(bytes)) = data
        {
            // Unannotated byte streams become one String at a Shelly value boundary.
            self.bytes.extend(bytes);
            self.buffering = true;
            self.prepared = Some((executable, arguments));
            return Ok(Step::Pending);
        }
        if self.stage.call && executable.is_callable()
        {
            if self.index > 0
            {
                if self.stage.explicit_arguments && !self.stage.input_used
                { return Err(self.error(
                    "A pipeline function with explicit arguments must use bare $")); }
                let getter = self.stage.property && match executable.underlying()
                    {
                        Value::String(_, Executable::Method(method)) => match &method.definition
                            {
                                MethodDefinition::Builtin(item) => item.argument_count == 0,
                                MethodDefinition::User(item) =>
                                    item.arguments.len() == 1 && !item.variadic,
                            },
                        _ => false,
                    };
                if !self.stage.explicit_arguments && !getter
                { arguments.push(input.clone().unwrap_or(Value::None)); }
            }
            let depth = interpreter.pipeline_inputs.len();
            if let Some(input) = input { interpreter.pipeline_inputs.push(input); }
            interpreter.last_result = None;
            let redirection_depth = interpreter.redirections.len();
            let result = (||
                {
                    self.begin_redirects(interpreter)?;
                    interpreter.execute_value(&self.stage.location, executable, arguments)
                })();
            let finished = interpreter.finish_redirections(redirection_depth);
            let result = result.and(finished);
            interpreter.pipeline_inputs.truncate(depth);
            match result
            {
                Err(InterpreterError { what: ErrorWhat::ExecutableBadReturn(code), .. }) =>
                    { context.record(self.index, ExecResult::Value(code)); return Ok(Step::End); },
                Err(InterpreterError { what: ErrorWhat::ExecutableSignaled, .. }) =>
                    { context.record(self.index, ExecResult::Signaled); return Ok(Step::End); },
                other => other?,
            }
            executable = interpreter.last_result.take().unwrap_or(Value::None);
        }
        else if !self.stage.redirects.is_empty()
        {
            let depth = interpreter.pipeline_inputs.len();
            if let Some(input) = input { interpreter.pipeline_inputs.push(input); }
            let redirection_depth = interpreter.redirections.len();
            let result = (||
                {
                    self.begin_redirects(interpreter)?;
                    interpreter.redirect_source(&self.stage.location, executable, false)
                })();
            let finished = interpreter.finish_redirections(redirection_depth);
            interpreter.pipeline_inputs.truncate(depth);
            result.and(finished)?;
            executable = interpreter.last_result.take().unwrap_or(Value::None);
        }
        Ok(Step::Data(Data::Value(executable)))
    }

    fn begin_redirects(&self, interpreter: &mut Interpreter) -> InterpreterResult<()>
    {
        for redirect in &self.stage.redirects
        {
            interpreter.execute_instructions(&redirect.code)?;
            let target = interpreter.last_result.take().unwrap_or(Value::None);
            interpreter.begin_redirection(&redirect.location, redirect.stream,
                redirect.variable, target)?;
        }
        Ok(())
    }

    fn initialize_destination(&mut self, interpreter: &Interpreter) -> InterpreterResult<()>
    {
        let name = self.stage.destination.as_ref().unwrap();
        if !self.destination_ready
        {
            let binding = interpreter.variable_binding(name)
                .ok_or_else(|| self.error(
                    format!("Variable '{}' not found; declare it with let", name)))?;
            let binding = binding.borrow();
            let array = binding.type_id.is_some_and(|id|
                array_type(&interpreter.scope().types, id).is_some())
                || matches!(binding.value.underlying(), Value::Array(_));
            if array { self.destination = Some(Vec::new()); }
            self.destination_ready = true;
        }
        Ok(())
    }

    fn assign(&mut self, interpreter: &mut Interpreter, mut value: Value) -> InterpreterResult<()>
    {
        self.initialize_destination(interpreter)?;
        let name = self.stage.destination.as_ref().unwrap();
        if let Some(values) = &mut self.destination
        {
            if let Some(id) = interpreter.variable_binding(name).unwrap().borrow().type_id
            {
                let types = &interpreter.scope().types;
                let definition = array_type(types, id).map(|id| types.get(id));
                let item = match definition.as_ref().map(|definition| &definition.kind)
                    { Some(TypeKind::Array(item)) => Some(*item),
                      Some(TypeKind::FixedArray(items)) => items.get(values.len()).copied(),
                      _ => None };
                if let Some(item) = item
                {
                    value = interpreter.scope().types.coerce(item, value).map_err(|message|
                        InterpreterError { location: self.stage.location.clone(),
                            what: ErrorWhat::InvalidOperand(
                                format!("Pipeline destination: {}", message)) })?;
                }
            }
            values.push(value);
            Ok(())
        }
        else { interpreter.write_variable(&self.stage.location, name, value) }
    }

    fn finish(&mut self, interpreter: &mut Interpreter) -> InterpreterResult<Step>
    {
        if self.stage.destination.is_some() && !self.destination_ready
        { self.initialize_destination(interpreter)?; }
        if let Some(values) = self.destination.take()
        {
            interpreter.write_variable(&self.stage.location,
                self.stage.destination.as_ref().unwrap(),
                Value::from_array(values))?;
        }
        self.ended = true;
        Ok(Step::End)
    }
}


impl Stream for StageStream<'_>
{
    fn poll(&mut self, interpreter: &mut Interpreter, context: &mut Context)
        -> InterpreterResult<Step>
    {
        if self.ended { return Ok(Step::End); }
        if self.external.is_some()
        {
            let result = self.external.as_mut().unwrap().poll(interpreter, context, self.index)?;
            if !matches!(result, Step::Pending)
            {
                if matches!(result, Step::End)
                {
                    if self.per_item_external
                    {
                        self.external.take();
                        return Ok(Step::Pending);
                    }
                    if let Some(input) = self.input.as_mut()
                    { input.cancel(interpreter, context)?; }
                    return self.finish(interpreter);
                }
                return Ok(result);
            }
            if self.external.as_ref().unwrap().wants_input()
            {
                let next = if let Some(data) = self.pending.take() { Step::Data(data) }
                    else if let Some(input) = self.input.as_mut()
                    { input.poll(interpreter, context)? }
                    else { Step::End };
                match next
                {
                    Step::Data(data) =>
                        {
                            let bytes = match data { Data::Bytes(bytes) => bytes,
                                Data::Value(value) =>
                                    interpreter.pipeline_bytes(self.stage, value)? };
                            self.external.as_mut().unwrap().feed(bytes);
                            if self.per_item_external
                            { self.external.as_mut().unwrap().close_input(); }
                        },
                    Step::End => self.external.as_mut().unwrap().close_input(),
                    Step::Pending => {},
                }
            }
            return Ok(Step::Pending);
        }
        if self.index == 0
        {
            if let Some(iteration) = &mut self.iteration
            {
                return match iteration.next(interpreter, &self.stage.location)
                    {
                        Ok(Some(value)) => Ok(Step::Data(Data::Value(value))),
                        Ok(None) => self.finish(interpreter),
                        Err(InterpreterError { what: ErrorWhat::ExecutableBadReturn(code), .. }) =>
                            {
                                context.record(self.index, ExecResult::Value(code));
                                self.finish(interpreter)
                            },
                        Err(InterpreterError { what: ErrorWhat::ExecutableSignaled, .. }) =>
                            {
                                context.record(self.index, ExecResult::Signaled);
                                self.finish(interpreter)
                            },
                        Err(error) => Err(error),
                    };
            }
            if self.source_done { return self.finish(interpreter); }
            self.source_done = true;
            let result = self.invoke(interpreter, context, None)?;
            if self.iterate_source && let Step::Data(Data::Value(value)) = result
            {
                self.iteration = Iteration::try_new(interpreter, &self.stage.location,
                    value.clone(), &self.stage.conversions)?;
                if self.iteration.is_some() { return Ok(Step::Pending); }
                return Ok(Step::Data(Data::Value(value)));
            }
            return Ok(result);
        }
        let next = self.input.as_mut().unwrap().poll(interpreter, context)?;
        match next
        {
            Step::Pending => Ok(Step::Pending),
            Step::End =>
                {
                    if    !self.flushed
                       && (self.buffering || self.input.as_ref().unwrap().bytes_output())
                    {
                        self.flushed = true;
                        let bytes = take(&mut self.bytes);
                        let text = String::from_utf8(bytes)
                            .map_err(|error| self.error(error.to_string()))?;
                        return self.invoke(interpreter, context,
                            Some(Data::Value(Value::from_string(text))));
                    }
                    self.finish(interpreter)
                },
            Step::Data(Data::Bytes(bytes)) if self.buffering || self.stage.input_used
                || self.stage.destination.is_some() =>
                { self.bytes.extend(bytes); self.buffering = true; Ok(Step::Pending) },
            Step::Data(data) => self.invoke(interpreter, context, Some(data)),
        }
    }

    fn cancel(&mut self, interpreter: &mut Interpreter, context: &mut Context)
        -> InterpreterResult<()>
    {
        let result = if let Some(external) = self.external.as_mut()
            { external.cancel(interpreter, context, self.index) } else { Ok(()) };
        let upstream = if let Some(input) = self.input.as_mut()
            { input.cancel(interpreter, context) } else { Ok(()) };
        self.iteration = None;
        self.ended = true;
        result.and(upstream)
    }

    fn bytes_output(&self) -> bool { self.external_started }
}
