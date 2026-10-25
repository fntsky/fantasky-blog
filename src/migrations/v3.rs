use rusqlite::{Result, Transaction};

pub(super) fn up(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch(
        "ALTER TABLE posts ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1));
         CREATE TABLE blog_state (id INTEGER PRIMARY KEY CHECK (id = 1), imported INTEGER NOT NULL);
         INSERT INTO blog_state VALUES (1, EXISTS(SELECT 1 FROM posts));",
    )
}
