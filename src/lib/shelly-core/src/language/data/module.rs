
use cranelift_jit::JITModule;



pub type ScriptTopLevel = extern "C" fn();


pub struct ScriptModule
{
    pub jit_module: Option<JITModule>,
    pub top_level: ScriptTopLevel
}


impl ScriptModule
{
    pub fn new(jit_module: JITModule, top_level: ScriptTopLevel) -> Self
    {
        Self
            {
                jit_module: Some(jit_module),
                top_level
            }
    }
}


impl Drop for ScriptModule
{
    fn drop(&mut self)
    {
        if let Some(module) = self.jit_module.take()
        {
            unsafe
            {
                module.free_memory();
            }
        }
    }
}
