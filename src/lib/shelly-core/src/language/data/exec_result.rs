
use std::fmt::{self, Display, Formatter};



pub struct ExecResult
{
    pub signaled: bool,
    pub code: u8
}


impl Display for ExecResult
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        if self.signaled
        {
            write!(f, "Signaled")
        }
        else
        {
            write!(f, "{}", self.code)
        }
    }
}
