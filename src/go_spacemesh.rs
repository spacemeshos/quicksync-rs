use anyhow::Result;
use std::{io::ErrorKind, path::Path, process::Command};

pub fn get_version(path: &Path) -> Result<String> {
  let output = Command::new(path)
    .arg("version")
    .output()
    .map_err(|error| match error.kind() {
      ErrorKind::NotFound => {
        anyhow::anyhow!("executable not found at path: {}", path.display())
      }
      other_error => anyhow::anyhow!("unexpected error: {other_error}"),
    })?;

  let version = String::from_utf8(output.stdout)?;
  Ok(version.split('+').next().unwrap().to_string())
}
