use reqwest::Client;
use std::{
  fs::File,
  io::{self, BufReader},
  io::{BufRead, Error},
  path::Path,
};
use url::Url;

use crate::utils::strip_trailing_newline;

pub async fn download_checksum(url: &Url) -> Result<String, Error> {
  let mut u = url.clone();
  u.path_segments_mut()
    .expect("Wrong URL")
    .pop()
    .push("state.sql.md5");
  let md5_url = u.to_string();

  let client = Client::new();
  let response = client
    .get(md5_url)
    .send()
    .await
    .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string()))?;

  if response.status().is_success() {
    let md5 = response
      .text()
      .await
      .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    let stripped = strip_trailing_newline(&md5);
    Ok(stripped.to_string())
  } else {
    Err(std::io::Error::new(
      std::io::ErrorKind::NotFound,
      "Cannot download MD5 checksum",
    ))
  }
}

pub fn calculate_checksum(file_path: &Path) -> io::Result<String> {
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

pub async fn verify(
  redirect_file_path: &Path,
  unpacked_file_path: &Path,
) -> Result<bool, std::io::Error> {
  let archive_url_str = String::from_utf8(std::fs::read(redirect_file_path)?)
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

  let archive_url = Url::parse(&archive_url_str)
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

  let md5_expected = download_checksum(&archive_url).await?;
  let md5_actual = calculate_checksum(unpacked_file_path)?;

  Ok(md5_actual == md5_expected)
}
