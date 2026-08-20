use schemars::JsonSchema;
use serde::Deserialize;

use common::define_tool;
use common::clickhouse::Clickhouse;

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

  let client = Clickhouse::new(&input.host, &input.username, &input.password);

  return match client.query(&input.query) {
    Ok(content) => {
      let content = match content.is_empty() {
        true => "No Data!".to_string(),
        false => format!("DATA AS CSV:\n{}", content)
      };

      ToolOutput {
        state: ToolOutputState::Done,
        content
      }
    },
    Err(error) => ToolOutput {
      state: ToolOutputState::Error,
      content: error.to_string()
    }
  };
}

define_tool!(
  ClickhouseClient,
  ClickhouseClientParams,
  "Executes a single SQL statement against a ClickHouse data store over its HTTP interface and returns the result of a SELECT as CSV (a header row of column names followed by one row per record). Pass the exact host, username and password from the AVAILABLE DATA STORES section plus the SQL to run. Use it to read from or write to a store (SELECT, INSERT, CREATE TABLE, ...). ClickHouse is column-oriented and optimised for large-scale analytical queries over immutable data; prefer it for aggregations and reporting rather than row-level updates or deletes.",
  vec![Permission::Network],
  run
);
