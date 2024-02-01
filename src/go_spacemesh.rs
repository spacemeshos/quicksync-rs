use std::error::Error;
use std::process::Command;

use crate::utils::trim_version;

pub fn get_version(path: &str) -> Result<String, Box<dyn Error>> {
  let output = Command::new(path).arg("version").output()?;

  let version = String::from_utf8(output.stdout)?;
  let trimmed = trim_version(version.trim()).to_string();

  Ok(trimmed)
}
