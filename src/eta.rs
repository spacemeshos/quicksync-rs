pub enum Eta {
  Unknown,
  Seconds(f64),
}

impl std::fmt::Display for Eta {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Eta::Unknown => write!(f, "unknown"),
      Eta::Seconds(s) => write!(f, "{s:.0} sec"),
    }
  }
}
