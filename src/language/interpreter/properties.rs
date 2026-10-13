
use std::rc::Rc;

use crate::language::{ data::{ conversion::is_conversion_method,
                               map_key::MapKey, methods::BoundMethod,
                               value::{ Executable, Value } },
                       interpreter::Interpreter };


impl Interpreter
{
    pub(super) fn add_property_methods(&self, receiver: &Value, method: &BoundMethod,
                                      properties: &mut Value)
    {
        let Value::HashMap(members) = properties else { unreachable!(); };
        let members = Rc::make_mut(members);
        let id = self.scope().types.value_type(receiver);
        members.insert(MapKey::String("type".into()), Value::Type(self.scope().types.get(id)));
        for name in ["key_type", "value_type"]
        {
            if let Some(value) = self.collection_property(
                &Default::default(), &method.receiver, receiver, name)
            { members.insert(MapKey::String(name.into()), value); }
        }
        let mut names = self.scope().types.method_names(id);
        let selected = method.receiver.dispatch_type.or_else(||
            {
                if method.receiver.indexes.is_empty()
                { let root = method.receiver.root.borrow(); root.type_id.or(root.dispatch_type) }
                else { None }
            });
        if let Some(id) = selected.and_then(|id| self.scope().types.union_interface(id))
        { names.extend(self.scope().types.method_names(id)); }
        let mut underlying = receiver;
        while let Value::Named(item) = underlying
        {
            underlying = &item.value;
            names.extend(self.scope().types.method_names(
                self.scope().types.value_type(underlying)));
        }
        if let Value::Struct(item) = underlying
        {
            for (owner, name) in self.scope().types.properties.keys()
            {
                if *owner != item.definition.id { continue; }
                let key = MapKey::String(name.clone());
                members.remove(&key);
                if let Some(bound) = self.resolve_method(receiver, method.receiver.clone(),
                    &format!("get_{}", name), &method.snapshot)
                {
                    members.insert(key, Value::String(
                        format!("{}.{}", receiver.type_name(), name),
                        Executable::Method(Rc::new(bound))));
                }
            }
        }
        // A saved reflector keeps lexical method versions even after a module reload.
        if let Value::HashMap(snapshot) = &method.snapshot
        {
            names.extend(snapshot.keys().filter_map(|key| match key
                {
                    MapKey::String(key) => key.rsplit(':').next().map(str::to_string),
                    _ => None,
                }));
        }
        for name in names
        {
            let key = MapKey::String(name.clone());
            if is_conversion_method(&name) || name == "type" { continue; }
            if let Some(bound) = self.resolve_method(receiver, method.receiver.clone(),
                                                    &name, &method.snapshot)
            {
                if members.contains_key(&key) && !self.is_union_method(&bound) { continue; }
                if self.collection_property(&Default::default(), &method.receiver,
                    receiver, &name).is_some() { continue; }
                members.insert(key, Value::String(format!("{}.{}", receiver.type_name(), name),
                    Executable::Method(Rc::new(bound))));
            }
        }
    }
}
