
use std::{ cell::RefCell, collections::{ HashMap, HashSet }, rc::Rc };

use crate::language::{ ast::AstTopLevel,
                       native::{ NativeFunction, NativeVisibility },
                       bytecode::{ FunctionBlock, FunctionBlockRef, FunctionRef, Instruction },
                       compiler::{ CompileResult, CompileTarget, compile_ast },
                       data::{ scoped_variables::ScopedVariables,
                               types::{ TypeDefinition, TypeId, TypeRegistry } } };


#[derive(Clone)]
pub struct Alias
{
    pub name: String,
    pub arguments: Vec<String>
}


// Persistent script state. Variables follow the caller's runtime scopes; types and
// functions keep the lexical identities and snapshots established during compilation.
pub(super) struct Scope
{
    pub variables: ScopedVariables,
    pub types: TypeRegistry,
    pub aliases: HashMap<String, Alias>,
    pub modules: HashMap<String, String>,
    pub native_functions: HashMap<String, Rc<NativeFunction>>,
    pub exports: HashSet<String>,
    pub prelude_types: HashMap<String, TypeId>,
    pub imported_types: HashMap<String, Rc<TypeDefinition>>,
    base_function_block: FunctionBlockRef,
    current_function_block: Option<FunctionBlockRef>,
}


impl Scope
{
    pub fn new(natives: impl IntoIterator<Item = Rc<NativeFunction>>,
               scope: String, types: TypeRegistry) -> Self
    {
        let native_functions: HashMap<_, _> = natives.into_iter()
            .filter(|item| item.visibility != NativeVisibility::Hidden)
            .map(|item| (item.name.to_string(), item)).collect();
        let builtins = native_functions.values().map(|item| item.name).collect();
        Self
            {
                variables: ScopedVariables::new_from_environment(),
                types,
                aliases: HashMap::new(),
                modules: HashMap::new(),
                native_functions,
                exports: HashSet::new(),
                imported_types: HashMap::new(),
                prelude_types: HashMap::new(),
                base_function_block: Rc::new(RefCell::new(FunctionBlock
                    {
                        scope,
                        parent: None,
                        function_name: None,
                        declared_functions: HashSet::new(),
                        builtins: Rc::new(builtins),
                        functions: HashMap::new(),
                    })),
                current_function_block: None,
            }
    }

    pub fn checkpoint(&self) -> Self
    {
        Self
            {
                variables: self.variables.clone(),
                types: self.types.clone(),
                aliases: self.aliases.clone(),
                modules: self.modules.clone(),
                native_functions: self.native_functions.clone(),
                exports: self.exports.clone(),
                imported_types: self.imported_types.clone(),
                prelude_types: self.prelude_types.clone(),
                base_function_block: Rc::new(RefCell::new(
                    self.base_function_block.borrow().clone())),
                current_function_block: self.current_function_block.clone(),
            }
    }

    pub fn import_native(&mut self, function: Rc<NativeFunction>)
    {
        let mut block = self.base_function_block.borrow_mut();
        Rc::make_mut(&mut block.builtins).insert(function.name);
        self.native_functions.insert(function.name.to_string(), function);
    }

    pub fn import_function(&mut self, name: String, function: FunctionRef)
    {
        self.base_function_block.borrow_mut().functions.insert(name, function);
    }

    pub fn compile(&mut self, statements: &mut AstTopLevel) -> CompileResult<Vec<Instruction>>
    {
        // The compiler stages both registries and publishes only a valid submission.
        compile_ast(&mut self.types, &self.base_function_block, &self.variables,
                    statements, CompileTarget::Toplevel)
    }

    pub fn compile_condition(&mut self, statements: &mut AstTopLevel)
        -> CompileResult<Vec<Instruction>>
    {
        // Preserve the final value so the loader can decide whether to import.
        compile_ast(&mut self.types, &self.base_function_block, &self.variables,
                    statements, CompileTarget::Function)
    }

    pub fn base_function(&self, name: &str) -> Option<FunctionRef>
    {
        self.base_function_block.borrow().functions.get(name).cloned()
    }

    pub fn function(&self, name: &str) -> Option<FunctionRef>
    {
        self.lexical_function(name).or_else(|| self.base_function(name))
    }

    pub fn lexical_function(&self, name: &str) -> Option<FunctionRef>
    {
        // Method lookup uses only this snapshot; ordinary command lookup may also
        // fall back to definitions published by later top-level submissions.
        let mut block = self.current_function_block.as_ref()
            .unwrap_or(&self.base_function_block).clone();
        loop
        {
            let parent =
                {
                    let block = block.borrow();
                    if let Some(function) = block.functions.get(name)
                    { return Some(function.clone()); }
                    block.parent.clone()
                };
            block = parent?;
        }
    }

    pub fn in_function(&self) -> bool
    {
        self.current_function_block.is_some()
    }

    pub fn enter_function(&mut self, functions: FunctionBlockRef) -> Option<FunctionBlockRef>
    {
        self.variables.push_scope();
        self.current_function_block.replace(functions)
    }

    pub fn exit_function(&mut self, caller: Option<FunctionBlockRef>)
    {
        self.current_function_block = caller;
        self.variables.pop_scope();
    }
}
