use std::{ collections::{HashMap, HashSet}, rc::Rc };

use crate::language::text::location::Location;


#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(pub usize);


#[derive(Debug)]
pub struct EnumVariant
{
    pub name: String,
    pub location: Location
}


#[derive(Debug)]
pub enum TypeKind
{
    Builtin,
    Pending,
    Array(TypeId),
    Map(TypeId, TypeId),
    Optional(TypeId),
    Struct(Vec<FieldDefinition>),
    Enum(Vec<EnumVariant>)
}


#[derive(Debug)]
pub struct TypeDefinition
{
    pub id: TypeId,
    pub name: String,
    pub kind: TypeKind,
    pub location: Option<Location>
}


#[derive(Debug)]
pub struct FieldDefinition
{
    pub name: String,
    pub type_id: TypeId,
    pub optional: bool,
    pub location: Location
}


#[derive(Clone, Debug, PartialEq)]
pub struct StructValue
{
    pub definition: Rc<TypeDefinition>,
    pub fields: Vec<super::value::Value>
}

// Definitions compare by identity, including when embedded in canonical map keys.
impl PartialEq for TypeDefinition
{
    fn eq(&self, other: &Self) -> bool
    {
        self.id == other.id
    }
}
impl Eq for TypeDefinition {}
impl PartialOrd for TypeDefinition
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering>
    {
        Some(self.cmp(other))
    }
}
impl Ord for TypeDefinition
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering
    {
        self.id.cmp(&other.id)
    }
}
impl std::hash::Hash for TypeDefinition
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H)
    {
        self.id.hash(state);
    }
}


// A value keeps its definition alive even after its lexical name goes out of scope.
#[derive(Clone, Debug)]
pub struct EnumValue
{
    pub definition: Rc<TypeDefinition>,
    pub variant: usize
}


impl PartialEq for EnumValue
{
    fn eq(&self, other: &Self) -> bool
    {
        self.definition.id == other.definition.id && self.variant == other.variant
    }
}

impl Eq for EnumValue {}

impl PartialOrd for EnumValue
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering>
    {
        Some(self.cmp(other))
    }
}

impl Ord for EnumValue
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering
    {
        (self.definition.id, self.variant).cmp(&(other.definition.id, other.variant))
    }
}

impl std::hash::Hash for EnumValue
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H)
    {
        self.definition.id.hash(state);
        self.variant.hash(state);
    }
}

impl std::fmt::Display for EnumValue
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        let TypeKind::Enum(variants) = &self.definition.kind else { return Err(std::fmt::Error); };
        write!(f, "{}::{}", self.definition.name, variants[self.variant].name)
    }
}


// Shared by all submissions to one interpreter. Clone to stage a compilation;
// commit only after checking, code generation, optimization, and linking succeed.
#[derive(Clone)]
pub struct TypeRegistry
{
    definitions: Vec<Rc<TypeDefinition>>,
    pub names: HashMap<String, TypeId>
}


impl TypeRegistry
{
    pub fn new() -> Self
    {
        let mut registry = Self { definitions: Vec::new(), names: HashMap::new() };
        for name in ["None", "ExecResult", "Integer", "Float", "Boolean", "String",
                     "Array", "HashMap", "Range", "ArgumentExpansion", "Number", "any"]
        {
            let id = registry.register(name.to_string(), TypeKind::Builtin, None);
            registry.names.insert(name.to_string(), id);
        }
        registry
    }

    pub fn register(&mut self, name: String, kind: TypeKind, location: Option<Location>) -> TypeId
    {
        let id = TypeId(self.definitions.len());
        self.definitions.push(Rc::new(TypeDefinition { id, name, kind, location }));
        id
    }

    pub fn get(&self, id: TypeId) -> Rc<TypeDefinition>
    {
        self.definitions[id.0].clone()
    }
}


impl TypeRegistry
{
    // Only uncommitted placeholders are completed; old definitions are immutable.
    pub fn finish(&mut self, id: TypeId, kind: TypeKind)
    {
        let old = &self.definitions[id.0];
        self.definitions[id.0] = Rc::new(TypeDefinition
            { id, name: old.name.clone(), location: old.location.clone(), kind });
    }

    pub fn intern(&mut self, kind: TypeKind) -> TypeId
    {
        if let Some(existing) = self.definitions.iter().find(|definition| match (&definition.kind, &kind)
            {
                (TypeKind::Array(a), TypeKind::Array(b)) | (TypeKind::Optional(a), TypeKind::Optional(b)) => a == b,
                (TypeKind::Map(a, b), TypeKind::Map(c, d)) => a == c && b == d,
                _ => false
            }) { return existing.id; }
        let name = match kind
            {
                TypeKind::Array(id) => format!("[{}]", self.get(id).name),
                TypeKind::Map(key, value) => format!("[{}: {}]", self.get(key).name, self.get(value).name),
                TypeKind::Optional(id) => format!("optional {}", self.get(id).name),
                _ => unreachable!("Only anonymous container constraints are interned")
            };
        self.register(name, kind, None)
    }

    pub fn validate(&self, id: TypeId, value: &super::value::Value) -> Result<(), String>
    {
        use super::value::Value;
        let definition = self.get(id);
        let valid = match &definition.kind
            {
                TypeKind::Builtin => match definition.name.as_str()
                    {
                        "any" => true,
                        "Number" => matches!(value, Value::Integer(_) | Value::Float(_, _)),
                        "Integer" => matches!(value, Value::Integer(_)),
                        "Float" => matches!(value, Value::Float(_, _)),
                        "String" => matches!(value, Value::String(_, _)),
                        "Boolean" => matches!(value, Value::Boolean(_)),
                        "None" => matches!(value, Value::None),
                        "ExecResult" => matches!(value, Value::ExecResult(_)),
                        "Array" => matches!(value, Value::Array(_)),
                        "HashMap" => matches!(value, Value::HashMap(_)),
                        "Range" => matches!(value, Value::Range(_)),
                        "ArgumentExpansion" => matches!(value, Value::ArgumentExpansion(_)),
                        _ => false
                    },
                TypeKind::Enum(_) => matches!(value, Value::Enum(item) if item.definition.id == id),
                TypeKind::Struct(fields) => if let Value::Struct(item) = value
                    {
                        if item.definition.id != id { false }
                        else
                        {
                            for (field, value) in fields.iter().zip(&item.fields)
                            {
                                self.validate(field.type_id, value).map_err(|error|
                                    format!("Field '{}.{}' (declared at {}): {}", definition.name, field.name, field.location, error))?;
                            }
                            fields.len() == item.fields.len()
                        }
                    } else { false },
                TypeKind::Optional(inner) =>
                    {
                        if matches!(value, Value::None) { true }
                        else { return self.validate(*inner, value); }
                    },
                TypeKind::Array(element) => if let Value::Array(values) = value
                    {
                        for (index, value) in values.iter().enumerate()
                        {
                            self.validate(*element, value).map_err(|error| format!("Array element {}: {}", index, error))?;
                        }
                        true
                    } else { false },
                TypeKind::Map(key_type, value_type) => if let Value::HashMap(values) = value
                    {
                        for (key, value) in values.iter()
                        {
                            self.validate_key(*key_type, key).map_err(|error| format!("Map key: {}", error))?;
                            self.validate(*value_type, value).map_err(|error| format!("Map value: {}", error))?;
                        }
                        true
                    } else { false },
                TypeKind::Pending => false
            };
        if valid { Ok(()) } else { Err(format!("Expected {}, got {}", definition.name, value.type_name())) }
    }

    // Validate every struct touched by an update, including structs inside untyped containers.
    pub fn validate_value(&self, value: &super::value::Value) -> Result<(), String>
    {
        use super::value::Value;
        match value
        {
            Value::Struct(item) =>
                {
                    self.validate(item.definition.id, value)?;
                    for value in &item.fields { self.validate_value(value)?; }
                },
            Value::Array(values) | Value::ArgumentExpansion(values) =>
                { for value in values.iter() { self.validate_value(value)?; } },
            Value::HashMap(values) => { for value in values.values() { self.validate_value(value)?; } },
            _ => {}
        }
        Ok(())
    }

    pub fn check_required_cycles(&self, roots: &[TypeId]) -> Result<(), String>
    {
        fn visit(registry: &TypeRegistry, id: TypeId, path: &mut Vec<TypeId>, done: &mut HashSet<TypeId>) -> Result<(), String>
        {
            if let Some(start) = path.iter().position(|item| *item == id)
            {
                let mut names: Vec<_> = path[start..].iter().map(|id| registry.get(*id).name.clone()).collect();
                names.push(registry.get(id).name.clone());
                return Err(format!("Required struct field cycle: {}. Use optional or a container to terminate recursion", names.join(" -> ")));
            }
            if done.contains(&id) { return Ok(()); }
            if let TypeKind::Struct(fields) = &registry.get(id).kind
            {
                path.push(id);
                for field in fields
                {
                    if matches!(registry.get(field.type_id).kind, TypeKind::Struct(_))
                    { visit(registry, field.type_id, path, done)?; }
                }
                path.pop();
            }
            done.insert(id);
            Ok(())
        }
        let mut done = HashSet::new();
        for root in roots { visit(self, *root, &mut Vec::new(), &mut done)?; }
        Ok(())
    }
}


impl TypeRegistry
{
    // Key annotations describe equivalence classes: integral floats and integers
    // already share one canonical key, including within collection keys.
    fn validate_key(&self, id: TypeId, key: &super::map_key::MapKey) -> Result<(), String>
    {
        use super::{ map_key::MapKey, value::Value };
        let definition = self.get(id);
        match (&definition.kind, key)
        {
            (TypeKind::Builtin, MapKey::Integer(integer)) if definition.name == "Float" =>
                {
                    let float = Value::Float(*integer as f64, None);
                    if float.equals(&Value::Integer(*integer)) { return Ok(()); }
                },
            (TypeKind::Optional(inner), _) if !matches!(key, MapKey::None) => return self.validate_key(*inner, key),
            (TypeKind::Array(element), MapKey::Array(values)) =>
                {
                    for value in values.iter() { self.validate_key(*element, value)?; }
                    return Ok(());
                },
            (TypeKind::Map(key_type, value_type), MapKey::HashMap(entries)) =>
                {
                    for (key, value) in entries.iter()
                    { self.validate_key(*key_type, key)?; self.validate_key(*value_type, value)?; }
                    return Ok(());
                },
            _ => {}
        }
        self.validate(id, &key.to_value())
    }
}
