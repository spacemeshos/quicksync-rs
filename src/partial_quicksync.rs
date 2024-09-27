use anyhow::{Context, Result};
use reqwest::blocking::Client;
use rusqlite::Connection;
use std::{
  env,
  fs::{self, File},
  io::{self, BufReader, BufWriter},
  time::Instant,
};
use zstd::stream::Decoder;

const DEFAULT_BASE_URL: &str = "https://quicksync.spacemesh.network/partials";

#[derive(Clone, Debug)]
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
  let mut continuous = false;

  for (index, line) in metadata.lines().enumerate() {
    let items: Vec<&str> = line.split(',').collect();
    let from: i64 = items[0].parse().unwrap();
    let to: i64 = items[1].parse().unwrap();

    if to > layer_from && from <= layer_from || continuous {
      continuous = true;
      if index >= jump_back {
        let jump_back_line = metadata.lines().nth(index - jump_back).unwrap();
        let jump_back_items: Vec<&str> = jump_back_line.split(',').collect();

        result.push(RestorePoint {
          from: jump_back_items[0].parse().unwrap(),
          to: jump_back_items[1].parse().unwrap(),
          hash: jump_back_items[2].to_string(),
        });
      }
    }
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
      }

      println!("Restoring from {} to {}...", point.from, point.to);
      let start = Instant::now();
      execute_restore(&conn, &restore_string)?;
      fs::remove_file(source_db_path)?;
      let _ = fs::remove_file(source_db_path_zst);
      let duration = start.elapsed();
      println!("Restored {} to {} in {:?}", point.from, point.to, duration);
    }
  }
  Ok(())
}
