use std::fs::File;
use std::error::Error;
use std::io::{Write, Read};
use std::path::Path;
use zip::ZipArchive;

pub async fn unpack(archive_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
  let file = File::open(archive_path)?;
  let mut zip = ZipArchive::new(file)?;

  let mut state_sql = zip.by_name("state.sql")
      .expect("State.sql file not found in archive");
  let outpath = Path::new(output_path);

  if let Some(p) = outpath.parent() {
      std::fs::create_dir_all(&p)?;
  }
  let mut outfile = File::create(&outpath)?;

  let total_size = state_sql.size();
  let mut extracted_size: u64 = 0;
  let mut buffer = [0; 4096];

  let mut last_reported_progress: i64 = -1;

  while let Ok(bytes_read) = state_sql.read(&mut buffer) {
      if bytes_read == 0 {
          break;
      }
      outfile.write_all(&buffer[..bytes_read])?;
      extracted_size += bytes_read as u64;

      let progress = (extracted_size as f64 / total_size as f64 * 100.0).round() as i64;
      if last_reported_progress != progress {
          last_reported_progress = progress;
          println!("Unzipping... {}%", progress);
      }
  }
  if last_reported_progress < 100 {
      // Ensure that 100% will be printed
      println!("Unzipping... 100%");
  }

  Ok(())
}