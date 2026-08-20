use schemars::JsonSchema;
use serde::Deserialize;

use waki::Client;

use common::define_tool;

#[derive(Deserialize, JsonSchema)]
struct ClickhouseClientParams {
  /// The exact `host` of the target data store, copied verbatim from the AVAILABLE DATA STORES section. It must point at the ClickHouse HTTP interface and start with an explicit http:// or https:// scheme, e.g. http://clickhouse:8123.
  host: String,
  /// The exact `username` of the target data store, copied verbatim from the AVAILABLE DATA STORES section.
  username: String,
  /// The exact `password` of the target data store, copied verbatim from the AVAILABLE DATA STORES section. Pass an empty string if the store lists no password.
  password: String,
  /// A single SQL statement to execute (SELECT, INSERT, CREATE TABLE, ...). The result of a SELECT is returned as CSV: a header row of column names followed by one row per record. Do not add a FORMAT clause; the tool already requests CSV.
  query: String
}

fn run(input: ClickhouseClientParams) -> ToolOutput {
  println!("[clickhouse_client] Enabling tool");

  let endpoint = format!("{}/", input.host.trim_end_matches('/'));

  println!("[clickhouse_client] Endpoint: {}", endpoint);

  let query_params: Vec<(&str, &str)> = vec![("default_format", "CSVWithNames")];

  let headers: Vec<(&str, &str)> = vec![
    ("X-ClickHouse-User", input.username.as_str()),
    ("X-ClickHouse-Key", input.password.as_str())
  ];

  let response = match Client::new().post(endpoint.as_str()).query(query_params).headers(headers).body(input.query.into_bytes()).send() {
    Ok(response) => response,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Request to clickhouse failed: {}", error)
      };
    }
  };

  let status = response.status_code();

  println!("[clickhouse_client] Response status: {}", status);

  let bytes = match response.body() {
    Ok(bytes) => bytes,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Could not read the clickhouse response: {}", error)
      };
    }
  };

  let content = String::from_utf8_lossy(&bytes).into_owned();

  if status < 200 || status >= 300 {
    return ToolOutput {
      state: ToolOutputState::Error,
      content: format!("Clickhouse returned status {}: {}", status, content)
    };
  }

  let content = match content.is_empty() {
    true => "No Data!".to_string(),
    false => format!("DATA AS CSV:\n{}", content)
  };

  return ToolOutput {
    state: ToolOutputState::Done,
    content
  };
}

define_tool!(
  ClickhouseClient,
  ClickhouseClientParams,
  "Executes a single SQL statement against a ClickHouse data store over its HTTP interface and returns the result of a SELECT as CSV (a header row of column names followed by one row per record). Pass the exact host, username and password from the AVAILABLE DATA STORES section plus the SQL to run. Use it to read from or write to a store (SELECT, INSERT, CREATE TABLE, ...). ClickHouse is column-oriented and optimised for large-scale analytical queries over immutable data; prefer it for aggregations and reporting rather than row-level updates or deletes.",
  vec![Permission::Network],
  run
);
