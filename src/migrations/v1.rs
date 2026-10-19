use rusqlite::{Result, Transaction};

pub(super) fn up(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch(
        "CREATE TABLE posts (
             slug TEXT PRIMARY KEY NOT NULL,
             title TEXT NOT NULL,
             excerpt TEXT NOT NULL,
             markdown TEXT NOT NULL,
             published_on TEXT NOT NULL,
             tags_json TEXT NOT NULL DEFAULT '[]',
             updated_at INTEGER NOT NULL
         );
         CREATE INDEX posts_published_on ON posts(published_on DESC, slug ASC);",
    )
}
