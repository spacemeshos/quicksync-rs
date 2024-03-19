use anyhow::Result;
use reqwest::blocking::{Client, Response};
use std::{
  fs::File,
  io::{BufRead, BufReader},
  path::Path,
};
use url::Url;

use crate::{
  read_error_response::read_error_response, user_agent::APP_USER_AGENT,
  utils::strip_trailing_newline,
};

fn get_link_to_db_md5(url: &Url) -> Result<Url> {
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

fn get_link_to_archive_md5(url: &Url) -> Result<Url> {
  let url_str = url.as_str();
  let mut md5_url = url_str.to_owned();
  let md5_ext = ".md5";
  md5_url.push_str(md5_ext);
  Ok(Url::parse(&md5_url)?)
}

pub fn download_checksum(url: Url) -> Result<String> {
  let client = Client::builder()
    .user_agent(APP_USER_AGENT)
    .timeout(std::time::Duration::from_secs(30))
    .build()?;
  let response: Response = client.get(url.clone()).send()?;

  let status = response.status();
  if status.is_success() {
    let md5 = response.text()?;
    let stripped = strip_trailing_newline(&md5);
    Ok(stripped.to_string())
  } else {
    let err = read_error_response(response.text()?);
    anyhow::bail!(format!(
      "Cannot download MD5 checksum from {}: {} {}",
      url, status, err
    ));
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

pub fn verify_archive(redirect_file_path: &Path, archive_path: &Path) -> Result<bool> {
  let archive_url_str = String::from_utf8(std::fs::read(redirect_file_path)?)?;
  let archive_url = Url::parse(&archive_url_str)?;
  let md5_url = get_link_to_archive_md5(&archive_url)?;

  let md5_expected = download_checksum(md5_url)?;
  let md5_actual = calculate_checksum(archive_path)?;

  Ok(md5_actual == md5_expected)
}

pub fn verify_db(redirect_file_path: &Path, unpacked_file_path: &Path) -> Result<bool> {
  let archive_url_str = String::from_utf8(std::fs::read(redirect_file_path)?)?;
  let archive_url = Url::parse(&archive_url_str)?;
  let md5_url = get_link_to_db_md5(&archive_url)?;

  let md5_expected = download_checksum(md5_url)?;
  let md5_actual = calculate_checksum(unpacked_file_path)?;

  Ok(md5_actual == md5_expected)
}
