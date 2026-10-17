
use crate::language::tokenizer::TokenBuffer;



/**
 * A guard that manages lookahead in the token buffer. When created it marks the current position
 * for an experimental lookahead. If the parser rule matches successfully, the lookahead can be
 * committed using the `commit` method. Otherwise, it will automatically roll back when the guard
 * is dropped. Thus allowing the parser to safely just return an error at the moment of failure.
 */
pub(super) struct LookaheadGuard<'buffer, 'tokenizer, 'input>
{
    /**
     * The token buffer that this guard is managing lookahead for.
     */
    pub buffer: &'buffer mut TokenBuffer<'tokenizer, 'input>,

    /**
     * Indicates whether the lookahead has been committed. Used by the drop handler to determine
     * whether to roll back the lookahead.
     */
    committed: bool
}


impl<'buffer, 'tokenizer, 'input> LookaheadGuard<'buffer, 'tokenizer, 'input>
{
    /**
     * Creates a new lookahead guard for the given token buffer. This will mark the current position
     * in the buffer for lookahead. The lookahead can later be committed using the `commit` method,
     * or it will automatically roll back when the guard is dropped if not committed.
     */
    pub fn new(buffer: &'buffer mut TokenBuffer<'tokenizer, 'input>) -> Self
    {
        buffer.mark_lookahead();

        Self
            {
                buffer,
                committed: false
            }
    }

    /**
     * Commits the lookahead, indicating that the parser rule matched successfully.
     * After calling this method, the lookahead will not be rolled back when the guard is dropped.
     */
    pub fn commit(&mut self)
    {
        self.buffer.commit_lookahead();
        self.committed = true;
    }
}


impl Drop for LookaheadGuard<'_, '_, '_>
{
    /**
     * Drops the lookahead guard. If the lookahead has not been committed, it will automatically
     * roll back the lookahead in the token buffer.
     */
    fn drop(&mut self)
    {
        if !self.committed
        {
            self.buffer.rollback_lookahead();
        }
    }
}