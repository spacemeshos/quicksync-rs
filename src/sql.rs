use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::PathBuf;

pub fn get_last_layer_from_db(db_path: &PathBuf) -> Result<i32> {
  let conn = Connection::open(db_path).context("Failed to connect to db")?;

  let mut stmt = conn.prepare("SELECT * FROM layers ORDER BY id DESC LIMIT 1")?;
  let mut layer_iter = stmt.query_map(params![], |row| row.get::<_, i32>(0))?;

  if let Some(result) = layer_iter.next() {
    let last_id = result?;
    Ok(last_id)
  } else {
    Ok(0)
  }
}
