use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use std::{env, path::PathBuf};
use url::{ParseError, Url};

pub fn strip_trailing_newline(input: &str) -> &str {
  input.trim_end()
}

pub fn calculate_latest_layer(
  genesis_time: DateTime<Utc>,
  layer_duration: Duration,
) -> Result<i64> {
  let delta = Utc::now() - genesis_time;
  Ok(delta.num_milliseconds() / layer_duration.num_milliseconds())
}

pub fn resolve_path(relative_path: &PathBuf) -> Result<PathBuf> {
  let current_dir = env::current_dir()?;
  let resolved_path = current_dir.join(relative_path);
  Ok(resolved_path)
}

pub fn trim_version(version: &str) -> &str {
  version.split('+').next().unwrap_or(version)
}

pub fn build_url(base: &Url, path: &str) -> Result<Url, ParseError> {
  let mut url = base.clone();
  url
    .path_segments_mut()
    .expect("cannot be base")
    .extend(path.split('/'));
  Ok(url)
}

pub fn backup_file(original_path: &PathBuf) -> Result<PathBuf> {
  if !original_path.exists() {
    anyhow::bail!("No file to make a backup");
  }

  let mut backup_path = original_path.with_extension("sql.bak");
  let mut counter = 1;

  while backup_path.exists() {
    let new_name = format!("state.sql.bak.{}", counter);
    backup_path = original_path.with_file_name(new_name);
    counter += 1;
  }

  std::fs::rename(original_path, &backup_path)?;

  Ok(backup_path)
}
