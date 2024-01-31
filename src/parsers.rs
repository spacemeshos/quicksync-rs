use std::io::{Error, ErrorKind};

pub fn parse_duration(v: &str) -> Result<chrono::Duration, Error> {
  let ds = v.parse::<duration_string::DurationString>().map_err(
    |e| Error::new(
      ErrorKind::InvalidInput,
      e.to_string()
    )
  )?;
  let res = chrono::Duration::from_std(ds.into())
    .map_err(|e| Error::new(
      ErrorKind::InvalidInput,
      e.to_string()
    ))?;
  
  Ok(res)
}
