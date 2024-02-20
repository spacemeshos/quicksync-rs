use anyhow::Result;
use std::fs::File;
use std::io::{BufReader, Error};
use std::path::Path;
use zip::read::ZipFile;
use zip::ZipArchive;

use crate::reader_with_progress::ReaderWithProgress;

fn find_file_in_archive<'a>(
  archive: &'a mut ZipArchive<File>,
  file_name: &str,
) -> Result<ZipFile<'a>, Error> {
  let mut found_idx = None;
  for i in 0..archive.len() {
    let file = archive.by_index(i)?;
    if file.name().ends_with(file_name) {
      found_idx = Some(i);
      break;
    }
  }
  if let Some(idx) = found_idx {
    return Ok(archive.by_index(idx)?);
  }

  Err(Error::new(
    std::io::ErrorKind::NotFound,
    format!("File '{}' not found in archive", file_name),
  ))
}

pub fn unpack(archive_path: &Path, output_path: &Path) -> Result<()> {
  let file = File::open(archive_path)?;
  let mut zip = ZipArchive::new(file)?;

  let state_sql: ZipFile = find_file_in_archive(&mut zip, "state.sql")?;
  let outpath = Path::new(output_path);

  if let Some(p) = outpath.parent() {
    std::fs::create_dir_all(p)?;
  }
  let mut outfile = File::create(outpath)?;

  let total_size = state_sql.size();
  let mut reader =
    ReaderWithProgress::new(BufReader::with_capacity(1024 * 1024, state_sql), total_size);

  std::io::copy(&mut reader, &mut outfile)?;
  println!("Unzipping... 100%");

  Ok(())
}
