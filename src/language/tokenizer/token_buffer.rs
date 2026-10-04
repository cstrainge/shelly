
use std::collections::VecDeque;

use crate::language::tokenizer::tokenizer::{ Token, Tokenizer, TokenizerError };



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
    lookahead_stack: VecDeque<usize>
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
                lookahead_stack: VecDeque::with_capacity(LOOKAHEAD_BUFFER_CAPACITY)
            }
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
                self.lookahead_stack.front().copied()
            }
            else
            {
                Some(self.position)
            };

        if let Some(lookahead) = lookahead
        {
            self.lookahead_stack.push_front(lookahead);
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
                self.lookahead_stack[0] = lookahead;
            }
            else
            {
                self.position = lookahead;
                self.clear_finished_buffer();
            }
        }
    }

    /**
     * Rollback the current lookahead transaction, reverting any changes made since it was marked.
     */
    pub fn rollback_lookahead(&mut self)
    {
        if self.lookahead_stack.pop_front().is_some()
        {
            self.clear_finished_buffer();
        }
    }

    /**
     * Extract the next token from the buffer or tokenizer. if there is a lookahead transaction
     * active then this isn't a permanent change.
     */
    pub fn next(&mut self) -> Result<Option<Token>, TokenizerError>
    {
        let index = self.lookahead_stack.front().copied().unwrap_or(self.position);

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
            *cursor += 1;
         }
        else
        {
            self.position += 1;
            self.clear_finished_buffer();
        }

        Ok(Some(token))
    }

    /**
     * Current cursor within the token cache, including lookahead. Positions can
     * be compared while a lookahead transaction remains active; cache cleanup
     * outside a transaction may reset the cursor.
     */
    pub(crate) fn position(&self) -> usize
    {
        self.lookahead_stack.front().copied().unwrap_or(self.position)
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
