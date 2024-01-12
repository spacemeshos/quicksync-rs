use std::error::Error;
use rusqlite::{Connection, params};

pub fn get_last_layer_from_db(db_path: &str) -> Result<i32, Box<dyn Error>> {
  let conn = Connection::open(db_path)?;

  let mut stmt = conn.prepare("SELECT * FROM layers ORDER BY id DESC LIMIT 1")?;
  let mut layer_iter = stmt.query_map(params![], |row| {
      Ok(row.get::<_, i32>(0)?)
  })?;

  if let Some(result) = layer_iter.next() {
      let last_id = result?;
      Ok(last_id)
  } else {
      Ok(0)
  }
}