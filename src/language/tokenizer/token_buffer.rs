
use std::collections::VecDeque;

use crate::language::{ parser::source::{ SyntaxContext, SyntaxFact, SyntaxJournal,
                                       SyntaxMetadata, SyntaxRole },
                       text::span::SourceSpan,
                       tokenizer::tokenizer::{ Token, TokenKind, Tokenizer, TokenizerError } };


#[derive(Clone, Copy)]
struct Transaction
{
    position: usize,
    syntax_checkpoint: usize,
}

/**
 * Simple buffer that provides lookahead functionality for a parser. The tokenizer just focuses on
 * producing tokens, while this buffer allows the parser to peek ahead without consuming them
 * immediately.
 *
 * It also features transactional capabilities, allowing the parser to start a transaction,
 * make speculative reads, and either commit or rollback based on the parsing outcome.
 *
 * This allows the parser to try out different parsing strategies without permanently consuming
 * tokens, providing flexibility in handling complex grammar rules. If a rule doesn't work out
 * as expected, the parser can rollback to a previous state and try an alternative approach.
 *
 * Otherwise it can commit the current transaction, making all speculative reads permanent.
 */
pub struct TokenBuffer<'tokenizer, 'input>
{
    /**
     * The underlying tokenizer that produces tokens for this buffer.
     */
    tokenizer: &'tokenizer mut Tokenizer<'input>,

    /**
     * Buffer that holds tokens for lookahead purposes. Tokens are read from the underlying
     * tokenizer as needed for lookahead operations.
     */
    lookahead_buffer: Vec<Token>,

    /**
     * The position of the current cursor within the lookahead buffer. It represents the current
     * toplevel transaction within the lookahead stack.
     */
    position: usize,

    /**
     * Stack of cursors representing nested lookahead transactions. Each entry corresponds to a
     * saved position in the lookahead buffer, allowing the parser to rollback to previous states
     * if needed.
     */
    lookahead_stack: VecDeque<Transaction>,
    syntax: Option<SyntaxJournal>,
    context: SyntaxContext,
}


/**
 * An estimate of the initial lookahead buffer capacity. We're hoping we don't need to resize it
 * frequently during parsing. If we do end up resizing it it isn't the end of the world.
 */
const LOOKAHEAD_BUFFER_CAPACITY: usize = 50;


impl<'tokenizer, 'input> TokenBuffer<'tokenizer, 'input>
{
    /**
     * Initialize a `TokenBuffer` with the given tokenizer. Ready to read from.
     */
    pub fn new(tokenizer: &'tokenizer mut Tokenizer<'input>) -> Self
    {
        Self
            {
                tokenizer,
                lookahead_buffer: Vec::with_capacity(LOOKAHEAD_BUFFER_CAPACITY),
                position: 0,
                lookahead_stack: VecDeque::with_capacity(LOOKAHEAD_BUFFER_CAPACITY),
                syntax: None,
                context: SyntaxContext::Expression,
            }
    }

    pub fn with_syntax(tokenizer: &'tokenizer mut Tokenizer<'input>) -> Self
    {
        let mut buffer = Self::new(tokenizer);
        buffer.syntax = Some(SyntaxJournal::default());
        buffer
    }

    pub fn syntax_metadata(&self) -> Option<SyntaxMetadata>
    { self.syntax.as_ref().map(SyntaxJournal::snapshot) }

    pub(crate) fn syntax_checkpoint(&self) -> usize
    { self.syntax.as_ref().map_or(0, |journal| journal.entries.len()) }

    pub(crate) fn with_context<T>(&mut self, context: SyntaxContext,
                                 parse: impl FnOnce(&mut Self) -> T) -> T
    {
        let previous = self.context;
        self.context = context;
        let result = parse(self);
        self.context = previous;
        result
    }

    pub(crate) fn annotate(&mut self, span: SourceSpan, role: SyntaxRole)
    {
        if let Some(journal) = &mut self.syntax
        { journal.entries.push(SyntaxFact { span, role, context: self.context }); }
    }

    pub(crate) fn annotate_word(&mut self, span: SourceSpan)
    {
        self.annotate(span, self.word_role());
    }

    fn word_role(&self) -> SyntaxRole
    {
        match self.context
        {
            SyntaxContext::Argument => SyntaxRole::Argument,
            SyntaxContext::Command => SyntaxRole::CommandName,
            SyntaxContext::Type => SyntaxRole::TypeName,
            SyntaxContext::Expression => SyntaxRole::Name,
        }
    }

    pub(crate) fn annotate_names_since(&mut self, checkpoint: usize, role: SyntaxRole)
    {
        if let Some(journal) = &mut self.syntax
        {
            let end = journal.entries.len();
            for index in checkpoint..end
            {
                let fact = journal.entries[index];
                if matches!(fact.role, SyntaxRole::Name | SyntaxRole::CommandName
                    | SyntaxRole::Argument | SyntaxRole::TypeName)
                { journal.entries.push(SyntaxFact { role, ..fact }); }
            }
        }
    }

    fn record_token(&mut self, token: &Token)
    {
        if self.syntax.is_none() { return; }
        let role = match token.kind
        {
            TokenKind::Symbol => self.word_role(),
            TokenKind::Identifier | TokenKind::AutoIdentifier => SyntaxRole::Variable,
            TokenKind::Member => SyntaxRole::Member,
            TokenKind::Literal => SyntaxRole::Literal,
            TokenKind::Let | TokenKind::Export | TokenKind::Public | TokenKind::Alias
            | TokenKind::Sub | TokenKind::If | TokenKind::Else | TokenKind::While
            | TokenKind::Until | TokenKind::For | TokenKind::In | TokenKind::Break
            | TokenKind::Continue | TokenKind::Loop | TokenKind::Match | TokenKind::Return
            | TokenKind::Function | TokenKind::Struct | TokenKind::Enum | TokenKind::Type
            | TokenKind::Import => SyntaxRole::Keyword,
            TokenKind::Comma | TokenKind::BlockOpen | TokenKind::BlockClose | TokenKind::EmptyBlock
            | TokenKind::SquareOpen | TokenKind::SquareClose | TokenKind::IndexOpen
            | TokenKind::ParenOpen | TokenKind::ParenClose | TokenKind::StatementBreak
            | TokenKind::LineBreak | TokenKind::LineContinue => SyntaxRole::Delimiter,
            _ => SyntaxRole::Operator,
        };
        self.annotate(token.span, role);
    }

    /**
     * Mark the current position in the lookahead buffer, starting a new lookahead transaction.
     */
    pub fn mark_lookahead(&mut self)
    {
        // Do we currently have a lookahead transaction active? If so, use the front of the stack
        // as the current lookahead position. Otherwise, use the current position.
        let lookahead = if self.is_peeking()
            {
                self.lookahead_stack.front().map(|frame| frame.position)
            }
            else
            {
                Some(self.position)
            };

        if let Some(lookahead) = lookahead
        {
            self.lookahead_stack.push_front(Transaction
                { position: lookahead, syntax_checkpoint: self.syntax_checkpoint() });
        }
    }

    /**
     * Commit the current lookahead transaction, making its changes permanent.
     */
    pub fn commit_lookahead(&mut self)
    {
        if let Some(lookahead) = self.lookahead_stack.pop_front()
        {
            if self.is_peeking()
            {
                self.lookahead_stack[0].position = lookahead.position;
            }
            else
            {
                self.position = lookahead.position;
                self.clear_finished_buffer();
            }
        }
    }

    /**
     * Rollback the current lookahead transaction, reverting any changes made since it was marked.
     */
    pub fn rollback_lookahead(&mut self)
    {
        if let Some(frame) = self.lookahead_stack.pop_front()
        {
            if let Some(journal) = &mut self.syntax
            { journal.entries.truncate(frame.syntax_checkpoint); }
            self.clear_finished_buffer();
        }
    }

    /**
     * Extract the next token from the buffer or tokenizer. if there is a lookahead transaction
     * active then this isn't a permanent change.
     */
    pub fn next(&mut self) -> Result<Option<Token>, TokenizerError>
    {
        let index = self.position();

        if index >= self.lookahead_buffer.len()
         {
            match self.tokenizer.next_token()?
             {
                Some(token) => self.lookahead_buffer.push(token),
                None => return Ok(None)
             }
         }

        let token = self.lookahead_buffer[index].clone();

        if let Some(cursor) = self.lookahead_stack.front_mut()
         {
            cursor.position += 1;
         }
        else
        {
            self.position += 1;
            self.clear_finished_buffer();
        }

        self.record_token(&token);
        Ok(Some(token))
    }

    /**
     * Current cursor within the token cache, including lookahead. Positions can
     * be compared while a lookahead transaction remains active; cache cleanup
     * outside a transaction may reset the cursor.
     */
    pub(crate) fn position(&self) -> usize
    {
        self.lookahead_stack.front().map_or(self.position, |frame| frame.position)
    }

    /**
     * Check if there is an active lookahead transaction. Returns `true` if there is, `false`
     * otherwise.
     */
    fn is_peeking(&self) -> bool
    {
        !self.lookahead_stack.is_empty()
    }

    /**
     * Clear out the finished portion of the lookahead buffer.
     */
    fn clear_finished_buffer(&mut self)
    {
        if    self.position >= self.lookahead_buffer.len()
           && !self.is_peeking()
        {
            self.lookahead_buffer.clear();
            self.position = 0;
        }
    }
}
