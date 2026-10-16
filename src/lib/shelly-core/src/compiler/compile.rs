
use crate::{ language::{ ast::TopLevel, data::module::ScriptModule }, result::CoreResult };



pub fn compile(/**/ _ast: &TopLevel) -> CoreResult<ScriptModule>
{
    Ok(ScriptModule {})
}
