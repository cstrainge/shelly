
/**
 * The color mode of the terminal we're running in.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtyColorMode
{
    /**
     * The terminal is a TTY but only supports monochrome output.
     */
    TtyMonochrome,

    /**
     * The terminal supports the basic set of ANSI colors.
     */
    TtyBasic,

    /**
     * The terminal is a TTY and supports 256 colors.
     */
    Tty256,

    /**
     * The terminal is a TTY and supports true color (24-bit).
     */
    TtyTrueColor
}


impl TtyColorMode
{
    // The native Shelly enum uses the same names and ordering as the runtime mode.
    pub const VARIANT_NAMES: [&'static str; 4] = [
            "TtyMonochrome", "TtyBasic", "Tty256", "TtyTrueColor",
        ];

    pub fn variant_index(self) -> usize
    {
        match self
        {
            Self::TtyMonochrome => 0,
            Self::TtyBasic => 1,
            Self::Tty256 => 2,
            Self::TtyTrueColor => 3,
        }
    }
}
