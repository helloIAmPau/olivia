use url::Url;

use schemars::JsonSchema;
use serde::Deserialize;

use bytes::BytesMut;

use fallible_iterator::FallibleIterator;

use postgres_protocol::message::frontend::startup_message;
use postgres_protocol::message::frontend::password_message;
use postgres_protocol::message::frontend::query;

use postgres_protocol::message::backend::Message;

use common::define_tool;
use common::tcp_socket::TcpSocket;

#[derive(Deserialize, JsonSchema)]
struct PostgresClientParams {
  /// The exact `connection_string` of the target data store, copied verbatim from the AVAILABLE DATA STORES section (e.g. postgresql://user:password@host:5432/database).
  connection_string: String,
  /// A single SQL statement to execute (SELECT, INSERT, UPDATE, CREATE TABLE, ...). The result is returned as CSV: a header row of column names followed by one row per record.
  query: String
}

fn run(input: PostgresClientParams) -> ToolOutput {
  println!("[postgres_client] Enabling tool");

  let connection = match Url::parse(&input.connection_string) {
    Ok(connection) => connection,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Invalid connection string {}: {}", input.connection_string, error)
      };
    }
  };

  println!("[postgres_client] Parsed connection stirng {}", input.connection_string);

  let host = match connection.host() {
    Some(host) => host.to_string().to_string().to_string().to_string(),
    None => "postgres".to_string()
  };

  println!("[postgres_client] Host: {}", host);

  let port = match connection.port() {
    Some(port) => port,
    None => 5432
  };

  println!("[postgres_client] Port: {}", port);

  let mut socket = match TcpSocket::new(host, port) {
    Ok(socket) => socket,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Unable to connect to postgres instance {}: {}", input.connection_string, error)
      };
    }
  };

  println!("[postgres_client] Socket created!");

  let mut startup_packet = BytesMut::new();
  match startup_message([
    ("user", connection.username()),
    ("database", &connection.path().replace("/", ""))
  ], &mut startup_packet) {
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Unable to create startup message for {}: {}", input.connection_string, error)
      };
    },
    _ => {}
  };

  match socket.write(&startup_packet) {
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Unable to send startup message: {}", error)
      };
    },
    _ => {}
  };

  let mut packet = BytesMut::new();
  let mut content = "".to_string();
  loop {
    match socket.read() {
      Ok(buffer) => {
        println!("[postgres_client] Received slice");

        packet.extend_from_slice(&buffer);
      },
      Err(error) => {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Unable to read from socket: {}", error)
        };
      }
    };

    loop {
      println!("[postgres_client] Raw message: {:?}", &packet);

      let message = match Message::parse(&mut packet) {
        Ok(None) => {
          println!("[postgres_client] Incomplete message received, waiting for the rest");

          break;
        },
        Ok(Some(message)) => message,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Error parsing message: {}", error)
          };
        }
      };

      println!("[postgres_client] New message received");

      match message {
        Message::AuthenticationCleartextPassword => {
          println!("[postgres_client] Received packet AuthenticationCleartextPassword");

          let password = match connection.password() {
            Some(password) => password,
            None => {
              return ToolOutput {
                state: ToolOutputState::Error,
                content: "The server requested a password, but connection string has none".to_string()
              };
            }
          };

          let mut password_packet = BytesMut::new(); 
          match password_message(password.as_bytes(), &mut password_packet) {
            Err(error) => {
              return ToolOutput {
                state: ToolOutputState::Error,
                content: format!("Unable to create password message for {}: {}", input.connection_string, error)
              };
            },
            _ => {}
          };

          match socket.write(&password_packet) {
            Err(error) => {
              return ToolOutput {
                state: ToolOutputState::Error,
                content: format!("Unable to send password message: {}", error)
              };
            },
            _ => {}
          };
        },
        Message::ReadyForQuery(_) => {
          println!("[postgres_client] Received packet ReadyForQuery");

          let mut query_packet = BytesMut::new();
          match query(&input.query, &mut query_packet) {
            Err(error) => {
              return ToolOutput {
                state: ToolOutputState::Error,
                content: format!("Unable to create query message {}: {}", input.query, error)
              };
            },
            _ => {}
          };

          match socket.write(&query_packet) {
            Err(error) => {
              return ToolOutput {
                state: ToolOutputState::Error,
                content: format!("Unable to send query message: {}", error)
              };
            },
            _ => {}
          };
        },
		  	Message::RowDescription(body) => {
		  	    println!("[postgres_client] Received packet RowDescription");

		  	    let header = match body.fields().map(|field| Ok(field.name().to_string())).collect::<Vec<String>>() {
              Ok(fields) => fields.join(", "),
              Err(error) => {
                return ToolOutput {
                  state: ToolOutputState::Error,
                  content: format!("Unable to get field names: {}", error)
                };
              }
            };
		  	
		  	    content = format!("{}DATA AS CSV:\n{}\n", content, header);
		  	},
        Message::DataRow(body) => {
          println!("[postgres_client] Received packet DataRow");

          let data = body.buffer();
          let csv = match body.ranges().map(|range| {
            match range {
              Some(range) => Ok(String::from_utf8_lossy(&data[range]).into_owned()),
              None => Ok(String::new())
            }
          }).collect::<Vec<String>>() {
            Ok(rows) => rows.join(", "),
            Err(error) => {
              return ToolOutput {
                state: ToolOutputState::Error,
                content: format!("Unable to get data: {}", error)
              };
            }
          };

          content = format!("{}{}\n", content, csv);
        },
        Message::EmptyQueryResponse => {
          println!("[postgres_client] Received packet EmptyQueryResponse");

          content = format!("{}No Data!", content);
        },
        Message::CommandComplete(_) => {
          println!("[postgres_client] Received packet CommandComplete");

          return ToolOutput {
            state: ToolOutputState::Done,
            content
          };

        },
        Message::ErrorResponse(_) => {
          println!("[postgres_client] Received packet ErrorResponse");

          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Postgres error")
          };
        },
        _ => {
          println!("[postgres_client] Message unhandled");
        }
      }
    }

    packet = BytesMut::new();
  }
}

define_tool!(
  PostgresClient,
  PostgresClientParams,
  "Executes a single SQL statement against a PostgreSQL data store and returns the result as CSV (a header row of column names followed by one row per record). Pass the exact connection_string from the AVAILABLE DATA STORES section plus the SQL to run. Use it to read from or write to a store (SELECT, INSERT, UPDATE, DELETE, CREATE TABLE, ...).",
  vec![Permission::Network],
  run
);
