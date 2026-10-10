
use std::{ cell::RefCell,
           collections::{ HashMap, HashSet },
           env::{ current_dir, split_paths },
           fs::{ canonicalize, File },
           io::{ BufReader, ErrorKind },
           mem::{ replace, take },
           path::{ Path, PathBuf },
           rc::Rc };

use crate::language::{ ast::{ AstExpression, AstExpressionKind, AstImportStatement,
                              AstStatement, AstTopLevel },
                       bytecode::{ FunctionRef, Instruction },
                       data::{ scoped_variables::{ ScopedValue, ValueReference, ValueVisibility },
                               types::{ TypeDefinition, TypeId }, value::Value },
                       native::NativeFunction,
                       interpreter::{ ErrorWhat, Interpreter, InterpreterError, InterpreterResult,
                                      scope::Scope },
                       exclusions::exclude_statements,
                       parser::parse_text,
                       text::{ location::Location, read_buffer::ReadBuffer },
                       tokenizer::Tokenizer };


// Each prelude generation retains its own scope and type handles. Changing the
// search path alone does not replace it; prelude_reload publishes a new generation.
#[derive(Clone)]
pub(super) struct Prelude
{
    scope: String,
    types: HashMap<String, Rc<TypeDefinition>>,
}


fn module_error(location: &Location, message: impl Into<String>) -> InterpreterError
{
    InterpreterError { location: location.clone(), what: ErrorWhat::ModuleError(message.into()) }
}


impl Interpreter
{
    // Return the original module and member, never a copied declaration.
    fn module_member<'a>(&'a self, name: &'a str) -> Option<(&'a Scope, &'a str)>
    {
        let mut scope = self.scope();
        let mut member = name;
        while member.contains("::")
        {
            // Imports may bind a compound namespace such as std::net. Prefer the
            // longest bound prefix, then continue through the target module's imports.
            let (namespace, key) = scope.modules.iter()
                .filter(|(namespace, _)| member.strip_prefix(namespace.as_str())
                    .is_some_and(|tail| tail.starts_with("::")))
                .max_by_key(|(namespace, _)| namespace.len())?;
            member = &member[namespace.len() + 2..];
            scope = self.scopes.get(key)?;
        }
        scope.exports.contains(member).then_some((scope, member))
    }

    pub(super) fn module_function(&self, name: &str) -> Option<FunctionRef>
    {
        if !name.contains("::") { return self.scope().function(name); }
        let (scope, name) = self.module_member(name)?;
        scope.base_function(name)
    }

    pub(super) fn module_native_function(&self, name: &str) -> Option<Rc<NativeFunction>>
    {
        if !name.contains("::") { return self.scope().native_functions.get(name).cloned(); }
        let (scope, name) = self.module_member(name)?;
        scope.native_functions.get(name).cloned()
    }

    pub(super) fn variable_binding(&self, name: &str) -> Option<Rc<RefCell<ScopedValue>>>
    {
        if let Some(binding) = self.scope().variables.binding(name) { return Some(binding); }
        if !name.contains("::") { return None; }
        let (namespace, member) = name.strip_prefix('$')?.rsplit_once("::")?;
        let qualified = format!("{}::${}", namespace, member);
        let (scope, name) = self.module_member(&qualified)?;
        scope.variables.binding(name)
    }

    pub(super) fn variable_reference(&self, name: &str) -> Option<ValueReference>
    {
        if let Some(reference) = self.scope().variables.reference(name) { return Some(reference); }
        let root = self.variable_binding(name)?;
        if let Some(reference) = &root.borrow().reference { return Some(reference.clone()); }
        Some(ValueReference
            { root, indexes: Vec::new(), fields: Vec::new(), constraints: Vec::new() })
    }

    pub(super) fn module_method(&self, id: TypeId, key: &str) -> Option<FunctionRef>
    {
        self.scopes.get(self.type_scopes.get(&id)?)?.base_function(key)
    }

    fn evaluate_condition(&mut self, condition: AstExpression) -> InterpreterResult<bool>
    {
        let location = condition.location.clone();
        let expression = AstExpression
            {
                location: location.clone(), string_flag: None,
                kind: AstExpressionKind::BooleanNot(Box::new(AstExpression
                    {
                        location, string_flag: None,
                        kind: AstExpressionKind::BooleanNot(Box::new(condition)),
                    })),
            };
        let previous = self.last_result.take();
        let result = (||
            {
                self.refresh_module_names();
                let mut condition = vec![AstStatement::ExpressionStatement(expression)];
                let code = self.scope_mut().compile_condition(&mut condition)?;
                self.execute_instructions(&code)?;
                Ok(self.last_result.as_ref().is_some_and(Value::as_bool))
            })();
        self.last_result = previous;
        result
    }

    fn import_conditions(&mut self, statements: &mut AstTopLevel)
        -> InterpreterResult<Vec<AstImportStatement>>
    {
        exclude_statements(statements, &mut |condition| self.evaluate_condition(condition))?;
        let mut imports = Vec::new();
        let mut body = Vec::new();
        for statement in take(statements)
        {
            if let AstStatement::ImportStatement(import) = statement { imports.push(*import); }
            else { body.push(statement); }
        }
        *statements = body;
        // None of the containing file's imports or declarations have been published yet.
        let mut enabled = Vec::new();
        for mut import in imports
        {
            let accepted = if let Some(condition) = import.condition.take()
                {
                    self.evaluate_condition(condition)?
                }
                else { true };
            import.enabled = accepted;
            enabled.push(import);
        }
        Ok(enabled)
    }

    fn source_directory(origin: &str) -> PathBuf
    {
        if origin.starts_with('<') { return current_dir().unwrap_or_else(|_| ".".into()); }
        canonicalize(origin).unwrap_or_else(|_| PathBuf::from(origin))
            .parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    }

    fn module_directories(&self, standard: bool, directory: &Path, location: &Location)
        -> InterpreterResult<Vec<PathBuf>>
    {
        let variable = if standard { "$SHELLY_STD_PATH" } else { "$SHELLY_MODULE_PATH" };
        let mut directories = if standard { Vec::new() } else { vec![directory.to_path_buf()] };
        let paths = if let Some(binding) = self.variable_binding(variable)
            {
                let value = binding.borrow().value.clone();
                let Value::String(paths, _) = value else
                { return Err(module_error(location, format!("{} must be a String", variable))); };
                paths
            }
            else if standard
            { "/etc/shelly/std:/usr/local/share/shelly/std:/usr/share/shelly/std".into() }
            else { String::new() };
        directories.extend(split_paths(&paths).filter(|path| !path.as_os_str().is_empty())
            .map(|path| PathBuf::from(self.eval_path_from(&path.to_string_lossy()))));
        Ok(directories)
    }

    fn find_module_file(&self, import: &AstImportStatement, directory: &Path)
        -> InterpreterResult<Option<PathBuf>>
    {
        let standard = import.module.strip_prefix("std::");
        let name = standard.unwrap_or(&import.module);
        let directories = self.module_directories(standard.is_some(), directory, &import.location)?;
        for directory in directories
        {
            let file = directory.join(format!("{}.shy", name));
            match canonicalize(&file)
            {
                Ok(path) => return Ok(Some(path)),
                Err(error) if error.kind() == ErrorKind::NotFound => {},
                Err(error) => return Err(module_error(&import.location,
                    format!("Cannot resolve '{}': {}", file.display(), error))),
            }
        }
        Ok(None)
    }

    fn module_file(&self, import: &AstImportStatement, directory: &Path)
        -> InterpreterResult<PathBuf>
    {
        if import.module == "std::prelude" && !self.prelude_loading
        {
            // An existing module keeps its own prelude generation after a reload.
            if let Some(key) = self.scope().modules.get("std::prelude")
            { return Ok(PathBuf::from(key)); }
            if let Some(prelude) = &self.prelude
            { return Ok(PathBuf::from(&prelude.scope)); }
        }
        self.find_module_file(import, directory)?.ok_or_else(|| module_error(&import.location,
            format!("Module '{}' was not found", import.module)))
    }

    pub(super) fn initialize_prelude(&mut self) -> InterpreterResult<()>
    {
        if self.prelude.is_some() { return Ok(()); }
        let location = Location::new("<prelude>", 1, 1);
        let import = AstImportStatement
            {
                location: location.clone(), module: "std::prelude".into(), names: Vec::new(),
                condition: None, enabled: true,
            };
        let Some(file) = self.find_module_file(&import, Path::new("."))? else { return Ok(()); };
        // No implicit prelude is installed while bootstrapping the prelude itself
        // and its dependencies. Explicit circular imports still fail normally.
        self.prelude_loading = true;
        let loaded = self.load_module(&file, &location);
        self.prelude_loading = false;
        let key = loaded?;
        if self.halted { return Ok(()); }
        let scope = &self.scopes[&key];
        let types = scope.exports.iter().filter_map(|name|
            scope.types.names.get(name).map(|id| (name.clone(), scope.types.get(*id)))).collect();
        self.prelude = Some(Prelude { scope: key.clone(), types });
        let loaded: Vec<_> = self.scopes.keys().filter(|name| **name != key).cloned().collect();
        for name in loaded
        {
            let caller = replace(&mut self.current_scope, name);
            let result = self.install_prelude(&location);
            self.current_scope = caller;
            result?;
        }
        self.refresh_module_names();
        Ok(())
    }

    pub(super) fn handle_prelude_reload(&mut self, location: &Location, args: &[Value])
        -> InterpreterResult<()>
    {
        if !args.is_empty()
        {
            return Err(InterpreterError { location: location.clone(),
                what: ErrorWhat::ArgumentMismatch("prelude_reload expects no arguments.".into()) });
        }
        if self.prelude_loading
        { return Err(module_error(location, "Cannot reload the prelude while it is loading")); }
        let import = AstImportStatement
            {
                location: location.clone(), module: "std::prelude".into(), names: Vec::new(),
                condition: None, enabled: true,
            };
        let file = self.find_module_file(&import, Path::new("."))?
            .ok_or_else(|| module_error(location, "Module 'std::prelude' was not found"))?;
        self.prelude_generation += 1;
        let key = format!("<prelude:{}:{}>", self.prelude_generation, file.display());
        self.prelude_loading = true;
        let loaded = self.load_module_as(&file, location, key);
        self.prelude_loading = false;
        let key = loaded?;
        if self.halted { return Ok(()); }
        let scope = &self.scopes[&key];
        let types = scope.exports.iter().filter_map(|name|
            scope.types.names.get(name).map(|id| (name.clone(), scope.types.get(*id)))).collect();
        let previous = self.prelude.replace(Prelude { scope: key, types });
        let checkpoint = self.scope().checkpoint();
        // Only remove bindings still belonging to this scope's previous implicit prelude.
        // A declaration that shadows one of those bindings must not be overwritten.
        for (name, id) in take(&mut self.scope_mut().prelude_types)
        {
            let scope = self.scope_mut();
            if scope.types.names.get(&name) == Some(&id)
            {
                scope.types.names.remove(&name);
                scope.imported_types.remove(&name);
                scope.exports.remove(&name);
            }
        }
        if let Err(error) = self.install_prelude(location)
        {
            self.scopes.insert(self.current_scope.clone(), checkpoint);
            self.prelude = previous;
            return Err(error);
        }
        self.refresh_module_names();
        self.last_result = Some(Value::None);
        Ok(())
    }

    fn install_prelude(&mut self, location: &Location) -> InterpreterResult<()>
    {
        if self.prelude_loading { return Ok(()); }
        let Some(prelude) = self.prelude.clone() else { return Ok(()); };
        if self.current_scope == prelude.scope { return Ok(()); }
        for (name, definition) in &prelude.types
        {
            if self.scope().types.names.get(name).is_some_and(|id| *id != definition.id)
            {
                return Err(module_error(location,
                    format!("Prelude type '{}' conflicts with an existing type", name)));
            }
        }
        let scope = self.scope_mut();
        scope.types.names.extend(prelude.types.iter().map(|(name, item)| (name.clone(), item.id)));
        scope.prelude_types = prelude.types.iter()
            .map(|(name, item)| (name.clone(), item.id)).collect();
        scope.exports.extend(prelude.types.keys().cloned());
        scope.imported_types.extend(prelude.types);
        scope.modules.insert("std::prelude".into(), prelude.scope);
        scope.types.module_names.insert("std".into());
        Ok(())
    }

    fn load_module(&mut self, file: &Path, location: &Location) -> InterpreterResult<String>
    {
        let key = file.to_string_lossy().into_owned();
        self.load_module_as(file, location, key)
    }

    fn load_module_as(&mut self, file: &Path, location: &Location, key: String)
        -> InterpreterResult<String>
    {
        // Diagnostics and circular-import detection use the source path, while
        // functions retain the distinct scope key of their loaded generation.
        let origin = file.to_string_lossy().into_owned();
        if self.loading_modules.contains(&origin)
        { return Err(module_error(location, format!("Circular import of '{}'", file.display()))); }
        if self.scopes.contains_key(&key) { return Ok(key); }
        self.loading_modules.insert(origin.clone());
        let result = (||
            {
                let file = File::open(file).map_err(|error|
                    module_error(location, format!("Cannot open '{}': {}", key, error)))?;
                let mut reader = BufReader::new(file);
                let mut buffer = ReadBuffer::new(&origin, &mut reader, Some(4));
                let mut statements = parse_text(&mut Tokenizer::new(&mut buffer))?;
                // Conditions see the caller before the new module's scope exists.
                let imports = self.import_conditions(&mut statements)?;
                let mut scope = Scope::new(self.native_functions.values().cloned(), key.clone(),
                    self.scope().types.module_registry());
                // Inherit shell settings as values, not aliases into the caller's bindings.
                for (name, mut binding) in self.scope().variables.get_all_flattened()
                {
                    if binding.exported == ValueVisibility::Exported || matches!(name.as_str(),
                        "$OS" | "$os" | "$shelly" | "$version" | "$build_date" | "$build_time"
                        | "$interactive" | "$login" | "$SHELLY_MODULE_PATH" | "$SHELLY_STD_PATH")
                    {
                        binding.value = self.read_raw_variable(&name, location)?;
                        binding.reference = None;
                        let _ = scope.variables.create(name, binding);
                    }
                }
                let _ = scope.variables.create("$args".into(), ScopedValue
                    {
                        value: Value::from_array(Vec::new()), type_id: None,
                        exported: ValueVisibility::Private, reference: None,
                    });
                self.scopes.insert(key.clone(), scope);
                let caller = replace(&mut self.current_scope, key.clone());
                let previous = self.last_result.take();
                let directory = Self::source_directory(&origin);
                let module_location = Location::new(origin.as_str(), 1, 1);
                let loaded = self.install_prelude(&module_location)
                    .and_then(|()| self.compile_module(&mut statements, imports, &directory))
                    .and_then(|code| self.execute_instructions(&code));
                self.last_result = previous;
                self.current_scope = caller;
                if loaded.is_err() { self.scopes.remove(&key); }
                loaded
            })();
        self.loading_modules.remove(&origin);
        result?;
        Ok(key)
    }

    fn install_import(&mut self, import: &AstImportStatement, key: &str) -> InterpreterResult<()>
    {
        if self.scope().modules.get(&import.module).is_some_and(|existing| existing != key)
        {
            return Err(module_error(&import.location,
                format!("Module '{}' is already bound", import.module)));
        }
        for name in &import.names
        {
            let module = &self.scopes[key];
            if !module.exports.contains(name)
            {
                return Err(module_error(&import.location,
                    format!("Module '{}' has no member '{}'", import.module, name)));
            }
            if name.starts_with('$')
            {
                let binding = module.variables.binding(name).ok_or_else(|| module_error(
                    &import.location, format!("Module variable '{}' is unavailable", name)))?;
                if    let Some(existing) = self.scope().variables.binding(name)
                   && !Rc::ptr_eq(&existing, &binding)
                {
                    return Err(module_error(&import.location,
                        format!("Variable '{}' is already bound", name)));
                }
                self.scope_mut().variables.import(name.clone(), binding);
            }
            else if let Some(id) = module.types.names.get(name)
            {
                let definition = module.types.get(*id);
                if self.scope().types.names.get(name)
                    .is_some_and(|existing| *existing != definition.id)
                {
                    return Err(module_error(&import.location,
                        format!("Type '{}' is already bound", name)));
                }
                self.scope_mut().types.names.insert(name.clone(), definition.id);
                self.scope_mut().imported_types.insert(name.clone(), definition);
            }
            else if let Some(function) = module.native_functions.get(name).cloned()
            {
                self.publish_native(&import.location, function, false)?;
            }
            else if let Some(function) = module.base_function(name)
            {
                if self.scope().native_functions.contains_key(name)
                {
                    return Err(module_error(&import.location,
                        format!("Function '{}' is already bound", name)));
                }
                if    let Some(existing) = self.scope().base_function(name)
                   && !Rc::ptr_eq(&existing, &function)
                {
                    return Err(module_error(&import.location,
                        format!("Function '{}' is already bound", name)));
                }
                self.scope_mut().import_function(name.clone(), function);
            }
            else
            {
                return Err(module_error(&import.location,
                    format!("Member '{}' cannot be imported", name)));
            }
            self.scope_mut().exports.insert(name.clone());
        }
        self.scope_mut().modules.insert(import.module.clone(), key.to_string());
        Ok(())
    }

    fn refresh_module_names(&mut self)
    {
        fn collect(scopes: &HashMap<String, Scope>, scope: &Scope, prefix: &str,
                   names: &mut HashMap<String, TypeId>, functions: &mut HashSet<String>,
                   visiting: &mut HashSet<String>)
        {
            for (name, key) in &scope.modules
            {
                if !visiting.insert(key.clone()) { continue; }
                let module = &scopes[key];
                let prefix = format!("{}{}::", prefix, name);
                for name in &module.exports
                {
                    if let Some(id) = module.types.names.get(name)
                    { names.insert(format!("{}{}", prefix, name), *id); }
                    if module.base_function(name).is_some()
                        || module.native_functions.contains_key(name)
                    { functions.insert(format!("{}{}", prefix, name)); }
                }
                collect(scopes, module, &prefix, names, functions, visiting);
                visiting.remove(key);
            }
        }
        let mut names = HashMap::new();
        let mut functions = HashSet::new();
        let mut visiting = HashSet::from([self.current_scope.clone()]);
        collect(&self.scopes, self.scope(), "", &mut names, &mut functions, &mut visiting);
        let extensions: Vec<_> = names.values().filter_map(|id|
            {
                Some((*id, self.scopes.get(self.type_scopes.get(id)?)?.types.clone()))
            }).collect();
        let types = &mut self.scope_mut().types;
        for (id, source) in extensions { types.import_extensions(&source, id); }
        types.names.retain(|name, _| !name.contains("::"));
        types.names.extend(names);
        types.qualified_functions = functions;
    }

    fn compile_module(&mut self, statements: &mut AstTopLevel, imports: Vec<AstImportStatement>,
                      directory: &Path) -> InterpreterResult<Vec<Instruction>>
    {
        let checkpoint = self.scope().checkpoint();
        let result = (||
            {
                for import in imports
                {
                    self.scope_mut().types.module_names
                        .insert(import.module.split("::").next().unwrap().to_string());
                    if !import.enabled { continue; }
                    let file = self.module_file(&import, directory)?;
                    let key = self.load_module(&file, &import.location)?;
                    self.install_import(&import, &key)?;
                }
                self.refresh_module_names();
                self.prepare_visibility(statements)?;
                let first_type = self.scope().types.definition_count();
                let code = self.scope_mut().compile(statements)?;
                for index in first_type..self.scope().types.definition_count()
                { self.type_scopes.insert(TypeId(index), self.current_scope.clone()); }
                for statement in statements
                {
                    let name = match statement
                        {
                            AstStatement::LetStatement(item) => &item.identifier,
                            AstStatement::FunctionDefinition(item) if item.receiver.is_none()
                                => &item.name,
                            AstStatement::EnumDeclaration(item) => &item.name,
                            AstStatement::StructDeclaration(item) => &item.name,
                            AstStatement::TypeDeclaration(item) => &item.name,
                            _ => continue,
                        };
                    self.scope_mut().exports.insert(name.clone());
                }
                Ok(code)
            })();
        if result.is_err() { self.scopes.insert(self.current_scope.clone(), checkpoint); }
        result
    }

    pub(super) fn execute_submission(&mut self, mut statements: AstTopLevel, origin: &str)
        -> InterpreterResult<()>
    {
        let directory = Self::source_directory(origin);
        let key = canonicalize(origin).ok().map(|path| path.to_string_lossy().into_owned());
        if let Some(key) = &key { self.loading_modules.insert(key.clone()); }
        let result = (||
            {
                let imports = self.import_conditions(&mut statements)?;
                let code = self.compile_module(&mut statements, imports, &directory)?;
                self.execute_instructions(&code)
            })();
        if let Some(key) = key { self.loading_modules.remove(&key); }
        result
    }
}
