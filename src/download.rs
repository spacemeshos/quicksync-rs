use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;

use crate::eta::Eta;
use crate::read_error_response::read_error_response;
use crate::user_agent::APP_USER_AGENT;

fn download_file<W: Write + Seek>(url: &str, file: &mut W, redirect_path: &Path) -> Result<()> {
  let offset = file.seek(SeekFrom::End(0))?;

  let url = if redirect_path.try_exists().unwrap_or(false) {
    std::fs::read_to_string(redirect_path)?
  } else {
    url.to_string()
  };

  let client = Client::builder()
    .user_agent(APP_USER_AGENT)
    .timeout(std::time::Duration::from_secs(30))
    .build()?;
  let mut response = client
    .get(&url)
    .header("Range", format!("bytes={offset}-"))
    .send()?;

  let code = response.status();
  match code {
    StatusCode::PARTIAL_CONTENT => {}
    _ if code.is_success() => {
      anyhow::bail!("expected {}, but got {}", StatusCode::PARTIAL_CONTENT, code);
    }
    _ => {
      let err = read_error_response(response.text()?);
      anyhow::bail!("failed to download from {url}: {code} {err}");
    }
  }
  let final_url = response.url().clone();

  std::fs::write(redirect_path, final_url.as_str())?;

  let content_len = response
    .headers()
    .get(reqwest::header::CONTENT_LENGTH)
    .and_then(|ct_len| ct_len.to_str().ok())
    .and_then(|ct_len| ct_len.parse::<u64>().ok())
    .unwrap_or(0);

  let total_size = content_len + offset;

  const MEASUREMENT_SIZE: usize = 500;

  let mut last_reported_progress: Option<f64> = None;
  let start = Instant::now();
  let mut measurements = VecDeque::with_capacity(MEASUREMENT_SIZE);
  let mut just_downloaded = 0;

  let mut buffer = [0; 16 * 1024];
  loop {
    match response.read(&mut buffer) {
      Ok(0) => {
        break;
      }
      Ok(bytes_read) => {
        file.write_all(&buffer[..bytes_read])?;
        just_downloaded += bytes_read as u64;
        let downloaded = offset + just_downloaded;

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

pub(crate) fn download_with_retries<W: Write + Seek>(
  url: &str,
  file: &mut W,
  redirect_path: &Path,
  max_retries: u32,
) -> Result<()> {
  let mut attempts = 0;

  loop {
    attempts += 1;
    match download_file(url, file, redirect_path) {
      Ok(()) => return Ok(()),
      Err(e) if attempts <= max_retries => {
        println!("Download error: {e}. Attempt {attempts} / {max_retries}",);
        std::thread::sleep(std::time::Duration::from_secs(5));
      }
      Err(e) => return Err(anyhow!(e)),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    cmp::min,
    fs,
    io::{Error, ErrorKind, Read, Seek},
    iter,
  };

  use rand::{Rng, SeedableRng};

  #[test]
  fn rejects_not_206() {
    let mut server = mockito::Server::new();
    let mock = server.mock("GET", "/").with_status(200).create();

    let tmpdir = tempfile::tempdir().unwrap();
    let redirect_path = tmpdir.path().join("redirect.txt");
    let mut file = tempfile::tempfile().unwrap();

    let result = super::download_file(&server.url(), &mut file, &redirect_path);
    let err = result.unwrap_err();
    assert_eq!(
      err.to_string(),
      "expected 206 Partial Content, but got 200 OK"
    );

    mock.assert();
  }

  #[test]
  fn fails_when_server_fails() {
    let mut server = mockito::Server::new();
    let mock = server.mock("GET", "/").with_status(500).create();

    let tmpdir = tempfile::tempdir().unwrap();
    let redirect_path = tmpdir.path().join("redirect.txt");
    let mut file = tempfile::tempfile().unwrap();

    let result = super::download_file(&server.url(), &mut file, &redirect_path);
    let err = result.unwrap_err();
    assert!(err.to_string().contains("failed to download from"));

    mock.assert();
  }

  #[test]
  fn downloads_file() {
    let binary = b"1234567890";

    let mut server = mockito::Server::new();
    let mock = server
      .mock("GET", "/file")
      .with_status(206)
      .with_body(binary)
      .create();

    let tmpdir = tempfile::tempdir().unwrap();
    let mut file = tempfile::tempfile().unwrap();
    let redirect_path = tmpdir.path().join("redirect.txt");

    let url = server.url() + "/file";

    super::download_file(&url, &mut file, &redirect_path).unwrap();
    file.seek(std::io::SeekFrom::Start(0)).unwrap();
    let content = file.bytes().collect::<Result<Vec<u8>, _>>().unwrap();
    assert_eq!(content, binary);

    let redirect_url = fs::read_to_string(redirect_path).unwrap();
    assert_eq!(redirect_url, url);

    mock.assert();
  }

  #[test]
  fn follows_redirect_and_persists_it() {
    let binary = b"1234567890";

    let mut server = mockito::Server::new();
    let redirected_url = server.url() + "/redirected";
    let mock_redirect = server
      .mock("GET", "/file")
      .with_status(301)
      .with_header("location", &redirected_url)
      .create();

    let mock = server
      .mock("GET", "/redirected")
      .with_status(206)
      .with_body(binary)
      .create();

    let tmpdir = tempfile::tempdir().unwrap();
    let mut file = tempfile::tempfile().unwrap();
    let redirect_path = tmpdir.path().join("redirect.txt");

    let url = server.url() + "/file";

    super::download_file(&url, &mut file, &redirect_path).unwrap();
    file.seek(std::io::SeekFrom::Start(0)).unwrap();
    let content = file.bytes().collect::<Result<Vec<u8>, _>>().unwrap();
    assert_eq!(content, binary);

    let redirect_url = fs::read_to_string(redirect_path).unwrap();
    assert_eq!(redirect_url, redirected_url);

    mock_redirect.assert();
    mock.assert();
  }

  #[test]
  fn retries_after_failure() {
    let mut server = mockito::Server::new();

    let mut rng = rand::rngs::StdRng::seed_from_u64(11);
    let binary: Vec<u8> = iter::repeat_with(|| rng.gen()).take(2_000).collect();
    let binary = std::sync::Arc::new(binary);
    let binary_clone = binary.clone();

    let mock_redirect = server
      .mock("GET", "/file")
      .with_status(301)
      .with_header("location", &(server.url() + "/redirected"))
      .create();

    let mock = server
      .mock("GET", "/redirected")
      .with_status(206)
      .with_body_from_request(move |req| {
        let range_hdr = req.header("Range").first().unwrap().to_str().unwrap();
        let range = range_hdr.split('=').nth(1).unwrap();
        let start = range.strip_suffix('-').unwrap().parse::<usize>().unwrap();

        binary_clone[start..].to_vec()
      })
      .create()
      .expect(2);

    let tmpdir = tempfile::tempdir().unwrap();
    let redirect_path = tmpdir.path().join("redirect.txt");

    // a mock file that fails once after writing the first bytes on the first attempt
    struct FileMock {
      bytes: Vec<u8>,
      failed: bool,
    }

    impl std::io::Write for FileMock {
      fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match (self.bytes.len(), self.failed) {
          (0, _) => {
            // write only the first 50 bytes
            let size = min(50, buf.len());
            self.bytes.extend_from_slice(&buf[..size]);
            Ok(size)
          }
          (1.., false) => {
            self.failed = true;
            Err(Error::new(ErrorKind::Other, "failed writing"))
          }
          _ => {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
          }
        }
      }

      fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
      }
    }

    impl std::io::Seek for FileMock {
      fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
        Ok(self.bytes.len() as u64)
      }
    }

    let mut file = FileMock {
      bytes: Vec::new(),
      failed: false,
    };

    let url = server.url() + "/file";
    super::download_with_retries(&url, &mut file, &redirect_path, 1).unwrap();

    mock_redirect.assert();
    mock.assert();

    assert_eq!(file.bytes, *binary);
  }
}
