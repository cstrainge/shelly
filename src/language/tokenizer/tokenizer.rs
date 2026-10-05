
use std::fmt::{ self, Display, Formatter };

use crate::language::text::{ buffer::Buffer, location::Location };



/**
 * Represents all the different types of tokens that can be recognized in the Shelly script
 * language.
 */
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TokenKind
{
    /**
     * A simple literal value, such as a number or string.
     */
    Literal,

    /**
     * A variable identifier and always starts with a $ character.
     */
    Identifier,

    /**
     * A symbol text representing blocks of text as found during parsing. Can be a name of an
     * executable or a command line parameter being passed to something. These can end up resolving
     * to simple strings.
     */
    Symbol,

    /**
     * The auto variable that is automatically supplied by the language in certain contexts.
     */
    AutoIdentifier,

    /**
     * The `let` keyword used for variable declaration.
     */
    Let,

    /**
     * The `export` keyword used for marking variables as exported.
     */
    Export,

    /**
     * The `,` character used to separate items in a list or function parameters.
     */
    Comma,

    /**
     * The `sub` keyword used for defining a sub-process block.
     */
    Sub,

    /**
     * The `if` keyword used for conditional branching.
     */
    If,

    /**
     * The `else` keyword used for conditional branching when the `if` condition is not met.
     */
    Else,

    /**
     * The `while` keyword used for looping while a condition is true.
     */
    While,

    /**
     * A unbound loop construct used for indefinite looping until explicitly broken.
     */
    Loop,

    /**
     * The `match` keyword used for pattern matching against values.
     */
    Match,

    /**
     * The `return` keyword used for returning a value from a function.
     */
    Return,

    /**
     * Keyword used for defining a function.
     */
    Function,

    /**
     * The `struct` keyword used for defining a new structure.
     */
    Struct,

    /**
     * The `enum` keyword used for defining a new enumeration.
     */
    Enum,

    /**
     * Keyword used for importing modules or other resources into the current scope.
     */
    Import,

    /**
     * The `:` character used for type annotations or other type-related syntax.
     */
    TypeDelimiter,

    /**
     * The `=` character used for assignment of values to variables.
     */
    Assign,

    /**
     * The `-` character used for subtraction or negation.
     */
    Minus,

    /**
     * The `+` character used for addition.
     */
    Plus,

    /**
     * The `*` character used for multiplication or file gathering operations.
     */
    Asterisk,

    /**
     * The `**` character used for globbing or file pattern matching.
     */
    Glob,

    /**
     * The `/` character used for division or file path operations.
     */
    Slash,

    /**
     * The `%` character used for modulo operations or other percentage-related syntax.
     */
    Percent,

    /**
     * The `;` character used to indicate the end of a statement, without requiring a newline.
     */
    StatementBreak,

    /**
     * The `\n` character used to indicate a line break in the source code. If the file includes any
     * `\r` characters they are ignored.
     */
    LineBreak,

    /**
     * The `\` character used to indicate that the current line continues onto the next line. It is
     * illegal for anything other than whitespace and comments to follow it.
     */
    LineContinue,

    /**
     * The scope symbol, `::` used to indicate the a scope delimiter for a keyword or to indicate
     * the executable should be taken from an outer scope.
     */
    Scope,

    /**
     * The `\`` character used to indicate that we're taking the value of an executable instead of
     * actually executing it.
     *
     * Used for assigning functions and executable references to variables for later use.
     */
    ExecEscape,

    /**
     * Open a block of code with `{`.
     */
    BlockOpen,

    /**
     * Close a block of code with `}`.
     */
    BlockClose,

    /**
     * Represents an empty block of code, `{}`.
     */
    EmptyBlock,

    /**
     * Open a square bracket with `[`. Used for array indexing or defining array literals.
     */
    SquareOpen,

    /**
     * Close a square bracket with `]`. Used for array indexing or defining array literals.
     */
    SquareClose,

    /**
     * Open a parenthesis with `(`. Used for grouping expressions, defining function parameters, and
     * grouping expressions/executable calls.
     */
    ParenOpen,

    /**
     * Close a parenthesis closing a group of expressions or an executable call.
     */
    ParenClose,

    /**
     * Symbol for piping IO between commands, represented by the `|` character.
     */
    Pipe,

    /**
     * Represents stderr in the source code, represented by the `~` character.
     */
    ErrorSource,

    /**
     * Represents redirection in the source code to a file or variable, the `->` symbol.
     */
    RedirectTo,

    /**
     * Represents redirection in the source code from a file or variable, the `<-` symbol.
     */
    RedirectFrom,

    /**
     * Represents the splat operator, `...`, used for argument expansion.
     */
    Splat
}


impl Display for TokenKind
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "{:?}", self)
    }
}


/**
 * Indicate if the string should participate in interpolation or not.
 */
#[derive(Clone, PartialEq, Eq)]
pub enum StringFlag
{
    Interpolated,
    NonInterpolated
}


/**
 * Represents the different types of literals that can be associated with a token.
 */
#[derive(Clone)]
pub enum TokenLiteral
{
    /**
     * An integer literal value.
     */
    Integer(i64, String),

    /**
     * A floating-point literal value.
     */
    Float(f64, String),

    /**
     * A boolean literal value.
     */
    Boolean(bool),

    /**
     * A string literal value.
     */
    String(String, StringFlag)
}


impl Display for TokenLiteral
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            TokenLiteral::Integer(value, text) => write!(f, "{}:{}", value, text),
            TokenLiteral::Float(value, text)   => write!(f, "{}:{}", value, text),
            TokenLiteral::Boolean(value)       => write!(f, "{}", value),
            TokenLiteral::String(text, _)      => write!(f, "{:?}", text)
        }
    }
}


/**
 * Represents a value that can be associated with a token, such as a literal, identifier, or none.
 */
#[derive(Clone)]
pub enum TokenValue
{
    None,
    Literal(TokenLiteral),
    Identifier(String),
    Symbol(String)
}


impl Display for TokenValue
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self
        {
            TokenValue::None                   => write!(f, "_"),
            TokenValue::Literal(literal)       => write!(f, "{}", literal),
            TokenValue::Identifier(identifier) => write!(f, "{}", identifier),
            TokenValue::Symbol(symbol)         => write!(f, "{}", symbol)
        }
    }
}


/**
 * Represents a single token of language as extracted from the input source code.
 */
#[derive(Clone)]
pub struct Token
{
    /**
     * The logical location in the source code this token was extracted from.
     */
    pub location: Location,

    /**
     * The type of the token, indicating its role in the language syntax.
     */
    pub kind: TokenKind,

    /**
     * The value of the token, which may be a literal, identifier, or none depending on the token
     * type.
     */
    pub value: TokenValue
}


impl Token
{
    /**
     * Returns the textual representation of the token's original value.
     */
    pub fn token_value_text(&self) -> String
    {
        match &self.kind
        {
            TokenKind::Literal =>
                {
                    if let TokenValue::Literal(literal) = &self.value
                    {
                        match literal
                        {
                            TokenLiteral::Integer(_, text) => return text.clone(),
                            TokenLiteral::Float(_, text)   => return text.clone(),
                            TokenLiteral::Boolean(value)   => return value.to_string(),
                            TokenLiteral::String(text, _)  => return text.clone()
                        }
                    }
                    else
                    {
                        panic!("A literal token does not have a literal value.");
                    }
                },

            TokenKind::Identifier =>
                {
                    if let TokenValue::Identifier(text) = &self.value
                    {
                        text.clone()
                    }
                    else
                    {
                        panic!("An identifier token does not have an identifier value.");
                    }
                },

            TokenKind::Symbol =>
                {
                    if let TokenValue::Symbol(text) = &self.value
                    {
                        text.clone()
                    }
                    else
                    {
                        panic!("A symbol token does not have a symbol value.");
                    }
                },

            TokenKind::AutoIdentifier => "$".to_string(),
            TokenKind::Let            => "let".to_string(),
            TokenKind::Export         => "export".to_string(),
            TokenKind::Comma          => ",".to_string(),
            TokenKind::Sub            => "sub".to_string(),
            TokenKind::If             => "if".to_string(),
            TokenKind::Else           => "else".to_string(),
            TokenKind::While          => "while".to_string(),
            TokenKind::Loop           => "loop".to_string(),
            TokenKind::Match          => "match".to_string(),
            TokenKind::Return         => "return".to_string(),
            TokenKind::Function       => "fn".to_string(),
            TokenKind::Struct         => "struct".to_string(),
            TokenKind::Enum           => "enum".to_string(),
            TokenKind::Import         => "import".to_string(),
            TokenKind::TypeDelimiter  => ":".to_string(),
            TokenKind::Assign         => "=".to_string(),
            TokenKind::Minus          => "-".to_string(),
            TokenKind::Plus           => "+".to_string(),
            TokenKind::Asterisk       => "*".to_string(),
            TokenKind::Glob           => "**".to_string(),
            TokenKind::Slash          => "/".to_string(),
            TokenKind::Percent        => "%".to_string(),
            TokenKind::StatementBreak => ";".to_string(),
            TokenKind::LineBreak      => "\n".to_string(),
            TokenKind::LineContinue   => "\\".to_string(),
            TokenKind::Scope          => "::".to_string(),
            TokenKind::ExecEscape     => "`".to_string(),
            TokenKind::BlockOpen      => "{".to_string(),
            TokenKind::BlockClose     => "}".to_string(),
            TokenKind::EmptyBlock     => "{}".to_string(),
            TokenKind::SquareOpen     => "[".to_string(),
            TokenKind::SquareClose    => "]".to_string(),
            TokenKind::ParenOpen      => "(".to_string(),
            TokenKind::ParenClose     => ")".to_string(),
            TokenKind::Pipe           => "|".to_string(),
            TokenKind::ErrorSource    => "~".to_string(),
            TokenKind::RedirectTo     => "->".to_string(),
            TokenKind::RedirectFrom   => "<-".to_string(),
            TokenKind::Splat          => "...".to_string()
        }
    }
}


impl Display for Token
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "{}: ({}, {})", self.location, self.kind, self.value)
    }
}


/**
 * Represents an error encountered during tokenization.
 */
pub struct TokenizerError
{
    /**
     * The logical location in the source code the error occurred at.
     */
    pub location: Location,

    /**
     * A descriptive message explaining the error.
     */
    pub message: String
}


impl Display for TokenizerError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f, "TokenizerError at {}: {}", self.location, self.message)
    }
}


/**
 * Indicates whether comments should be skipped during tokenization.
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
 * A Shelly script tokenizer that lazily extracts tokens from the input source code buffer as
 * needed.
 */
pub struct Tokenizer<'a>
{
    /**
     * The source code input buffer to read our tokens from.
     */
    input: &'a mut dyn Buffer
}


impl<'a> Tokenizer<'a>
{
    /**
     * Construct a new tokenizer with the given input buffer.
     */
    pub fn new(input: &'a mut dyn Buffer) -> Self
    {
        Self { input }
    }

    /**
     * Attempt to extract the next token from the input stream. Returns `Ok(Some(Token))` if a token
     * is successfully parsed, `Ok(None)` if the end of the input stream is reached, or
     * `Err(TokenizerError)` if an error is encountered during tokenization.
     */
    pub fn next_token(&mut self) -> Result<Option<Token>, TokenizerError>
    {
        // Skip past any whitespace and comments.
        self.skip_whitespace(SkipComments::Yes, SkipNewlines::No);

        // Check to see what the next character is, if any.
        let next = self.input.peek_next();

        if let None = next
        {
            return Ok(None);
        }

        // Unwrap the next character since we know it exists then proceed with matching it. Attempt
        // to return the extracted token to the caller, if any.
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
                "fn"     => TokenKind::Function,
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

        identifier += &self.extract_to_separator(Some(&['.', '/']));

        // A variable-prefixed path is one interpolated word, not a variable name.
        if self.input.peek_next() == Some('/')
        {
            identifier += &self.extract_to_separator(None);
            return Self::symbol_str_to_token(location, identifier);
        }

        if identifier.len() == 1
        {
            return Token
                {
                    location,
                    kind: TokenKind::AutoIdentifier,
                    value: TokenValue::None
                };
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
                    value: TokenValue::Literal(TokenLiteral::Integer(number, number_str))
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
     * Extract characters from the input stream until a separator or an optional additional
     * separator is encountered.
     */
    fn extract_to_separator(&mut self, additional_separators: Option<&[char]>) -> String
    {
        let mut result = String::new();

        while let Some(next) = self.input.peek_next()
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
     * Parse a string literal from the input stream. It will attempt to extract the string as a
     * single line string. If it is a multi-line string, it will call out for multi-line handling.
     *
     * In single line strings `\n`, new lines are illegal and will result in a parsing error. An
     * error is also generated if the closing quote is never encountered in the source text.
     */
    fn parse_string(&mut self, flag: StringFlag) -> Result<Option<Token>, TokenizerError>
    {
        let location = self.input.location().clone();
        let next = self.input.next().unwrap();

        assert!(matches!(next, '"' | '\''),
                "Internal error, expected a quote to start a string literal.");

        if    let Some(next) = self.input.peek_next()
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
                            return Err(TokenizerError
                                {
                                    location,
                                    message: "Newline encountered in single-line string literal."
                                        .to_string()
                                });
                        },

                    '\\' => literal_string.push(self.process_string_escape()?),

                    _    => literal_string.push(next)
                }
            }

            if !closed
            {
                return Err(TokenizerError
                    {
                        location,
                        message: "Missing ending quote for string literal.".to_string()
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
                              flag: StringFlag) -> Result<Option<Token>, TokenizerError>
    {
        // Helper for skipping extra whitespace at the beginning of each line.  If there is no text
        // on a given line it is skipped entirely.
        fn skip_whitespace_until_column<'a>(location: &Location, buffer: &mut dyn Buffer,
                                            target_column: usize)
                                            -> Result<(), TokenizerError>
        {
            while   let Some(next) = buffer.peek_next()
                 && Tokenizer::<'a>::is_whitespace_char(&next)
                 && buffer.location().column < target_column
            {
                let _ = buffer.next();
            }

            if buffer.peek_next().is_none()
            {
                return Err(TokenizerError
                    {
                        location: location.clone(),
                        message: "Unexpected end of file in string literal.".to_string()
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
                    if let Some(quote) = self.input.peek_next()
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
                        return Err(TokenizerError
                            {
                                location: location.clone(),
                                message: "Unexpected end of file in string literal.".to_string()
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
            return Err(TokenizerError
                {
                    location: location.clone(),
                    message: "Unterminated string literal.".to_string()
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
    fn process_string_escape(&mut self) -> Result<char, TokenizerError>
    {
        // Attempt to parse the numeric value from the escape sequence. If not parsed correctly, an
        // error will be returned.
        fn parse_number(location: &Location, input: &mut dyn Buffer,
                        is_digit: fn(char) -> bool, radix: u32)
                        -> Result<char, TokenizerError>
        {
            let number_str = read_digits(input, is_digit);
            let number = u32::from_str_radix(&number_str, radix)
                .map_err(|_| TokenizerError
                    {
                        location: location.clone(),
                        message: format!("Failed to parse numeric literal from '{}'.",
                                         number_str)
                    })?;

            char::from_u32(number)
                .ok_or_else(|| TokenizerError
                    {
                        location: location.clone(),
                        message: format!("Numeric literal '{}' is not a valid character.",
                                         number_str)
                    })
        }

        // Read a sequence of digits from the input buffer that satisfy the `is_digit` predicate
        // breaking when a non-digit character is encountered.
        fn read_digits(input: &mut dyn Buffer, is_digit: fn(char) -> bool) -> String
        {
            let mut number_str = String::new();

            while    let Some(next) = input.peek_next()
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
                    Err(TokenizerError
                        {
                            location,
                            message: "Unexpected end of file in string literal.".to_string()
                        })
                }
        }
    }

    /**
     * Attempt to extract a single or multiple character operator from the input stream. Returns
     * `Ok(Some(Token))` if an operator is successfully parsed, `Ok(None)` if no operator is found,
     * or `Err(TokenizerError)` if an error is encountered during parsing.
     */
    fn try_parse_operator(&mut self) -> Result<Option<Token>, TokenizerError>
    {
        fn operator_token(location: Location, kind: TokenKind) -> Result<Option<Token>,
                                                                       TokenizerError>
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
                    if let Some(':') = self.input.peek_next()
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
            "{}"  => return operator_token(location, TokenKind::EmptyBlock),
            "**"  => return operator_token(location, TokenKind::Glob),
            "..." => return operator_token(location, TokenKind::Splat),
            _     => Ok(Some(Self::symbol_str_to_token(location, operator_str)))
        }
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
        while let Some(next) = self.input.peek_next()
        {
            // Check to see if the next character is not whitespace. If it isn't, break out of the
            // loop, without consuming the non-whitespace character.
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
                while let Some(next) = self.input.peek_next()
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
     * Determines if the given character is considered whitespace, excluding newline characters.
     * This is because newlines are considered significant in the language's syntax.
     */
    fn is_whitespace_char(next: &char) -> bool
    {
        *next != '\n' && next.is_whitespace()
    }
}
