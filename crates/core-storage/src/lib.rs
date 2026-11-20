pub mod settings;

pub use settings::{AppSettings, HotkeySettings};

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct TranslationCache {
    conn: Mutex<Connection>,
}

impl TranslationCache {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path != Path::new(":memory:") {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 3000;
             CREATE TABLE IF NOT EXISTS translation_cache_v2 (
                 source_text TEXT NOT NULL,
                 target_lang TEXT NOT NULL,
                 provider    TEXT NOT NULL,
                 translated  TEXT NOT NULL,
                 updated_at  INTEGER NOT NULL,
                 PRIMARY KEY (source_text, target_lang, provider)
             );
             CREATE TABLE IF NOT EXISTS favorites (
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

    pub fn default_path() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Lumen")
            .join("lumen_cache.sqlite3")
    }

    pub fn get(
        &self,
        source_text: &str,
        target_lang: &str,
        provider: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("cache lock poisoned");
        let res = conn
            .query_row(
                "SELECT translated FROM translation_cache_v2
                 WHERE source_text = ?1 AND target_lang = ?2 AND provider = ?3",
                params![source_text, target_lang, provider],
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
        if translated.trim().is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().expect("cache lock poisoned");
        conn.execute(
            "INSERT INTO translation_cache_v2
             (source_text, target_lang, translated, provider, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source_text, target_lang, provider)
             DO UPDATE SET translated = excluded.translated, updated_at = excluded.updated_at",
            params![source_text, target_lang, translated, provider, unix_time()],
        )?;
        conn.execute(
            "DELETE FROM translation_cache_v2
             WHERE rowid NOT IN (
                 SELECT rowid FROM translation_cache_v2 ORDER BY updated_at DESC LIMIT 2000
             )",
            [],
        )?;
        Ok(())
    }

    pub fn is_favorite(&self, source_text: &str, target_lang: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("cache lock poisoned");
        let exists = conn.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM favorites WHERE source_text = ?1 AND target_lang = ?2
             )",
            params![source_text, target_lang],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(exists == 1)
    }

    pub fn toggle_favorite(
        &self,
        source_text: &str,
        target_lang: &str,
        translated: &str,
        provider: &str,
    ) -> Result<bool> {
        if source_text.trim().is_empty() || translated.trim().is_empty() {
            return Ok(false);
        }
        let conn = self.conn.lock().expect("cache lock poisoned");
        let deleted = conn.execute(
            "DELETE FROM favorites WHERE source_text = ?1 AND target_lang = ?2",
            params![source_text, target_lang],
        )?;
        if deleted > 0 {
            return Ok(false);
        }
        conn.execute(
            "INSERT INTO favorites (source_text, target_lang, translated, provider, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![source_text, target_lang, translated, provider, unix_time()],
        )?;
        Ok(true)
    }
}

fn unix_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_is_scoped_by_provider() {
        let cache = TranslationCache::open(":memory:").unwrap();
        cache.put("hello", "auto", "你好", "free").unwrap();
        assert_eq!(
            cache.get("hello", "auto", "free").unwrap().as_deref(),
            Some("你好")
        );
        assert_eq!(cache.get("hello", "auto", "llm").unwrap(), None);
    }

    #[test]
    fn favorites_roundtrip() {
        let cache = TranslationCache::open(":memory:").unwrap();
        assert!(cache
            .toggle_favorite("lumen", "auto", "流明", "free")
            .unwrap());
        assert!(cache.is_favorite("lumen", "auto").unwrap());
        assert!(!cache
            .toggle_favorite("lumen", "auto", "流明", "free")
            .unwrap());
        assert!(!cache.is_favorite("lumen", "auto").unwrap());
    }

    #[test]
    fn default_path_is_absolute() {
        assert!(TranslationCache::default_path().is_absolute());
    }
}
