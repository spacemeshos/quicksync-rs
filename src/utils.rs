use chrono::{DateTime, Utc, Duration};
use duration_string::DurationString;
use url::{Url, ParseError};
use std::{error::Error, path::PathBuf, env};

pub fn parse_iso_date(iso_date: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
  iso_date.parse::<DateTime<Utc>>()
}

pub fn strip_trailing_newline(input: &str) -> &str {
  input
      .strip_suffix("\r\n")
      .or(input.strip_suffix("\n"))
      .unwrap_or(input)
}

pub fn calculate_latest_layer(genesis_time: String, layer_duration: String) -> Result<i64, Box<dyn Error>> {
  let genesis = parse_iso_date(&genesis_time)?;
  let delta = Utc::now() - genesis;
  let dur = Duration::from_std(DurationString::from_string(layer_duration)?.into())?;
  Ok(delta.num_milliseconds() / dur.num_milliseconds())
}

pub fn resolve_path(relative_path: &str) -> Result<PathBuf, Box<dyn Error>> {
  let current_dir = env::current_dir()?;
  let resolved_path = current_dir.join(relative_path);
  Ok(resolved_path)
}

pub fn trim_version(version: &str) -> &str {
  version.split('+').next().unwrap_or(version)
}

pub fn build_url(base: &str, path: &str) -> Result<Url, ParseError> {
  let mut url = Url::parse(base)?;
  url.path_segments_mut().expect("cannot be base").extend(path.split('/'));
  Ok(url)
}