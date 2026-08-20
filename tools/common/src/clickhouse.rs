use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use crate::http::HttpClient;
use crate::http::HttpError;

// A reusable ClickHouse client built on the shared HTTP client. It speaks the
// HTTP interface and returns a SELECT result as CSV (a header row followed by
// one row per record). Callers decide how to present an empty result.

#[derive(Debug)]
pub enum ClickhouseError {
  Http(HttpError),
  Server(u16, String)
}

impl Display for ClickhouseError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      ClickhouseError::Http(error) => write!(formatter, "{}", error),
      ClickhouseError::Server(status, body) => write!(formatter, "Clickhouse returned status {}: {}", status, body)
    };
  }
}

pub struct Clickhouse {
  host: String,
  username: String,
  password: String
}

impl Clickhouse {
  pub fn new(host: &str, username: &str, password: &str) -> Self {
    return Self {
      host: host.to_string(),
      username: username.to_string(),
      password: password.to_string()
    };
  }

  /// Runs a single SQL statement and returns the raw response body. A SELECT
  /// comes back as CSV (`default_format=CSVWithNames`); statements with no
  /// result set return an empty string.
  pub fn query(&self, sql: &str) -> Result<String, ClickhouseError> {
    let client = HttpClient::new(self.host.as_str());

    let headers: Vec<(&str, &str)> = vec![
      ("X-ClickHouse-User", self.username.as_str()),
      ("X-ClickHouse-Key", self.password.as_str())
    ];

    let query: Vec<(&str, &str)> = vec![("default_format", "CSVWithNames")];

    let response = match client.post_raw("/", headers, query, sql.as_bytes().to_vec()) {
      Ok(response) => response,
      Err(error) => {
        return Err(ClickhouseError::Http(error));
      }
    };

    if response.is_success() == false {
      return Err(ClickhouseError::Server(response.status, response.text()));
    }

    return Ok(response.text());
  }
}
