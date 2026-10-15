
use crate::{ language::{ text::{ buffer::Buffer, location::Location },
                         tokenizer::token::{ Token,
                                             TokenKind,
                                             TokenValue,
                                             TokenLiteral,
                                             StringFlag } },
             result::{ CoreError, CoreErrorKind, CoreResult} };



/**
 * Internal flag, it indicates whether comments should be skipped during tokenization.
 */
#[derive(PartialEq, Eq, Clone, Copy)]
enum SkipComments
{
    /**
     * Skip comments during tokenization.
     */
    Yes,

    /**
     * Do not skip comments during tokenization.
     */
    No
}


/**
 * Internal flag, it indicates whether newlines should be skipped during tokenization.
 */
#[derive(PartialEq, Eq, Clone, Copy)]
enum SkipNewlines
{
    /**
     * Skip newlines during tokenization.
     */
    Yes,

    /**
     * Do not skip newlines during tokenization.
     */
    No
}


/**
 * The Shelly tokenizer lazily extracts tokens from the input buffer. It only supports sequential
 * extraction with zero lookahead.
 *
 * To enable lookahead and buffering look at `TokenBuffer` that wraps this tokenizer and provides
 * buffering and lookahead capabilities.
 */
pub struct Tokenizer<'a>
{
    /**
     * The input stream from which tokens are extracted. It only supports sequential extraction
     * with a single character of lookahead. Once a character is consumed, it cannot be revisited.
     */
    input: &'a mut dyn Buffer,
}


impl<'a> Tokenizer<'a>
{
    /**
     * Creates a new tokenizer that extracts tokens from the given input buffer.
     */
    pub fn new(input: &'a mut dyn Buffer) -> Self
    {
        Self { input }
    }

    /**
     * Extract the next token from the input buffer. Returns `None` if the end of the input is
     * reached.
     */
    pub fn next(&mut self) -> CoreResult<Option<Token>>
    {
        self.skip_whitespace(SkipComments::Yes, SkipNewlines::No);

        let next = self.input.peek();

        if let None = next
        {
            return Ok(None);
        }

        let next = next.unwrap();

        match next
        {
            '_' | 'a'..='z' | 'A'..='Z' => Ok(Some(self.parse_symbol())),
            '$'                         => Ok(Some(self.parse_identifier())),
            '0'..='9'                   => Ok(Some(self.try_parse_number())),
            '"'                         => self.parse_string(StringFlag::Interpolated),
            '\''                        => self.parse_string(StringFlag::NonInterpolated),
            _                           => self.try_parse_operator()
        }
    }

    /**
     * Extract a text symbol from the input stream. A symbol typically consists of alphabetic
     * characters, digits, and underscores, and is used to represent identifiers, keywords, or
     * other executables in the system where an executable can be an external program or function.
     */
    fn parse_symbol(&mut self) -> Token
    {
        let location = self.input.location().clone();
        let symbol = self.extract_to_separator(None);

        Self::symbol_str_to_token(location, symbol)
    }

    /**
     * Parse a variable identifier from the input stream. A variable identifier starts with a `$`
     * sign followed by alphanumeric characters and underscores.
     */
    fn parse_identifier(&mut self) -> Token
    {
        let location = self.input.location().clone();
        let mut identifier = "$".to_string();

        // Ensure that the next character is a '$' before proceeding with parsing the identifier.
        assert!(self.input.next() == Some('$'),
                "Expected '$' at the beginning of an identifier");

        identifier += &self.extract_to_separator(Some(&['.']));

        if identifier.len() == 1
        {
            return Token
                {
                    location,
                    kind: TokenKind::AutoIdentifier,
                    value: TokenValue::None
                };
        }

        // Now that we know for sure the identifier started with a '$' we can keep going until we
        // hit a whitespace, separator, or dot character. We break on a dot only in the context of
        // parsing the identifier to allow for dotted notation (e.g., `$variable.property`).
        while let Some(next) = self.input.peek()
        {
            if    !Self::is_whitespace_char(&next)
               && !Self::is_separator_char(&next)
               && !(next == '.')
            {
                let _ = &self.input.next();
                identifier.push(next);
            }
            else
            {
                break;
            }
        }

        Token
            {
                location,
                kind: TokenKind::Identifier,
                value: TokenValue::Identifier(identifier)
            }
    }

    /**
     * Attempt to parse a literal number from the input stream. It will handle integers and
     * floating-point numbers, and other numeric formats as defined by the language's syntax.
     */
    fn try_parse_number(&mut self) -> Token
    {
        let location = self.input.location().clone();
        let number_str = self.extract_to_separator(None);

        // Attempt to parse the collected string as an integer first. If that fails, try parsing it
        // as a float. If both fail, treat it as a symbol.
        if let Ok(number) = number_str.parse::<i64>()
        {
            Token
                {
                    location,
                    kind: TokenKind::Literal,
                    value: TokenValue::Literal(TokenLiteral::Integer(number))
                }
        }
        else if let Ok(number) = number_str.parse::<f64>()
        {
            Token
                {
                    location,
                    kind: TokenKind::Literal,
                    value: TokenValue::Literal(TokenLiteral::Float(number, number_str))
                }
        }
        else
        {
            Self::symbol_str_to_token(location, number_str)
        }
    }

    /**
     * Parse a string literal from the input stream. It will attempt to extract the string as a
     * single line string. If it is a multi-line string, it will call out for multi-line handling.
     *
     * In single line strings `\n`, new lines are illegal and will result in a parsing error. An
     * error is also generated if the closing quote is never encountered in the source text.
     */
    fn parse_string(&mut self, flag: StringFlag) -> CoreResult<Option<Token>>
    {
        let location = self.input.location().clone();
        let next = self.input.next().unwrap();

        assert!(matches!(next, '"' | '\''),
                "Internal error, expected a quote to start a string literal.");

        if    let Some(next) = self.input.peek()
           && next == '*'
        {
            return self.parse_multiline_string(location, flag);
        }
        else
        {
            let mut literal_string = String::new();
            let mut closed = false;

            while let Some(next) = self.input.next()
            {
                match next
                {
                    '"' if flag == StringFlag::Interpolated =>
                        {
                            closed = true;
                            break;
                        },

                    '\'' if flag == StringFlag::NonInterpolated =>
                        {
                            closed = true;
                            break;
                        },

                    '\n' =>
                        {
                            let message = "Unexpected newline encountered in single-line string \
                                          literal.";

                            return Err(CoreError
                                {
                                    location: Some(location),
                                    kind: CoreErrorKind::LexicalError,
                                    message: Some(message.to_string())
                                });
                        },

                    '\\' => literal_string.push(self.process_string_escape()?),

                    _    => literal_string.push(next)
                }
            }

            if !closed
            {
                return Err(CoreError
                    {
                        location: Some(location),
                        kind: CoreErrorKind::LexicalError,
                        message: Some("Missing ending quote for string literal.".to_string())
                    })
            }

            Ok(Some(Token
                {
                    location,
                    kind: TokenKind::Literal,
                    value: TokenValue::Literal(TokenLiteral::String(literal_string, flag))
                }))
        }
    }

    /**
     * Called from parse string when a multi-line string is detected. The string parsed will have
     * leading whitespace removed while preserving the relative indentation of the subsequent lines.
     */
    fn parse_multiline_string(&mut self,
                              location: Location,
                              flag: StringFlag) -> CoreResult<Option<Token>>
    {
        // Helper for skipping extra whitespace at the beginning of each line.  If there is no text
        // on a given line it is skipped entirely.
        fn skip_whitespace_until_column<'a>(location: &Location, buffer: &mut dyn Buffer,
                                            target_column: usize)
                                            -> CoreResult<()>
        {
            while   let Some(next) = buffer.peek()
                 && Tokenizer::<'a>::is_whitespace_char(&next)
                 && buffer.location().column < target_column
            {
                let _ = buffer.next();
            }

            if buffer.peek().is_none()
            {
                return Err(CoreError
                    {
                        location: Some(location.clone()),
                        kind: CoreErrorKind::LexicalError,
                        message: Some("Unexpected end of file in string literal.".to_string())
                    });
            }

            Ok(())
        }

        // Append newlines for skipped empty lines.
        fn append_newlines(text: &mut String, count: usize)
        {
            for _ in 0..count
            {
                text.push('\n');
            }
        }

        // We expect that the " has already be processed and that we need to consume the following *.
        let next = self.input.next().unwrap();
        assert!(next == '*');

        // Skip over any whitespace at the beginning of the string.  Using the location of the first
        // textual character to calibrate what we will consider the beginning of the actual line of
        // text.  This way we can remove any extra whitespace at the beginning of each line while
        // allowing for any extra indentation the user may want to add.
        self.skip_whitespace(SkipComments::No, SkipNewlines::Yes);

        let target_column = self.input.location().column;

        let mut closed = false;
        let mut text = String::new();

        // Keep going until we either hit the end of the buffer or the closing *" pair.
        while let Some(next) = self.input.next()
        {
            match next
            {
                // We found the * but did we find the "?
                '*' =>
                {
                    if let Some(quote) = self.input.peek()
                    {
                        // We're at the end of the string.
                        if   (quote == '"' && flag == StringFlag::Interpolated)
                          || (quote == '\'' && flag == StringFlag::NonInterpolated)
                        {
                            let _ = self.input.next();
                            closed = true;
                            break;
                        }
                        else
                        {
                            // Looks like a stray * so we'll just add it to the text.
                            text.push('*');
                        }
                    }
                    else
                    {
                        let message = "Unexpected end of file in string literal.";

                        return Err(CoreError
                            {
                                location: Some(location.clone()),
                                kind: CoreErrorKind::LexicalError,
                                message: Some(message.to_string())
                            });
                    }
                }

                // Process the escape sequence.
                '\\' => text.push(self.process_string_escape()?),

                // Process the new line skipping any extra whitespace until we hit the target column.
                '\n' =>
                {
                    text.push('\n');

                    // Keep track of the starting line so that we can add newlines for skipped empty
                    // lines.  Then start skipping until we find something useful or we hit the
                    // target column.
                    let start_line = self.input.location().line;

                    skip_whitespace_until_column(&location, self.input, target_column)?;

                    // If we skipped any empty lines then we need to backfill the newlines.
                    let current_line = self.input.location().line;

                    if current_line > start_line
                    {
                        append_newlines(&mut text, current_line - start_line);
                    }
                }

                // Just add the character to the text.
                _ =>
                {
                    text.push(next);
                }
            }
        }

        if !closed
        {
            return Err(CoreError
                {
                    location: Some(location.clone()),
                    kind: CoreErrorKind::LexicalError,
                    message: Some("Unterminated string literal.".to_string())
                });
        }

        Ok(Some(Token
            {
                location,
                kind: TokenKind::Literal,
                value: TokenValue::Literal(TokenLiteral::String(text, flag))
            }))
    }

    /**
     * Processes an escape sequence within a string literal and returns the corresponding character.
     *
     * The sequence can be a single known character like \n or \\ or a numeric escape sequence like
     * \xNN or \o{NNNN}.
     */
    fn process_string_escape(&mut self) -> CoreResult<char>
    {
        // Attempt to parse the numeric value from the escape sequence. If not parsed correctly, an
        // error will be returned.
        fn parse_number(location: &Location, input: &mut dyn Buffer,
                        is_digit: fn(char) -> bool, radix: u32)
                        -> CoreResult<char>
        {
            let number_str = read_digits(input, is_digit);
            let number = u32::from_str_radix(&number_str, radix)
                .map_err(|_| CoreError
                    {
                        location: Some(location.clone()),
                        kind: CoreErrorKind::LexicalError,
                        message: Some(format!("Failed to parse numeric literal from '{}'.",
                                              number_str))
                    })?;

            char::from_u32(number)
                .ok_or_else(|| CoreError
                    {
                        location: Some(location.clone()),
                        kind: CoreErrorKind::LexicalError,
                        message: Some(format!("Numeric literal '{}' is not a valid character.",
                                              number_str))
                    })
        }

        // Read a sequence of digits from the input buffer that satisfy the `is_digit` predicate
        // breaking when a non-digit character is encountered.
        fn read_digits(input: &mut dyn Buffer, is_digit: fn(char) -> bool) -> String
        {
            let mut number_str = String::new();

            while    let Some(next) = input.peek()
                  && is_digit(next)
            {
                number_str.push(input.next().unwrap());
            }

            number_str
        }

        let location = self.input.location().clone();

        match self.input.next()
        {
            // Perform a simple translation of the escape sequence.
            Some('n') => Ok('\n'),
            Some('r') => Ok('\r'),
            Some('t') => Ok('\t'),

            // Parse a decimal numeric literal for the character.
            Some('0') =>
                {
                    parse_number(&location, self.input, |next| next.is_ascii_digit(), 10)
                }

            // Parse a hexadecimal numeric literal for the character.
            Some('x') | Some('X') =>
                {
                    parse_number(&location, self.input, |next| next.is_ascii_hexdigit(), 16)
                }

            // Parse an octal numeric literal for the character.
            Some('o') | Some('O') =>
                {
                    parse_number(&location, self.input, |next| ('0'..='7').contains(&next), 8)
                }

            // The escape was on a non-special character so just pass it through without translation.
            Some(next) => Ok(next),

            // Looks like we hit the end of the buffer while processing a string.
            None =>
                {
                    let message = "Unexpected end of file in string literal.";

                    Err(CoreError
                        {
                            location: Some(location),
                            kind: CoreErrorKind::LexicalError,
                            message: Some(message.to_string())
                        })
                }
        }
    }

    /**
     * Attempt to extract a single or multiple character operator from the input stream. Returns
     * `Ok(Some(Token))` if an operator is successfully parsed, `Ok(None)` if no operator is found,
     * or `Err(TokenizerError)` if an error is encountered during parsing.
     */
    fn try_parse_operator(&mut self) -> CoreResult<Option<Token>>
    {
        fn operator_token(location: Location, kind: TokenKind) -> CoreResult<Option<Token>>
        {
            Ok(Some(Token{ location, kind, value: TokenValue::None }))
        }

        // First, check to see if the next character is a separator operator that can be immediately
        // returned. If it isn't a separator operator, we will need to parse it as a multi-character
        // operator. This disambiguates "-" from "-f" or "--foo" where the latter two can be treated
        // as text to be passed through.
        let location = self.input.location().clone();
        let next = self.input.next().unwrap();

        match next
        {
            '|'  => return operator_token(location, TokenKind::Pipe),
            ';'  => return operator_token(location, TokenKind::StatementBreak),
            '('  => return operator_token(location, TokenKind::ParenOpen),
            ')'  => return operator_token(location, TokenKind::ParenClose),
            '['  => return operator_token(location, TokenKind::SquareOpen),
            ']'  => return operator_token(location, TokenKind::SquareClose),
            '`'  => return operator_token(location, TokenKind::ExecEscape),
            '\n' => return operator_token(location, TokenKind::LineBreak),
            '\\' => return operator_token(location, TokenKind::LineContinue),
            ','  => return operator_token(location, TokenKind::Comma),
            ':'  =>
                {
                    if let Some(':') = self.input.peek()
                    {
                        let _ = self.input.next();
                        return operator_token(location, TokenKind::Scope);
                    }

                    return operator_token(location, TokenKind::TypeDelimiter);
                }

            // Nothing to do here, pass through to non-separator operator parsing.
            _ => {}
        }

        // Ok we know it isn't a separator operator, so we will attempt to parse it as a
        // non-separator operator, some of which can be multi-character.
        let mut operator_str = String::new();

        operator_str.push(next);
        operator_str += &self.extract_to_separator(None);

        match operator_str.as_str()
        {
            "="   => return operator_token(location, TokenKind::Assign),
            "{"   => return operator_token(location, TokenKind::BlockOpen),
            "}"   => return operator_token(location, TokenKind::BlockClose),
            "-"   => return operator_token(location, TokenKind::Minus),
            "~"   => return operator_token(location, TokenKind::ErrorSource),
            "+"   => return operator_token(location, TokenKind::Plus),
            "*"   => return operator_token(location, TokenKind::Asterisk),
            "/"   => return operator_token(location, TokenKind::Slash),
            "%"   => return operator_token(location, TokenKind::Percent),
            "->"  => return operator_token(location, TokenKind::RedirectTo),
            "<-"  => return operator_token(location, TokenKind::RedirectFrom),
            "**"  => return operator_token(location, TokenKind::Glob),
            "..." => return operator_token(location, TokenKind::Splat),
            _     => Ok(Some(Self::symbol_str_to_token(location, operator_str)))
        }
    }

    /**
     * Internal method to convert the symbol string into a corresponding token. This is used to
     * differentiate between keywords and general symbols.
     */
    fn symbol_str_to_token(location: Location, symbol: String) -> Token
    {
        let kind = match symbol.as_str()
            {
                "let"    => TokenKind::Let,
                "export" => TokenKind::Export,
                "sub"    => TokenKind::Sub,
                "if"     => TokenKind::If,
                "else"   => TokenKind::Else,
                "while"  => TokenKind::While,
                "loop"   => TokenKind::Loop,
                "match"  => TokenKind::Match,
                "return" => TokenKind::Return,
                "fn"     => TokenKind::Fn,
                "struct" => TokenKind::Struct,
                "enum"   => TokenKind::Enum,
                "import" => TokenKind::Import,
                "true"   => TokenKind::Literal,
                "false"  => TokenKind::Literal,
                _        => TokenKind::Symbol
            };

        let value = match &kind
            {
                TokenKind::Symbol  => TokenValue::Symbol(symbol),
                TokenKind::Literal => TokenValue::Literal(TokenLiteral::Boolean(symbol == "true")),
                _                  => TokenValue::None
            };

         Token { location, kind, value }
    }

    /**
     * Extract characters from the input stream until a separator or an optional additional
     * separator is encountered.
     */
    fn extract_to_separator(&mut self, additional_separators: Option<&[char]>) -> String
    {
        let mut result = String::new();

        while let Some(next) = self.input.peek()
        {
            if    !Self::is_separator_char(&next)
               && !additional_separators.map_or(false, |separators| separators.contains(&next))
            {
                let _ = &self.input.next();
                result.push(next);
            }
            else
            {
                break;
            }
        }

        result
    }

    /**
     * Is the given character a separator character? Separator characters are those that are used as
     * operators or punctuation in the language's syntax. They also act as separator characters
     * between tokens. So `$var|$var` appear as separate tokens in the stream despite the lack of
     * whitespace.
     */
    fn is_separator_char(next: &char) -> bool
    {
           *next == '|'
        || *next == ';'
        || *next == '('
        || *next == ')'
        || *next == '['
        || *next == ']'
        || *next == ':'
        || *next == '#'
        || *next == '`'
        || *next == ','
        || *next == '\n'
        || *next == '\\'
        || Self::is_whitespace_char(next)
    }

    /**
     * Skip all text that is considered whitespace, including comments, until a non-whitespace
     * character is encountered. In this syntax new lines are not specifically considered
     * whitespace. Because newlines are significant in terminating certain statements like
     * executable object calls.
     */
    fn skip_whitespace(&mut self, skip_comments: SkipComments, skip_newlines: SkipNewlines)
    {
        // Peek at the next character and skip it if it's whitespace or part of a comment.
        while let Some(next) = self.input.peek()
        {
            // Stop unless the character is whitespace, a comment marker with comment skipping
            // enabled, or a newline with newline skipping enabled.
            if    !Self::is_whitespace_char(&next)
               && !(next == '#' && skip_comments == SkipComments::Yes)
               && !(next == '\n' && skip_newlines == SkipNewlines::Yes)
            {
                break;
            }

            // It's a whitespace character or part of a comment, so consume it.
            let _ = self.input.next();

            // If the next character is a comment indicator and we are supposed to skip comments,
            // skip the entire comment up until the next newline character. We don't consume the
            // newline itself so that it can be handled separately as a significant token.
            if next == '#' && skip_comments == SkipComments::Yes
            {
                // Consume any characters in the comment until a newline is encountered.
                while let Some(next) = self.input.peek()
                {
                    if next == '\n'
                    {
                        break;
                    }

                    let _ = self.input.next();
                }
            }
        }
    }

    /**
     * Determines if the given character is considered whitespace, excluding newline characters.
     * This is because newlines are considered significant in the language's syntax.
     */
    fn is_whitespace_char(next: &char) -> bool
    {
        *next != '\n' && next.is_whitespace()
    }
}
