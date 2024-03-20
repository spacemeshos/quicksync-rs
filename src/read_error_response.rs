use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct ErrorResponse {
  msg: String,
}

pub fn read_error_response(body: String) -> String {
  match serde_json::from_str::<ErrorResponse>(body.as_str()) {
    Ok(j) => j.msg,
    Err(_) => String::from("Unknown error"),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_returns_expected_message() {
    let body = String::from("{ \"msg\": \"Expected error message\" }");
    assert_eq!(read_error_response(body), "Expected error message");
  }

  #[test]
  fn test_returns_unknown_error_on_failure() {
    let body = String::from("<html></html>");
    assert_eq!(read_error_response(body), "Unknown error");
  }
}
