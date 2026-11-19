use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

pub struct TranslationCache {
    conn: Mutex<Connection>,
}

impl TranslationCache {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS translation_cache (
                 source_text TEXT NOT NULL,
                 target_lang TEXT NOT NULL,
                 translated  TEXT NOT NULL,
                 provider    TEXT NOT NULL,
                 updated_at  INTEGER NOT NULL,
                 PRIMARY KEY (source_text, target_lang)
             );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn get(&self, source_text: &str, target_lang: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("cache lock poisoned");
        let res = conn
            .query_row(
                "SELECT translated FROM translation_cache WHERE source_text = ?1 AND target_lang = ?2",
                params![source_text, target_lang],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(res)
    }

    pub fn put(
        &self,
        source_text: &str,
        target_lang: &str,
        translated: &str,
        provider: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("cache lock poisoned");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO translation_cache (source_text, target_lang, translated, provider, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source_text, target_lang)
             DO UPDATE SET translated = excluded.translated, provider = excluded.provider, updated_at = excluded.updated_at",
            params![source_text, target_lang, translated, provider, now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_in_memory_cache() {
        let cache = TranslationCache::open(":memory:").unwrap();
        assert_eq!(cache.get("hello", "中文").unwrap(), None);
        cache.put("hello", "中文", "你好", "demo").unwrap();
        assert_eq!(cache.get("hello", "中文").unwrap().as_deref(), Some("你好"));
    }
}
