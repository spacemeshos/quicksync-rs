use anyhow::{Context, Result};
// use parse_display::{Display, FromStr};
use reqwest::blocking::Client;
use rusqlite::Connection;
use std::{
  env,
  fs::{self, File},
  io::{self, BufReader, BufWriter},
  str::FromStr,
  time::Instant,
};
use zstd::stream::Decoder;

const DEFAULT_BASE_URL: &str = "https://quicksync.spacemesh.network/partials";

#[derive(Clone, Debug, parse_display::FromStr)]
#[display("{from},{to},{hash}")]
struct RestorePoint {
  from: i64,
  to: i64,
  hash: String,
}

fn get_base_url() -> String {
  env::var("QS_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

fn get_previous_hash(layer_at: i64, conn: &Connection) -> Result<String> {
  conn
    .query_row(
      "SELECT aggregated_hash FROM layers WHERE id = ?",
      [layer_at - 1],
      |row| {
        let hash: Vec<u8> = row.get(0)?;
        Ok(hex::encode(&hash[..2]))
      },
    )
    .context("Failed to get previous hash")
}

fn find_start_points(layer_from: i64, metadata: &str, jump_back: usize) -> Vec<RestorePoint> {
  let mut result = Vec::new();

  let mut target_index = 0;

  for (index, line) in metadata.lines().enumerate() {
    let items: Vec<&str> = line.split(',').collect();
    let to: i64 = items[1].parse().unwrap();
    if to > layer_from {
      target_index = index;
      break;
    }
  }
  target_index = target_index - jump_back;
  for line in metadata.lines().skip(target_index) {
    result.push(RestorePoint::from_str(line).unwrap());
  }
  result
}

fn get_latest_from_db(target_path: &str) -> Result<i64> {
  let conn = Connection::open(target_path)?;
  conn
    .query_row(
      "SELECT max(id) FROM layers WHERE applied_block IS NOT null",
      [],
      |row| row.get(0),
    )
    .context("Failed to get latest layer from DB")
}

fn get_user_version(target_path: &str) -> Result<i64> {
  let conn = Connection::open(target_path)?;
  conn
    .query_row("PRAGMA user_version", [], |row| row.get(0))
    .context("Failed to get user version")
}

fn download_file(
  client: &Client,
  user_version: i64,
  layer_from: i64,
  layer_to: i64,
  hash: &str,
  target_path: &str,
) -> Result<()> {
  let base_url = get_base_url();
  let suffix = if target_path.ends_with("zst") {
    ".zst"
  } else {
    ""
  };
  let url = format!(
    "{}/{}/{}_{}_{}/state.sql_diff.{}_{}.sql{}",
    base_url, user_version, layer_from, layer_to, hash, layer_from, layer_to, suffix
  );
  println!("Downloading from {}", url);
  let mut resp = client.get(&url).send().context("Failed to send request")?;
  if !resp.status().is_success() {
    anyhow::bail!(
      "Failed to download file {}: HTTP status {}",
      url,
      resp.status()
    );
  }
  let mut file = File::create(target_path).context("Failed to create file")?;
  resp
    .copy_to(&mut file)
    .context("Failed to copy response to file")?;
  Ok(())
}

fn decompress_file(input_path: &str, output_path: &str) -> Result<()> {
  let input_file = File::open(input_path).context("Failed to open input file")?;
  let output_file = File::create(output_path).context("Failed to create output file")?;

  let mut reader = BufReader::new(input_file);
  let mut writer = BufWriter::new(output_file);

  let mut decoder = Decoder::new(&mut reader).context("Failed to create decoder")?;
  decoder
    .window_log_max(31)
    .context("Failed to set window log max")?;

  io::copy(&mut decoder, &mut writer).context("Failed to decompress")?;

  Ok(())
}

fn execute_restore(target_conn: &Connection, restore_string: &str) -> Result<()> {
  target_conn
    .execute_batch(restore_string)
    .context("Failed to execute restore")
}

pub fn partial_restore(layer_from: i64, target_db_path: &str, jump_back: usize) -> Result<()> {
  let client = Client::new();
  let base_url = get_base_url();
  let user_version = get_user_version(target_db_path)?;
  let remote_metadata = client
    .get(format!("{}/{}/metadata.csv", base_url, user_version))
    .send()?
    .text()?;
  let restore_string = client
    .get(format!("{}/{}/restore.sql", base_url, user_version))
    .send()?
    .text()?;
  let layer_from = if layer_from == 0 {
    get_latest_from_db(target_db_path)?
  } else {
    layer_from
  };
  let start_points = find_start_points(layer_from, &remote_metadata, jump_back);
  if start_points.is_empty() {
    anyhow::bail!("No suitable restore point found");
  }
  println!("Found {} potential restore points", start_points.len());

  for point in start_points {
    let conn = Connection::open(target_db_path)?;
    let previous_hash = get_previous_hash(point.from, &conn)?;

    if previous_hash == &point.hash[..4] {
      let source_db_path_zst = "backup_source.db.zst";
      let source_db_path = "backup_source.db";

      if download_file(
        &client,
        user_version,
        point.from,
        point.to,
        &point.hash,
        source_db_path_zst,
      )
      .is_err()
      {
        download_file(
          &client,
          user_version,
          point.from,
          point.to,
          &point.hash,
          source_db_path,
        )?;
      } else {
        decompress_file(source_db_path_zst, source_db_path)?;
        fs::remove_file(source_db_path_zst)?;
      }

      println!("Restoring from {} to {}...", point.from, point.to);
      let start = Instant::now();
      execute_restore(&conn, &restore_string)?;
      fs::remove_file(source_db_path)?;
      let duration = start.elapsed();
      println!("Restored {} to {} in {:?}", point.from, point.to, duration);
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use anyhow::Result;
  use rusqlite::Connection;
  use tempfile::tempdir;

  fn create_test_db() -> Result<(tempfile::TempDir, String)> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let conn = Connection::open(&db_path)?;
    conn.execute(
      "CREATE TABLE layers (id INTEGER, applied_block INTEGER, aggregated_hash BLOB)",
      [],
    )?;
    conn.execute(
      "INSERT INTO layers (id, applied_block, aggregated_hash) VALUES (?, ?, ?), (?, ?, ?)",
      rusqlite::params![1, 100, 0xAAAA, 2, 200, 0xBBBB],
    )?;
    Ok((dir, db_path.to_str().unwrap().to_string()))
  }

  #[test]
  fn test_get_base_url() {
    std::env::set_var("QS_BASE_URL", "https://test.com");
    assert_eq!(get_base_url(), "https://test.com");
    std::env::remove_var("QS_BASE_URL");
    assert_eq!(get_base_url(), DEFAULT_BASE_URL);
  }

  #[test]
  fn test_find_start_points() {
    let metadata = "0,100,aaaa\n101,200,bbbb\n201,300,ijkl\n";
    let result = find_start_points(150, metadata, 0);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].from, 101);
    assert_eq!(result[0].to, 200);
    assert_eq!(result[0].hash, "bbbb");

    let result = find_start_points(150, metadata, 1);
    println!("{:?}", result);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].from, 0);
    assert_eq!(result[0].to, 100);
    assert_eq!(result[0].hash, "aaaa");
  }

  #[test]
  fn test_get_latest_from_db() -> Result<()> {
    let (_dir, db_path) = create_test_db()?;
    let result = get_latest_from_db(&db_path)?;
    assert_eq!(result, 2);
    Ok(())
  }

  #[test]
  fn test_get_user_version() -> Result<()> {
    let (_dir, db_path) = create_test_db()?;
    let conn = Connection::open(&db_path)?;
    conn.execute("PRAGMA user_version = 42", [])?;
    let result = get_user_version(&db_path)?;
    assert_eq!(result, 42);
    Ok(())
  }
}
