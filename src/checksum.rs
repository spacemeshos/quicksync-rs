use reqwest::Client;
use url::Url;
use std::{error::Error, fs::File, io, io::Read, path::Path};

use crate::utils::strip_trailing_newline;

pub async fn download_checksum(url: &str) -> Result<String, Box<dyn Error>> {
  let mut u = Url::parse(url)?;
  u.path_segments_mut().expect("Wrong URL").pop().push("state.sql.md5");
  let md5_url = u.to_string();

  let client = Client::new();
  let response = client.get(md5_url)
      .send()
      .await?;

  if response.status().is_success() {
      let md5 = response.text().await?;
      let stripped = strip_trailing_newline(&md5);
      Ok(stripped.to_string())
  } else {
      Err(
          Box::new(std::io::Error::new(
              std::io::ErrorKind::NotFound,
              "Cannot download MD5 checksum"
          ))
      )
  }
}

pub fn calculate_checksum(file_path: &Path) -> io::Result<String> {
  let mut file = File::open(file_path)?;
  let mut buffer = Vec::new();
  file.read_to_end(&mut buffer)?;
  let hash = md5::compute(buffer);
  Ok(format!("{:x}", hash))
}