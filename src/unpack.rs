use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use zstd::stream::read::Decoder;

use crate::reader_with_bytes::ReaderWithBytes;

pub(crate) fn unpack(archive_path: &Path, outpath: &Path) -> Result<()> {
  let file = File::open(archive_path).context(format!(
    "Failed to open archive at path: {:?}",
    archive_path
  ))?;
  let reader = BufReader::new(file);
  let mut decoder = Decoder::new(reader)?;

  decoder.window_log_max(31)?;
  if let Some(p) = outpath.parent() {
    std::fs::create_dir_all(p).with_context(|| format!("creating directory: {}", p.display()))?;
  }
  let outfile = File::create(outpath)
    .with_context(|| format!("creating file to unpack into at: {}", outpath.display()))?;
  let mut writer = BufWriter::new(outfile);

  let mut reader = ReaderWithBytes::new(decoder);

  std::io::copy(&mut reader, &mut writer)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::fs::File;
  use std::io::{Read, Write};

  use super::unpack;

  #[test]
  fn unpack_zst() {
    let tempdir = tempfile::tempdir().unwrap();
    let archive_path = tempdir.path().join("database.zst");
    let archive = File::create(&archive_path).unwrap();

    let mut encoder = zstd::stream::write::Encoder::new(archive, 0).unwrap();
    encoder.write_all(b"Hello, World!\n").unwrap();
    encoder.finish().unwrap();

    // unpack the archive
    let output_filepath = tempdir.path().join("state.sql");
    unpack(&archive_path, &output_filepath).unwrap();

    // check the output
    let mut output_file = File::open(&output_filepath).unwrap();
    let mut output = String::new();
    output_file.read_to_string(&mut output).unwrap();
    assert_eq!(output, "Hello, World!\n");
  }
}
