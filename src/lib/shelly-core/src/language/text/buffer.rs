
use crate::language::text::location::Location;



/**
 * A trait representing a buffer of characters with location tracking. Used by the parser to read
 * input while keeping track of the current logical text location.
 */
pub trait Buffer
{
    /**
     * Set the width of a tab character for location tracking purposes.
     */
    fn set_tab_width(&mut self, width: usize);

    /**
     * A persistent read/decoding error, distinct from normal end of input.
     */
    fn read_error(&self) -> Option<&str> { None }

    /**
     * Read the buffer's current logical location in the text.
     */
    fn location(&self) -> &Location;

    /**
     * Get a mutable reference to the buffer's current logical location in the text.
     */
    fn location_mut(&mut self) -> &mut Location;

    /**
     * Peek at the next character without advancing the buffer. We only support a single character
     * lookahead.
     *
     * Will return None if we've hit EOF.
     */
    fn peek(&mut self) -> Option<char>;

    /**
     * Advance the buffer and return the next character. Will return None if we've hit EOF.
     */
    fn next(&mut self) -> Option<char>;

    /**
     * Logically increment the current text location based on the next character.
     */
    fn increment_location(&mut self, next: char, tab_width: usize)
    {
        let location = &mut self.location_mut();

        // Advance the location based on the encountered character.
        match next
        {
            // Handle tab character by advancing the column to the next tab stop.
            '\t' =>
                {
                    let mut column = location.column;

                    column += tab_width - ((column - 1) % tab_width);
                    location.column = column;
                }

            // Handle newline character by advancing the line and resetting the column.
            '\n' =>
                {
                    location.line += 1;
                    location.column = 1;
                },

            // Handle any other character by just advancing the column.
            _ => location.column += 1
        }
    }
}
