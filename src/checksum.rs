use anyhow::Result;
use reqwest::blocking::{Client, Response};
use std::{
  fs::File,
  io::{BufRead, BufReader},
  path::Path,
};
use url::Url;

use crate::utils::strip_trailing_newline;

fn replace_sql_zip_with_md5(url: &Url) -> Result<Url> {
  let url_str = url.as_str();
  if url_str.ends_with(".sql.zip") {
    let new_url_str = url_str.replace(".sql.zip", ".sql.md5");
    Ok(Url::parse(&new_url_str)?)
  } else if url_str.ends_with(".sql.zst") {
    let new_url_str = url_str.replace(".sql.zst", ".sql.md5");
    Ok(Url::parse(&new_url_str)?)
  } else {
    anyhow::bail!("URL does not end with .sql.zip")
  }
}

pub fn download_checksum(url: &Url) -> Result<String> {
  let md5_url = replace_sql_zip_with_md5(&url)?;

  let client = Client::new();
  let response: Response = client.get(md5_url).send()?;

  if response.status().is_success() {
    let md5 = response.text()?;
    let stripped = strip_trailing_newline(&md5);
    Ok(stripped.to_string())
  } else {
    anyhow::bail!(
      "Cannot download MD5 checksum: status code is {:?}",
      response.status()
    );
  }
}

pub fn calculate_checksum(file_path: &Path) -> Result<String> {
  let file = File::open(file_path)?;
  let mut reader = BufReader::with_capacity(16 * 1024 * 1024, file);
  let mut hasher = md5::Context::new();

  loop {
    let chunk = reader.fill_buf()?;
    if chunk.is_empty() {
      break;
    }
    hasher.consume(chunk);
    let chunk_len = chunk.len();
    reader.consume(chunk_len);
  }

  let hash = hasher.compute();
  Ok(format!("{:x}", hash))
}

pub fn verify(redirect_file_path: &Path, unpacked_file_path: &Path) -> Result<bool> {
  let archive_url_str = String::from_utf8(std::fs::read(redirect_file_path)?)?;

  let archive_url = Url::parse(&archive_url_str)?;

  let md5_expected = download_checksum(&archive_url)?;
  let md5_actual = calculate_checksum(unpacked_file_path)?;

  Ok(md5_actual == md5_expected)
}
