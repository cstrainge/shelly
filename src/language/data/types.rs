use std::{ collections::HashMap, rc::Rc };

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
                     "Array", "HashMap", "Range", "ArgumentExpansion"]
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
