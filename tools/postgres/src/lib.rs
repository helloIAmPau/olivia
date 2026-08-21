use schemars::JsonSchema;
use serde::Deserialize;

use common::define_tool;
use common::postgres::Postgres;

#[derive(Deserialize, JsonSchema)]
struct PostgresClientParams {
  /// The exact `connection_string` of the target data store, copied verbatim from the AVAILABLE DATA STORES section (e.g. postgresql://user:password@host:5432/database).
  connection_string: String,
  /// A single SQL statement to execute (SELECT, INSERT, UPDATE, CREATE TABLE, ...). The result is returned as CSV: a header row of column names followed by one row per record.
  query: String
}

fn run(input: PostgresClientParams) -> ToolOutput {
  println!("[postgres_client] Enabling tool");

  let client = Postgres::new(&input.connection_string);

  return match client.query(input.query) {
    Ok(content) => ToolOutput {
      state: ToolOutputState::Done,
      content
    },
    Err(error) => ToolOutput {
      state: ToolOutputState::Error,
      content: error.to_string()
    }
  };
}

define_tool!(
  PostgresClient,
  PostgresClientParams,
  "Executes a single SQL statement against a PostgreSQL data store and returns the result as CSV (a header row of column names followed by one row per record). Pass the exact connection_string from the AVAILABLE DATA STORES section plus the SQL to run. Use it to read from or write to a store (SELECT, INSERT, UPDATE, DELETE, CREATE TABLE, ...).",
  vec![Permission::Network],
  run
);
