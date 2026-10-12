
use std::{ collections::HashSet, sync::Arc };

use crate::language::{ analysis::{ SymbolDescriptor, SymbolKind, SymbolSnapshot, TypeDescriptor },
                       data::{ methods::method_key, types::{ TypeId, TypeKind }, value::Value },
                       interpreter::Interpreter };


impl Interpreter
{
    /**
     * Copy already registered information for tooling. In particular, value_type and
     * inferred_value_type may intern types, so this path must not call either one.
     */
    pub fn symbol_snapshot(&self) -> SymbolSnapshot
    {
        let mut snapshot = SymbolSnapshot::default();
        let registry = &self.scope().types;
        let mut types = Vec::with_capacity(registry.definition_count());
        for index in 0..registry.definition_count()
        {
            let id = TypeId(index);
            let definition = registry.get(id);
            let mut descriptor = TypeDescriptor
                {
                    name: definition.name.clone(),
                    ..TypeDescriptor::default()
                };
            match &definition.kind
            {
                TypeKind::Named(inner) => descriptor.parent = Some(inner.0),
                TypeKind::Array(item) =>
                    {
                        descriptor.parent = registry.builtin_id("Array").map(|id| id.0);
                        descriptor.item = Some(item.0);
                    },
                TypeKind::FixedArray(items) =>
                    {
                        descriptor.parent = registry.builtin_id("Array").map(|id| id.0);
                        descriptor.item = items.first().filter(|first|
                            items.iter().all(|id| id == *first)).copied()
                            .or_else(|| registry.builtin_id("any")).map(|id| id.0);
                    },
                TypeKind::Map(key, item) =>
                    {
                        descriptor.parent = registry.builtin_id("HashMap").map(|id| id.0);
                        descriptor.key = Some(key.0);
                        descriptor.item = Some(item.0);
                    },
                TypeKind::Struct(fields) =>
                    for field in fields
                    {
                        descriptor.fields.insert(field.name.clone(),
                            SymbolDescriptor::new(SymbolKind::Field, Some(field.type_id.0)));
                    },
                TypeKind::Enum(variants) =>
                    descriptor.variants.extend(variants.iter().map(|variant| variant.name.clone())),
                TypeKind::Function(_, result) => descriptor.call_result = Some(result.0),
                _ => {},
            }
            for name in registry.method_names(id)
            {
                for receiver in registry.method_types(id)
                {
                    let key = method_key(receiver, &name);
                    let function = self.scope().lexical_function(&key)
                        .or_else(|| self.module_method(receiver, &key));
                    let result = if let Some(function) = function
                        { function.return_type.map(|id| id.0) }
                        else if let Some(method) = registry.registered_method(receiver, &name)
                        { Some(method.return_type.0) }
                        else { continue; };
                    descriptor.methods.insert(name.clone(), SymbolDescriptor
                        {
                            kind: SymbolKind::Method, type_id: None, return_type: result
                        });
                    break;
                }
            }
            types.push(descriptor);
        }
        snapshot.types = Arc::new(types);
        for (name, id) in &registry.names
        {
            snapshot.symbols.insert(name.clone(),
                SymbolDescriptor::new(SymbolKind::Type, Some(id.0)));
        }
        for name in self.scope().function_names()
        {
            if let Some(function) = self.scope().function(&name)
            {
                snapshot.symbols.insert(name, SymbolDescriptor
                    {
                        kind: SymbolKind::Function,
                        type_id: None, return_type: function.return_type.map(|id| id.0)
                    });
            }
        }
        // Alias expansion precedes native and scripted command lookup.
        for name in self.scope().native_functions.keys()
        {
            snapshot.symbols.insert(name.clone(), SymbolDescriptor::new(SymbolKind::Builtin, None));
        }
        for name in self.scope().aliases.keys()
        {
            snapshot.symbols.insert(name.clone(), SymbolDescriptor::new(SymbolKind::Alias, None));
        }
        for name in self.variable_names()
        {
            let type_id = self.variable_binding(&name).and_then(|binding|
                {
                    let binding = binding.borrow();
                    binding.type_id.or_else(|| binding.value.collection_type())
                        .map(|id| id.0).or_else(|| match &binding.value
                            {
                                Value::Type(_) => registry.builtin_id("Type").map(|id| id.0),
                                Value::Named(item) => Some(item.definition.id.0),
                                Value::Struct(item) => Some(item.definition.id.0),
                                Value::Enum(item) => Some(item.definition.id.0),
                                _ => registry.names.get(&binding.value.type_name()).map(|id| id.0),
                            })
                });
            snapshot.symbols.insert(name, SymbolDescriptor::new(SymbolKind::Variable, type_id));
        }
        self.snapshot_modules(&mut snapshot, "", &self.current_scope, &mut HashSet::new(), true);
        snapshot
    }

    fn snapshot_modules(&self, snapshot: &mut SymbolSnapshot, prefix: &str, home: &str,
                        visiting: &mut HashSet<String>, local: bool)
    {
        if visiting.len() >= 64 || !visiting.insert(home.to_string()) { return; }
        let scope = &self.scopes[home];
        if !local
        {
            for name in &scope.exports
            {
                let qualified = if let Some(name) = name.strip_prefix('$')
                    { format!("${}{}", prefix, name) }
                    else { format!("{}{}", prefix, name) };
                let descriptor = if let Some(id) = scope.types.names.get(name)
                    { Some(SymbolDescriptor::new(SymbolKind::Type, Some(id.0))) }
                    else if scope.aliases.contains_key(name)
                    { Some(SymbolDescriptor::new(SymbolKind::Alias, None)) }
                    else if scope.native_functions.contains_key(name)
                    { Some(SymbolDescriptor::new(SymbolKind::Builtin, None)) }
                    else if let Some(function) = scope.base_function(name)
                    { Some(SymbolDescriptor
                        {
                            kind: SymbolKind::Function, type_id: None,
                            return_type: function.return_type.map(|id| id.0)
                        }) }
                    else if let Some(binding) = scope.variables.get(name)
                    { Some(SymbolDescriptor::new(SymbolKind::Variable,
                        binding.type_id.map(|id| id.0))) }
                    else { None };
                if let Some(descriptor) = descriptor
                { snapshot.symbols.insert(qualified, descriptor); }
            }
        }
        for (name, module) in &scope.modules
        {
            if !local && !scope.exports.contains(name) { continue; }
            let qualified = format!("{}{}", prefix, name);
            snapshot.symbols.insert(qualified.clone(),
                SymbolDescriptor::new(SymbolKind::Module, None));
            self.snapshot_modules(snapshot, &format!("{}::", qualified), module, visiting, false);
        }
        visiting.remove(home);
    }
}
