
use crate::{ compiler::abi::Abi,
             language::{ ast::TopLevel, /*data::module::ScriptModule*/ },
             result::CoreResult };



pub fn compile(_abi: &Abi, _ast: &TopLevel) -> CoreResult<()>
{
    //Ok(ScriptModule {})

    Ok(())
}
