use std::io::{self, Read};

const MB: u64 = 1024 * 1024;

pub struct ReaderWithBytes<R: Read> {
  reader: R,
  bytes_read: u64,
  last_reported: u64,
}

impl<R: Read> ReaderWithBytes<R> {
  pub fn new(reader: R) -> Self {
    ReaderWithBytes {
      reader,
      bytes_read: 0,
      last_reported: 0,
    }
  }
}

impl<R: Read> Read for ReaderWithBytes<R> {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    let bytes_read = self.reader.read(buf)?;
    self.bytes_read += bytes_read as u64;

    if self.bytes_read / MB > self.last_reported / MB {
      println!("Unpacking... {} MB extracted", self.bytes_read / MB);
      self.last_reported = self.bytes_read;
    }

    Ok(bytes_read)
  }
}
