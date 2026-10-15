
use std::fmt::{self, Display, Formatter};



pub struct Range
{
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub inclusive: bool
}


impl Display for Range
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        if let Some(start) = self.start
        {
            write!(f, "{}", start)?;
        }

        write!(f, "..")?;

        if self.inclusive
        {
            write!(f, "=")?;
        }

        if let Some(end) = self.end
        {
            write!(f, "{}", end)?;
        }

        Ok(())
    }
}
