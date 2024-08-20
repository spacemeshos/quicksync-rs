use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use zip::read::ZipFile;
use zip::ZipArchive;
use zstd::stream::read::Decoder;

use crate::reader_with_bytes::ReaderWithBytes;
use crate::reader_with_progress::ReaderWithProgress;

const DB_FILENAME: &str = "state.sql";

fn find_file_in_archive<'a>(
  archive: &'a mut ZipArchive<File>,
  file_name: &str,
) -> Result<ZipFile<'a>> {
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

  Err(anyhow::anyhow!("File '{file_name}' not found in archive"))
}

fn unpack_zstd(archive_path: &Path, outpath: &Path) -> Result<()> {
  let file = File::open(archive_path).context(format!(
    "Failed to open archive at path: {:?}",
    archive_path
  ))?;
  let reader = BufReader::new(file);
  let mut decoder = Decoder::new(reader)?;

  decoder.window_log_max(31)?;
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

fn unpack_zip(archive_path: &Path, outpath: &Path) -> Result<()> {
  let file = File::open(archive_path)?;
  let mut zip = ZipArchive::new(file)?;

  let state_sql: ZipFile = find_file_in_archive(&mut zip, DB_FILENAME)?;

  if let Some(p) = outpath.parent() {
    std::fs::create_dir_all(p)?;
  }
  let mut outfile = File::create(outpath)?;

  let total_size = state_sql.size();
  let mut reader =
    ReaderWithProgress::new(BufReader::with_capacity(1024 * 1024, state_sql), total_size);

  std::io::copy(&mut reader, &mut outfile)?;

  Ok(())
}

pub(crate) fn unpack(archive_path: &Path, output_path: &Path) -> Result<()> {
  match archive_path.extension() {
    Some(ext) if ext == "zst" => unpack_zstd(archive_path, output_path),
    Some(ext) if ext == "zip" => unpack_zip(archive_path, output_path),
    _ => Err(anyhow::anyhow!("Unsupported archive format")),
  }
}

#[cfg(test)]
mod tests {
  use std::fs::File;
  use std::io::{Read, Write};

  use zip::write::SimpleFileOptions;

  use super::{unpack, DB_FILENAME};

  fn test_unpack(ext: &str) {
    let tempdir = tempfile::tempdir().unwrap();
    let archive_path = tempdir.path().join(format!("database.{}", ext));
    let archive = File::create(&archive_path).unwrap();

    match ext {
      "zip" => {
        let mut zip = zip::ZipWriter::new(&archive);
        let options = SimpleFileOptions::default();
        zip.start_file(DB_FILENAME, options).unwrap();
        zip.write_all(b"Hello, World!\n").unwrap();
        zip.finish().unwrap();
      }
      "zst" => {
        let mut encoder = zstd::stream::write::Encoder::new(archive, 0).unwrap();
        encoder.write_all(b"Hello, World!\n").unwrap();
        encoder.finish().unwrap();
      }
      _ => panic!("Unsupported archive format"),
    }

    // unpack the archive
    let output_filepath = tempdir.path().join(DB_FILENAME);
    unpack(&archive_path, &output_filepath).unwrap();

    // check the output
    let mut output_file = File::open(&output_filepath).unwrap();
    let mut output = String::new();
    output_file.read_to_string(&mut output).unwrap();
    assert_eq!(output, "Hello, World!\n");
  }

  #[test]
  fn unpack_zst() {
    test_unpack("zst");
  }

  #[test]
  fn unpack_zip() {
    test_unpack("zip");
  }
}
