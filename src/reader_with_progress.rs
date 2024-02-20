use std::io::Read;

pub struct ReaderWithProgress<R: Read> {
  reader: R,
  total: u64,
  extracted: u64,
  last_reported_progress: u64,
}

impl<R: Read> ReaderWithProgress<R> {
  pub fn new(reader: R, total_size: u64) -> ReaderWithProgress<R> {
    ReaderWithProgress {
      reader,
      total: total_size,
      extracted: 0,
      last_reported_progress: 0,
    }
  }
}

impl<R: Read> Read for ReaderWithProgress<R> {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    let bytes_read = self.reader.read(buf)?;
    self.extracted += bytes_read as u64;

    let progress = (self.extracted as f64 / self.total as f64 * 100.0).round() as u64;
    if self.last_reported_progress != progress {
      self.last_reported_progress = progress;
      println!("Unzipping... {}%", progress);
    }

    Ok(bytes_read)
  }
}
