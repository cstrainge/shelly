
use std::str::Chars;

use crate::language::text::{ buffer::Buffer, location::Location };



/**
 * String iterator based buffer. Start with a loaded string source and allow a user to parse through
 * it character by character while keeping track of the current location.
 */
pub struct SimpleBuffer<'a>
{
    /**
     * The underlying iterator to the characters of the source string.
     */
    text: Chars<'a>,

    /**
     * The current logical location in the source text.
     */
    location: Location,

    /**
     * The width of a tab character in terms of spaces.
     */
    tab_width: usize,

    /**
     * Buffered copy of the last character read from the source iterator. Peeking will populate this
     * value. Getting the next character will drain it if it's present.
     */
    current_char: Option<char>
}


impl<'a> SimpleBuffer<'a>
{
    /**
     * Construct a new buffer from the given source string.
     */
    pub fn new(origin: &str, source: &'a str, tab_width: Option<usize>) -> Self
    {
        Self
            {
                text: source.chars(),
                location: Location::new(origin, 1, 1),
                tab_width: tab_width.unwrap_or(4).max(1),
                current_char: None
            }
    }
}


impl<'a> Buffer for SimpleBuffer<'a>
{
    /**
     * Set the width of a tab character for location tracking purposes.
     */
    fn set_tab_width(&mut self, width: usize)
    {
        self.tab_width = width;
    }

    /**
     * Return the current logical location in the text.
     */
    fn location(&self) -> &Location
    {
        &self.location
    }

    /**
     * Get the mutable reference to the current logical location in the text.
     */
    fn location_mut(&mut self) -> &mut Location
    {
        &mut self.location
    }

    /**
     * Peek at the next character without advancing the buffer.
     */
    fn peek(&mut self) -> Option<char>
    {
        // Check to see if we've already buffered a character. If so, return it directly. Otherwise,
        // fetch the next character from the source iterator.
        match self.current_char
        {
            Some(_) => self.current_char,

            None =>
            {
                let next = self.text.next();

                self.current_char = next;
                next
            }
        }
    }

    /**
     * Advance the buffer and return the next character.
     */
    fn next(&mut self) -> Option<char>
    {
        // Take the buffered character if it exists, otherwise fetch the next character from the
        // source iterator.
        let next = self.current_char.take().or_else(|| self.text.next());

        // If we hadn't hit EOF yet then increment the logical location based on the next character.
        if let Some(next) = next
        {
            self.increment_location(next, self.tab_width);
        }

        next
    }
}
