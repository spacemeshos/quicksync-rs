use reqwest::{Client, header};
use tokio::time::sleep;
use std::error::Error;
use std::path::Path;
use std::time::Instant;
use std::fs::{OpenOptions, create_dir_all};
use std::io::{Seek, SeekFrom, Write};
use futures_util::StreamExt;

pub async fn download_file(url: &str, file_path: &str, redirect_path: &str) -> Result<(), Box<dyn Error>> {
  let path = Path::new(file_path);

  if let Some(dir) = path.parent() {
      create_dir_all(dir).expect("Cannot create directory");
  }

  let mut file = OpenOptions::new()
      .create(true)
      .read(true)
      .write(true)
      .open(file_path)
      .expect("Cannot create file");

  let file_size = file.metadata()?.len();

  let client = Client::new();
  let response = client.get(url)
      .header("Range", format!("bytes={}-", file_size))
      .send()
      .await?;
  let final_url = response.url().clone();

  std::fs::write(redirect_path, final_url.as_str())?;

  if response.status().is_success() {
      let total_size = response
          .headers()
          .get(header::CONTENT_LENGTH)
          .and_then(|ct_len| ct_len.to_str().ok())
          .and_then(|ct_len| ct_len.parse::<u64>().ok())
          .unwrap_or(0) + file_size;

      file.seek(SeekFrom::End(0))?;
      let mut stream = response.bytes_stream();
      let start = Instant::now();

      let mut downloaded: u64 = file_size;
      let mut last_reported_progress: i64 = -1;

      while let Some(item) = stream.next().await {
          let chunk = item?;
          file.write_all(&chunk)?;
          downloaded += chunk.len() as u64;

          let elapsed = start.elapsed().as_secs_f64();
          let speed = if elapsed > 0.0 {
              downloaded as f64 / elapsed
          } else {
              0.0
          };
          let eta = if speed > 0.0 {
              (total_size as f64 - downloaded as f64) / speed
          } else {
              0.0
          };

          let progress = (downloaded as f64 / total_size as f64 * 100.0).round() as i64;
          if progress > last_reported_progress {
              println!("Downloading... {:.2}% ({:.2} MB/{:.2} MB) ETA: {:.0} sec",
                  progress,
                  downloaded as f64 / 1_024_000.0,
                  total_size as f64 / 1_024_000.0,
                  eta);
              last_reported_progress = progress;
          }
      }
      Ok(())
  } else {
      let err_message = format!("Cannot download: {:?}", response.status());

      std::fs::remove_file(redirect_path)?;
      std::fs::remove_file(file_path)?;
      Err(
          Box::new(std::io::Error::new(
              std::io::ErrorKind::NotFound,
              err_message
          ))
      )
  }
}

pub async fn download_with_retries(url: &str, file_path: &str, redirect_path: &str, max_retries: u32) -> Result<(), Box<dyn std::error::Error>> {
  let mut attempts = 0;

  loop {
      match download_file(url, file_path, redirect_path).await {
          Ok(()) => return Ok(()),
          Err(e) if attempts < max_retries => {
              eprintln!("Download error: {}. Attemmpt {} / {}", e, attempts + 1, max_retries);
              attempts += 1;
              sleep(std::time::Duration::from_secs(5)).await;
          }
          Err(e) => return Err(e),
      }
  }
}