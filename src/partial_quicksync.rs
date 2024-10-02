use anyhow::{Context, Result};
use reqwest::blocking::Client;
use rusqlite::Connection;
use std::{
  fs::{self, File},
  io::{self, BufReader, BufWriter},
  path::Path,
  str::FromStr,
  time::Instant,
};
use zstd::stream::Decoder;

pub(crate) const DEFAULT_BASE_URL: &str = "https://quicksync.spacemesh.network/partials";

#[derive(Clone, Debug, PartialEq, Eq, parse_display::Display, parse_display::FromStr)]
#[display("{from},{to},{hash}")]
struct RestorePoint {
  from: u32,
  to: u32,
  hash: String,
}

fn get_previous_hash(layer_at: u32, conn: &Connection) -> Result<String> {
  let layer_at = layer_at - 1;
  conn
    .query_row(
      "SELECT aggregated_hash FROM layers WHERE id = ?",
      [layer_at],
      |row| {
        let hash: Vec<u8> = row.get(0)?;
        Ok(hex::encode(&hash[..2]))
      },
    )
    .with_context(|| format!("failed to get previous hash for layer {layer_at}"))
}

// Find restore points for layers >= `layer_from` in layers described by `metadata`.
// The `metadata` holds non-overlapping, ordered restore points (one per line) in form:
// {layer_from (inlusive)},{layer_to (exclusive)},{short hash (4)}
//
// The `jump_back` tells how many "previous" points should be included in
// the returned vector.
fn find_restore_points(layer_from: u32, metadata: &str, jump_back: usize) -> Vec<RestorePoint> {
  let mut all_points = Vec::new();
  let mut target_index = None;

  for (index, line) in metadata.trim().lines().enumerate() {
    let point = RestorePoint::from_str(line.trim()).expect("parsing restore point");
    if point.to > layer_from && target_index.is_none() {
      target_index = Some(index);
    }
    all_points.push(point);
  }
  // A None `target_index` means there aren't any layers > `layer_from`
  // in the data described by `metadata`.
  let target_index = if let Some(t) = target_index {
    if t >= jump_back {
      t - jump_back
    } else {
      0
    }
  } else {
    return vec![];
  };

  all_points.split_off(target_index)
}

fn get_latest_from_db(conn: &Connection) -> Result<u32> {
  conn
    .query_row(
      "SELECT max(id) FROM layers WHERE applied_block IS NOT null",
      [],
      |row| row.get(0),
    )
    .context("failed to get latest layer from DB")
}

fn get_user_version(conn: &Connection) -> Result<usize> {
  conn
    .query_row("PRAGMA user_version", [], |row| row.get(0))
    .context("failed to get user version")
}

fn file_url(user_version: usize, p: &RestorePoint, suffix: Option<&str>) -> String {
  let suffix = suffix.unwrap_or_default();
  format!(
    "{}/{}_{}_{}/state.sql_diff.{}_{}.sql{}",
    user_version, p.from, p.to, p.hash, p.from, p.to, suffix
  )
}

fn download_file(
  client: &Client,
  base_url: &str,
  user_version: usize,
  point: &RestorePoint,
  target_path: &Path,
) -> Result<()> {
  let suffix = target_path
    .extension()
    .is_some_and(|ext| ext == "zst")
    .then_some(".zst");
  let url = format!("{}/{}", base_url, file_url(user_version, point, suffix));
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

fn decompress_file(input_path: &Path, output_path: &Path) -> Result<()> {
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

pub fn partial_restore(base_url: &str, conn: &Connection, jump_back: usize) -> Result<()> {
  let client = Client::new();
  let user_version = get_user_version(conn)?;
  let remote_metadata = client
    .get(format!("{}/{}/metadata.csv", base_url, user_version))
    .send()?
    .text()?;
  let restore_string = client
    .get(format!("{}/{}/restore.sql", base_url, user_version))
    .send()?
    .text()?;
  let latest_layer = get_latest_from_db(conn)?;
  let layer_from = latest_layer + 1; // start with the first layer that is not in the DB
  let start_points = find_restore_points(layer_from, &remote_metadata, jump_back);
  if start_points.is_empty() {
    anyhow::bail!("No suitable restore point found");
  }
  let total = start_points.len();
  println!("Found {total} potential restore points");

  for (idx, p) in start_points.into_iter().enumerate() {
    let previous_hash = get_previous_hash(p.from, conn)?;
    anyhow::ensure!(
      previous_hash == p.hash[..4],
      "unexpected hash: '{previous_hash}' doesn't match restore point {p:?}",
    );
    let source_db_path_zst = &Path::new("backup_source.db.zst");
    let source_db_path = &Path::new("backup_source.db");

    if download_file(&client, base_url, user_version, &p, source_db_path_zst).is_err() {
      download_file(&client, base_url, user_version, &p, source_db_path)?;
    } else {
      decompress_file(source_db_path_zst, source_db_path)?;
      fs::remove_file(source_db_path_zst)?;
    }

    println!("[{idx}/{total}] Restoring from {} to {}...", p.from, p.to);
    let start = Instant::now();
    conn
      .execute_batch(&restore_string)
      .context("executing restore")?;
    fs::remove_file(source_db_path)?;
    let duration = start.elapsed();
    println!(
      "[{idx}/{total}] Restored {} to {} in {:?}",
      p.from, p.to, duration
    );
  }
  Ok(())
}

#[cfg(test)]
impl RestorePoint {
  fn new(from: u32, to: u32, hash: String) -> Self {
    Self { from, to, hash }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use rusqlite::Connection;
  use tempfile::tempdir;

  fn create_test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn
      .execute(
        "CREATE TABLE layers (id INTEGER, applied_block INTEGER, aggregated_hash BLOB)",
        [],
      )
      .unwrap();
    conn
  }

  #[test]
  fn test_find_start_points() {
    let metadata = r#"
    0,100,aaaa
    101,200,bbbb
    201,300,ijkl
    "#;
    let result = find_restore_points(150, metadata, 0);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].from, 101);
    assert_eq!(result[0].to, 200);
    assert_eq!(result[0].hash, "bbbb");

    let result = find_restore_points(150, metadata, 1);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].from, 0);
    assert_eq!(result[0].to, 100);
    assert_eq!(result[0].hash, "aaaa");

    // `jump_back` over the first point
    let result2 = find_restore_points(150, metadata, 5);
    assert_eq!(result, result2);

    // no points for the requested starting layer
    let result = find_restore_points(500, metadata, 1);
    assert!(result.is_empty());
  }

  fn insert_layer(conn: &Connection, id: u32, applied_block: i64, hash: &[u8]) {
    conn
      .execute(
        "INSERT INTO layers (id, applied_block, aggregated_hash) VALUES (?, ?, ?)",
        rusqlite::params![id, applied_block, hash],
      )
      .unwrap();
  }

  #[test]
  fn getting_previous_hash() {
    let conn = create_test_db();
    insert_layer(&conn, 2, 100, &[0xAA, 0xBB]);
    let result = get_previous_hash(3, &conn).unwrap();
    assert_eq!("aabb", result);
  }

  #[test]
  fn test_get_latest_from_db() {
    let conn = create_test_db();
    insert_layer(&conn, 2, 100, &[0xAA, 0xBB]);
    let result = get_latest_from_db(&conn).unwrap();
    assert_eq!(result, 2);
  }

  #[test]
  fn test_get_user_version() {
    let conn = create_test_db();
    conn.execute("PRAGMA user_version = 42", []).unwrap();
    let result = get_user_version(&conn).unwrap();
    assert_eq!(result, 42);
  }

  #[test]
  fn downloading_file() {
    let point = RestorePoint {
      from: 100,
      to: 200,
      hash: "abcd".to_string(),
    };
    let file_url = file_url(1, &point, Some(".zst"));
    let mut server = mockito::Server::new();
    let mock = server
      .mock("GET", format!("/{file_url}").as_str())
      .with_status(200)
      .with_body("file contents")
      .create();

    let dir = tempdir().unwrap();
    let dst = dir.path().join("dst.zst");
    super::download_file(&Client::new(), &server.url(), 1, &point, &dst).unwrap();
    mock.assert();

    let data = std::fs::read(&dst).unwrap();
    assert_eq!(&data, "file contents".as_bytes());
  }

  #[test]
  fn partial_restore() {
    let conn = create_test_db();
    insert_layer(&conn, 99, 100, &[0xBB, 0xBB]);
    let mut server = mockito::Server::new();
    let user_version = 0;

    let points = [
      ("bbbb", RestorePoint::new(0, 100, "aaaa".into())),
      ("cccc", RestorePoint::new(100, 200, "bbbb".into())),
      ("dddd", RestorePoint::new(200, 300, "cccc".into())),
      ("eeee", RestorePoint::new(300, 400, "dddd".into())),
    ];

    let metadata = points
      .iter()
      .map(|(_, p)| p.to_string())
      .collect::<Vec<_>>()
      .join("\n");

    let mock_metadata = server
      .mock("GET", format!("/{user_version}/metadata.csv").as_str())
      .with_status(200)
      .with_body(metadata)
      .create();

    // Restore SQL just copies contents of the `layers` table
    let mock_query = server
      .mock("GET", format!("/{user_version}/restore.sql").as_str())
      .with_status(200)
      .with_body(
        r#"ATTACH DATABASE 'backup_source.db' AS src;
           INSERT OR IGNORE INTO layers SELECT * from src.layers;
           DETACH DATABASE src"#,
      )
      .create();

    let data_mocks = points
      .iter()
      .skip(1)
      .map(|(hash, point)| {
        // For simplicity, the database used to restore contains only
        // the last layer of the point and its expected hash.
        let conn = create_test_db();
        let hash = hex::decode(hash).unwrap();
        insert_layer(&conn, point.to - 1, 111, &hash);
        let dir = tempdir().unwrap();
        let checkpoint = dir.path().join("checkpoint.db");
        conn
          .backup(rusqlite::DatabaseName::Main, &checkpoint, None)
          .unwrap();

        let file_url = file_url(user_version, point, None);
        server
          .mock("GET", format!("/{file_url}").as_str())
          .with_status(200)
          .with_body(std::fs::read(&checkpoint).unwrap())
          .create()
      })
      .collect::<Vec<_>>();

    super::partial_restore(&server.url(), &conn, 0).unwrap();

    mock_metadata.assert();
    mock_query.assert();
    for mock in data_mocks {
      mock.assert();
    }

    let latest = get_latest_from_db(&conn).unwrap();
    assert_eq!(latest, points.last().unwrap().1.to - 1);

    let result = get_previous_hash(latest + 1, &conn).unwrap();
    assert_eq!(result, points.last().unwrap().0);
  }

  #[test]
  fn fails_on_hash_mismatch() {
    let conn = create_test_db();
    insert_layer(&conn, 99, 100, &[0xFF, 0xFF]);
    let mut server = mockito::Server::new();
    let user_version = 0;

    let metadata = RestorePoint::new(100, 200, "aaaa".to_string()).to_string();
    let mock_metadata = server
      .mock("GET", format!("/{user_version}/metadata.csv").as_str())
      .with_status(200)
      .with_body(metadata)
      .create();

    let mock_query = server
      .mock("GET", format!("/{user_version}/restore.sql").as_str())
      .with_status(200)
      .with_body(".import backup_source.db layers")
      .create();

    let err = super::partial_restore(&server.url(), &conn, 0).unwrap_err();
    assert!(err.to_string().contains("unexpected hash"));
    mock_metadata.assert();
    mock_query.assert();
  }
}
