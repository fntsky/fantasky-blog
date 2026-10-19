use rusqlite::{Result, Transaction};

pub(super) fn up(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch(
        "ALTER TABLE posts ADD COLUMN created_by TEXT NOT NULL DEFAULT '';
         ALTER TABLE posts ADD COLUMN updated_by TEXT NOT NULL DEFAULT '';
         UPDATE posts SET created_by = 'Markdown import', updated_by = 'Markdown import';",
    )
}
