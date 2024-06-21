use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use reqwest::{blocking::Client, redirect};
use std::{env, path::PathBuf};
use url::Url;

use crate::user_agent::APP_USER_AGENT;

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

pub fn build_url(base: &Url, path: &str) -> Url {
  let mut url = base.clone();
  url
    .path_segments_mut()
    .expect("cannot be base")
    .extend(path.split('/'));
  url
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

fn extract_number_from_url(url: &Url) -> Result<u64> {
  let re = Regex::new(r"/(\d+)\.sql\.(zip|zst)$")?;
  let path = url.path();
  let caps = re
    .captures(path)
    .ok_or_else(|| anyhow!("No numeric value found in URL: {}", url))?;

  let number_str = caps
    .get(1)
    .ok_or_else(|| anyhow!("No numeric value captured"))?
    .as_str();
  let number = number_str.parse::<u64>()?;

  Ok(number)
}

pub fn fetch_latest_available_layer(download_url: &Url, go_version: &str) -> Result<u64> {
  let client = Client::builder()
    .user_agent(APP_USER_AGENT)
    .redirect(redirect::Policy::none())
    .timeout(std::time::Duration::from_secs(30))
    .build()?;

  let path = format!("{}/state.zst", go_version);
  let url = build_url(&download_url, &path);

  let response = client.head(url).send()?;

  let location = response.headers().get("location").unwrap().to_str()?;
  let final_url = Url::parse(location)?;
  let num = extract_number_from_url(&final_url)?;

  Ok(num)
}

pub fn to_precision(number: f64, precision: u8) -> f64 {
  let pow = u32::pow(10, precision as u32);
  let mult: f64 = f64::from(pow);
  if mult > 1.0 {
    return (number * mult).round() / mult;
  } else {
    return number;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use url::Url;

  #[test]
  fn test_extract_number_zip_valid() {
    let url = Url::parse("https://quicksync-downloads.spacemesh.network/10/61579.sql.zip").unwrap();
    assert_eq!(extract_number_from_url(&url).unwrap(), 61579);
  }

  #[test]
  fn test_extract_number_zstd_valid() {
    let url = Url::parse("https://quicksync-downloads.spacemesh.network/10/61579.sql.zst").unwrap();
    assert_eq!(extract_number_from_url(&url).unwrap(), 61579);
  }

  #[test]
  fn test_extract_number_invalid() {
    let url = Url::parse("https://quicksync.spacemesh.network/state.zip").unwrap();
    assert!(extract_number_from_url(&url).is_err());
  }

  #[test]
  fn test_to_precision() {
    assert_eq!(to_precision(23.57742, 3), 23.577);
    assert_eq!(to_precision(23.57742, 2), 23.58);
    assert_eq!(to_precision(55555.0, 3), 55555.0);
    assert_eq!(to_precision(55555.0, 0), 55555.0);
    assert_eq!(to_precision(123.456789, 5), 123.45679);
  }
}
