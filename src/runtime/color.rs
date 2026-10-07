
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
