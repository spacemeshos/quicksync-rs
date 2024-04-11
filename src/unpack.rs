use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufReader, BufWriter, Error};
use std::path::Path;
use zip::read::ZipFile;
use zip::ZipArchive;
use zstd::stream::read::Decoder;

use crate::reader_with_bytes::ReaderWithBytes;
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

pub fn unpack_zstd(archive_path: &Path, output_path: &Path) -> Result<()> {
  let file = File::open(archive_path).context(format!(
    "Failed to open archive at path: {:?}",
    archive_path
  ))?;
  let reader = BufReader::new(file);
  let mut decoder = Decoder::new(reader)?;

  decoder.window_log_max(31)?;
  let outpath = Path::new(output_path);
  if let Some(p) = outpath.parent() {
    std::fs::create_dir_all(p).context(format!("Failed to create directory at path: {:?}", p))?;
  }
  let outfile = File::create(outpath).context(format!(
    "Failed to create output file at path: {:?}",
    outpath
  ))?;
  let mut writer = BufWriter::new(outfile);

  let mut reader = ReaderWithBytes::new(decoder);

  std::io::copy(&mut reader, &mut writer)?;
  println!("Unpacking complete!");

  Ok(())
}

pub fn unpack_zip(archive_path: &Path, output_path: &Path) -> Result<()> {
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
