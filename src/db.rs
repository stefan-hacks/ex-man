use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::PathBuf;

use crate::manpage::ManOption;

/// Local SQLite database for caching parsed manpage data and generated examples
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create the database in the user's XDG data directory
    pub fn open() -> Result<Self> {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("ex-man");

        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("ex-man.db");

        let conn = Connection::open(&db_path)?;
        let db = Database { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS tools (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                last_updated INTEGER NOT NULL DEFAULT (unixepoch())
            );

            CREATE TABLE IF NOT EXISTS options (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_id INTEGER NOT NULL,
                short_flag TEXT,
                long_flag TEXT,
                description TEXT NOT NULL,
                takes_arg INTEGER NOT NULL DEFAULT 0,
                arg_name TEXT,
                section TEXT NOT NULL DEFAULT 'UNKNOWN',
                FOREIGN KEY (tool_id) REFERENCES tools(id) ON DELETE CASCADE,
                UNIQUE(tool_id, short_flag, long_flag)
            );

            CREATE INDEX IF NOT EXISTS idx_options_tool ON options(tool_id);
            CREATE INDEX IF NOT EXISTS idx_options_short ON options(short_flag);
            CREATE INDEX IF NOT EXISTS idx_options_long ON options(long_flag);

            CREATE TABLE IF NOT EXISTS examples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_id INTEGER NOT NULL,
                command_line TEXT NOT NULL,
                description TEXT NOT NULL,
                flags_used TEXT NOT NULL DEFAULT '', -- comma-separated short/long flags
                generated_at INTEGER NOT NULL DEFAULT (unixepoch()),
                FOREIGN KEY (tool_id) REFERENCES tools(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_examples_tool ON examples(tool_id);
            "#,
        )?;
        Ok(())
    }

    /// Check if a tool already exists in the database
    pub fn has_tool(&self, tool_name: &str) -> Result<bool> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tools WHERE name = ?1",
                params![tool_name],
                |row| row.get(0),
            )?;
        Ok(count > 0)
    }

    /// Store a tool and its parsed options
    pub fn store_tool(&self, tool_name: &str, options: &[ManOption]) -> Result<()> {
        // Begin transaction
        let tx = self.conn.unchecked_transaction()?;

        // Insert or replace tool
        tx.execute(
            "INSERT OR REPLACE INTO tools (name, last_updated) VALUES (?1, unixepoch())",
            params![tool_name],
        )?;

        let tool_id: i64 = tx.query_row(
            "SELECT id FROM tools WHERE name = ?1",
            params![tool_name],
            |row| row.get(0),
        )?;

        // Delete old options for this tool
        tx.execute(
            "DELETE FROM options WHERE tool_id = ?1",
            params![tool_id],
        )?;

        // Insert new options
        for opt in options {
            tx.execute(
                "INSERT INTO options (tool_id, short_flag, long_flag, description, takes_arg, arg_name, section)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    tool_id,
                    opt.short.as_deref(),
                    opt.long.as_deref(),
                    &opt.description,
                    opt.takes_arg as i64,
                    opt.arg_name.as_deref(),
                    &opt.section,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Load a tool's options from the database
    pub fn load_tool(&self, tool_name: &str) -> Result<Vec<ManOption>> {
        let tool_id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM tools WHERE name = ?1",
                params![tool_name],
                |row| row.get(0),
            )?;

        let mut stmt = self.conn.prepare(
            "SELECT short_flag, long_flag, description, takes_arg, arg_name, section
             FROM options WHERE tool_id = ?1
             ORDER BY id",
        )?;

        let option_iter = stmt.query_map(params![tool_id], |row| {
            Ok(ManOption {
                short: row.get(0)?,
                long: row.get(1)?,
                description: row.get(2)?,
                takes_arg: row.get(3)?,
                arg_name: row.get(4)?,
                section: row.get(5)?,
            })
        })?;

        let mut options = Vec::new();
        for opt in option_iter {
            options.push(opt?);
        }

        Ok(options)
    }

    /// Store generated examples for a tool
    pub fn store_examples(&self, tool_name: &str, examples: &[(String, String, Vec<String>)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        let tool_id: i64 = tx.query_row(
            "SELECT id FROM tools WHERE name = ?1",
            params![tool_name],
            |row| row.get(0),
        )?;

        // Delete old examples
        tx.execute(
            "DELETE FROM examples WHERE tool_id = ?1",
            params![tool_id],
        )?;

        for (cmd_line, desc, flags) in examples {
            let flags_str = flags.join(",");
            tx.execute(
                "INSERT INTO examples (tool_id, command_line, description, flags_used)
                 VALUES (?1, ?2, ?3, ?4)",
                params![tool_id, cmd_line, desc, flags_str],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Load stored examples for a tool
    pub fn load_examples(&self, tool_name: &str) -> Result<Vec<(String, String, Vec<String>)>> {
        let tool_id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM tools WHERE name = ?1",
                params![tool_name],
                |row| row.get(0),
            )?;

        let mut stmt = self.conn.prepare(
            "SELECT command_line, description, flags_used
             FROM examples WHERE tool_id = ?1
             ORDER BY id",
        )?;

        let example_iter = stmt.query_map(params![tool_id], |row| {
            let cmd: String = row.get(0)?;
            let desc: String = row.get(1)?;
            let flags_str: String = row.get(2)?;
            let flags = if flags_str.is_empty() {
                Vec::new()
            } else {
                flags_str.split(',').map(|s| s.to_string()).collect()
            };
            Ok((cmd, desc, flags))
        })?;

        let mut examples = Vec::new();
        for ex in example_iter {
            examples.push(ex?);
        }

        Ok(examples)
    }

    /// List all known tools in the database
    pub fn list_known_tools(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM tools ORDER BY name")?;

        let tool_iter = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            Ok(name)
        })?;

        let mut tools = Vec::new();
        for t in tool_iter {
            tools.push(t?);
        }
        Ok(tools)
    }

    /// Search for tools by partial name match
    pub fn search_tools(&self, query: &str) -> Result<Vec<String>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM tools WHERE name LIKE ?1 ORDER BY name")?;

        let tool_iter = stmt.query_map(params![pattern], |row| {
            let name: String = row.get(0)?;
            Ok(name)
        })?;

        let mut tools = Vec::new();
        for t in tool_iter {
            tools.push(t?);
        }
        Ok(tools)
    }

    /// Get the number of tools in the database
    pub fn tool_count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tools", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get the number of options for a specific tool
    pub fn option_count(&self, tool_name: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(o.id) FROM options o
             JOIN tools t ON o.tool_id = t.id
             WHERE t.name = ?1",
            params![tool_name],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_database_operations() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let conn = Connection::open(&db_path).unwrap();
        let db = Database { conn };
        db.init_schema().unwrap();

        let options = vec![
            ManOption {
                short: Some("-a".to_string()),
                long: Some("--all".to_string()),
                description: "Show all".to_string(),
                takes_arg: false,
                arg_name: None,
                section: "OPTIONS".to_string(),
            },
            ManOption {
                short: Some("-v".to_string()),
                long: Some("--verbose".to_string()),
                description: "Verbose output".to_string(),
                takes_arg: false,
                arg_name: None,
                section: "OPTIONS".to_string(),
            },
        ];

        db.store_tool("ls", &options).unwrap();
        assert!(db.has_tool("ls").unwrap());
        assert_eq!(db.tool_count().unwrap(), 1);
        assert_eq!(db.option_count("ls").unwrap(), 2);

        let loaded = db.load_tool("ls").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].short, Some("-a".to_string()));

        let tools = db.list_known_tools().unwrap();
        assert_eq!(tools, vec!["ls"]);

        let search = db.search_tools("l").unwrap();
        assert_eq!(search, vec!["ls"]);
    }
}
