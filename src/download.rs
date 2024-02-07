use anyhow::Result;
use reqwest::blocking::Client;
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;

pub fn download_file(url: &str, file_path: &Path, redirect_path: &Path) -> Result<()> {
  if let Some(dir) = file_path.parent() {
    fs::create_dir_all(dir)?;
  }

  let mut file = OpenOptions::new()
    .create(true)
    .read(true)
    .write(true)
    .open(file_path)?;

  let file_size = file.metadata()?.len();

  let client = Client::builder()
    .timeout(std::time::Duration::from_secs(30))
    .build()?;
  let mut response = client
    .get(url)
    .header("Range", format!("bytes={}-", file_size))
    .send()?;

  let final_url = response.url().clone();

  fs::write(redirect_path, final_url.as_str())?;

  if !response.status().is_success() {
    fs::remove_file(redirect_path)?;
    fs::remove_file(file_path)?;

    anyhow::bail!(
      "Failed to download: Response status code is {:?}",
      response.status()
    );
  }

  let total_size = response
    .headers()
    .get(reqwest::header::CONTENT_LENGTH)
    .and_then(|ct_len| ct_len.to_str().ok())
    .and_then(|ct_len| ct_len.parse::<u64>().ok())
    .unwrap_or(0)
    + file_size;

  file.seek(SeekFrom::End(0))?;
  let mut downloaded: u64 = file_size;
  let mut last_reported_progress: i64 = -1;
  let start = Instant::now();
  let mut measurements = VecDeque::with_capacity(10);

  let mut buffer = [0; 16 * 1024];
  while let Ok(bytes_read) = response.read(&mut buffer) {
    if bytes_read == 0 {
      break;
    }
    file.write_all(&buffer[..bytes_read])?;
    downloaded += bytes_read as u64;

    let elapsed = start.elapsed().as_secs_f64();
    let speed = if elapsed > 0.0 {
      downloaded as f64 / elapsed
    } else {
      0.0
    };
    measurements.push_back(speed);
    if measurements.len() > 10 {
      measurements.pop_front();
    }
    let avg_speed = measurements.iter().sum::<f64>() / measurements.len() as f64;
    let eta = if avg_speed > 0.0 {
      (total_size as f64 - downloaded as f64) / avg_speed
    } else {
      0.0
    };

    let progress = (downloaded as f64 / total_size as f64 * 100.0).round() as i64;
    if progress > last_reported_progress {
      println!(
        "Downloading... {:.2}% ({:.2} MB/{:.2} MB) ETA: {:.0} sec",
        progress,
        downloaded as f64 / 1_024_000.00,
        total_size as f64 / 1_024_000.00,
        eta
      );
      last_reported_progress = progress;
    }
  }

  println!("Download finished");

  Ok(())
}

pub fn download_with_retries(
  url: &str,
  file_path: &Path,
  redirect_path: &Path,
  max_retries: u32,
) -> Result<()> {
  let mut attempts = 0;

  loop {
    match download_file(url, file_path, redirect_path) {
      Ok(()) => return Ok(()),
      Err(e) if attempts < max_retries => {
        eprintln!(
          "Download error: {}. Attempt {} / {}",
          e,
          attempts + 1,
          max_retries
        );
        attempts += 1;
        std::thread::sleep(std::time::Duration::from_secs(5));
      }
      Err(e) => return Err(e),
    }
  }
}
