use anyhow::Result;
use std::{io::ErrorKind, process::Command};

use crate::utils::trim_version;

pub fn get_version(path: &str) -> Result<String> {
  let output = Command::new(path)
    .arg("version")
    .output()
    .map_err(|error| match error.kind() {
      ErrorKind::NotFound => {
        anyhow::anyhow!("Executable not found at path: {}", path)
      }
      other_error => panic!("Unexpected error: {:?}", other_error),
    })?;

  let version = String::from_utf8(output.stdout)?;
  let trimmed = trim_version(version.trim()).to_string();

  Ok(trimmed)
}
