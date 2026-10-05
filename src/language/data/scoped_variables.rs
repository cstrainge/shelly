
use std::collections::{ HashMap, VecDeque };

use crate::language::data::value::Value;


#[derive(Clone, PartialEq, Eq)]
pub enum ValueVisibility
{
    Private,
    Exported
}


#[derive(Clone)]
pub struct ScopedValue
{
    pub value: Value,
    pub exported: ValueVisibility
}


pub struct ScopedVariables
{
    scopes: VecDeque<HashMap<String, ScopedValue>>,
}


impl ScopedVariables
{
    pub fn new_from_environment() -> Self
    {
        let mut variables = HashMap::new();

        for (name, value) in std::env::vars()
        {
            let scoped_value = ScopedValue
                {
                    value: Value::String(value),
                    exported: ValueVisibility::Exported
                };

            variables.insert(format!("${}", name), scoped_value);
        }

        let mut scopes = VecDeque::new();

        scopes.push_back(variables);

        Self
        {
            scopes
        }
    }

    pub fn push_scope(&mut self)
    {
        self.scopes.push_back(HashMap::new());
    }

    pub fn pop_scope(&mut self)
    {
        if self.scopes.len() == 1
        {
            panic!("Cannot pop the global scope!");
        }

        self.scopes.pop_back();
    }

    pub fn create(&mut self, name: String, value: ScopedValue) -> Result<(), String>
    {
        if let Some(scope) = self.scopes.back_mut()
        {
            if scope.contains_key(&name)
            {
                return Err(format!("Variable '{}' already exists in the current scope.", name));
            }

            scope.insert(name, value);
            Ok(())
        }
        else
        {
            Err("No scope available to create the variable.".to_string())
        }
    }

    pub fn get(&self, name: &str) -> Option<&ScopedValue>
    {
        for scope in self.scopes.iter().rev()
        {
            if let Some(value) = scope.get(name)
            {
                return Some(value);
            }
        }

        None
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut ScopedValue>
    {
        for scope in self.scopes.iter_mut().rev()
        {
            if let Some(value) = scope.get_mut(name)
            {
                return Some(value);
            }
        }

        None
    }

    pub fn get_all_flattened(&self) -> HashMap<String, ScopedValue>
    {
        let mut flattened = HashMap::new();

        for scope in self.scopes.iter()
        {
            for (name, value) in scope.iter()
            {
                flattened.insert(name.clone(), value.clone());
            }
        }

        flattened
    }

    pub fn current_scope(&self) -> usize
    {
        self.scopes.len() - 1
    }

    pub fn reset_to_scope(&mut self, scope_index: usize)
    {
        while self.scopes.len() > scope_index + 1
        {
            self.scopes.pop_back();
        }
    }
}
