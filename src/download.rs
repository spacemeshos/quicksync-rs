use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;

use crate::eta::Eta;
use crate::read_error_response::read_error_response;
use crate::user_agent::APP_USER_AGENT;

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
    .user_agent(APP_USER_AGENT)
    .timeout(std::time::Duration::from_secs(30))
    .build()?;
  let mut response = client
    .get(url)
    .header("Range", format!("bytes={}-", file_size))
    .send()?;

  let final_url = response.url().clone();

  fs::write(redirect_path, final_url.as_str())?;

  let status = response.status();
  if !status.is_success() {
    let err = read_error_response(response.text()?);
    anyhow::bail!(format!(
      "Failed to download from {}: {} {}",
      final_url, status, err
    ));
  }

  let content_len = response
    .headers()
    .get(reqwest::header::CONTENT_LENGTH)
    .and_then(|ct_len| ct_len.to_str().ok())
    .and_then(|ct_len| ct_len.parse::<u64>().ok())
    .unwrap_or(0);

  let total_size = content_len + file_size;

  file.seek(SeekFrom::End(0))?;

  const MEASUREMENT_SIZE: usize = 500;

  let mut last_reported_progress: Option<f64> = None;
  let start = Instant::now();
  let mut measurements = VecDeque::with_capacity(MEASUREMENT_SIZE);
  let mut just_downloaded: u64 = 0;

  let mut buffer = [0; 16 * 1024];
  loop {
    match response.read(&mut buffer) {
      Ok(0) => {
        break;
      }
      Ok(bytes_read) => {
        file.write_all(&buffer[..bytes_read])?;
        just_downloaded += bytes_read as u64;
        let downloaded = file_size + just_downloaded;

        let elapsed = start.elapsed().as_secs_f64();
        let speed = if elapsed > 0.0 {
          just_downloaded as f64 / elapsed
        } else {
          0.0
        };
        measurements.push_back(speed);
        if measurements.len() > MEASUREMENT_SIZE {
          measurements.pop_front();
        }
        let avg_speed = measurements.iter().sum::<f64>() / measurements.len() as f64;
        let eta = if avg_speed > 1.0 && measurements.len() > (MEASUREMENT_SIZE / 2) {
          Eta::Seconds((total_size as f64 - downloaded as f64) / avg_speed)
        } else {
          Eta::Unknown
        };

        let progress = downloaded as f64 / total_size as f64;
        if last_reported_progress.is_none()
          || last_reported_progress.is_some_and(|x| progress > x + 0.001)
        {
          println!(
            "Downloading... {:.2}% ({:.2} MB/{:.2} MB) ETA: {}",
            progress * 100.0,
            downloaded as f64 / 1_024_000.00,
            total_size as f64 / 1_024_000.00,
            eta
          );
          last_reported_progress = Some(progress);
        }
      }
      Err(e) => {
        return Err(anyhow!(e));
      }
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
        println!(
          "Download error: {}. Attempt {} / {}",
          e,
          attempts + 1,
          max_retries
        );
        attempts += 1;
        std::thread::sleep(std::time::Duration::from_secs(5));
      }
      Err(e) => return Err(anyhow!(e)),
    }
  }
}
