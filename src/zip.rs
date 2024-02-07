use anyhow::Result;
use std::fs::File;
use std::io::{Error, Read, Write};
use std::path::Path;
use zip::ZipArchive;

pub fn unpack(archive_path: &Path, output_path: &Path) -> Result<()> {
  let file = File::open(archive_path)?;
  let mut zip = ZipArchive::new(file)?;

  let mut state_sql = zip
    .by_name("state.sql")
    .map_err(|e| Error::new(std::io::ErrorKind::NotFound, e.to_string()))?;
  let outpath = Path::new(output_path);

  if let Some(p) = outpath.parent() {
    std::fs::create_dir_all(p)?;
  }
  let mut outfile = File::create(outpath)?;

  let total_size = state_sql.size();
  let mut extracted_size: u64 = 0;
  let mut buffer = [0; 4096];

  let mut last_reported_progress: i64 = -1;

  loop {
    match state_sql.read(&mut buffer) {
      Ok(0) => {
        if last_reported_progress != 100 {
          last_reported_progress = 100;
          println!("Unzipping... {}%", last_reported_progress);
        }
        break;
      }
      Ok(bytes_read) => {
        outfile.write_all(&buffer[..bytes_read])?;
        extracted_size += bytes_read as u64;

        let progress = (extracted_size as f64 / total_size as f64 * 100.0).round() as i64;
        if last_reported_progress != progress {
          last_reported_progress = progress;
          println!("Unzipping... {}%", progress);
        }
      }
      Err(e) => anyhow::bail!(e),
    }
  }

  if last_reported_progress < 100 {
    anyhow::bail!("Archive was not fully unpacked");
  }

  Ok(())
}
