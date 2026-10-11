
include!(concat!(env!("OUT_DIR"), "/bundled_std.rs"));

pub(super) fn source(name: &str) -> Option<&'static str>
{
    MODULES.iter().find_map(|(module, source)| (*module == name).then_some(*source))
}
