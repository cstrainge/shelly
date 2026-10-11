
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
        let mut names = self.scope().types.method_names(id);
        let mut underlying = receiver;
        while let Value::Named(item) = underlying
        {
            underlying = &item.value;
            names.extend(self.scope().types.method_names(
                self.scope().types.value_type(underlying)));
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
            if members.contains_key(&key) || is_conversion_method(&name) { continue; }
            if let Some(bound) = self.resolve_method(receiver, method.receiver.clone(),
                                                    &name, &method.snapshot)
            {
                members.insert(key, Value::String(format!("{}.{}", receiver.type_name(), name),
                    Executable::Method(Rc::new(bound))));
            }
        }
    }
}
