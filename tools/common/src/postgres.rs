use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use url::Url;

use bytes::BytesMut;

use fallible_iterator::FallibleIterator;

use postgres_protocol::message::frontend::startup_message;
use postgres_protocol::message::frontend::password_message;
use postgres_protocol::message::frontend::query as query_message;

use postgres_protocol::message::backend::Message;

use crate::tcp_socket::TcpSocket;
use crate::tcp_socket::TcpSocketError;

// A reusable PostgreSQL client that speaks the v3 wire protocol directly over a
// TCP socket. It connects, performs cleartext-password authentication if the
// server asks for it, runs a single statement and returns the result as CSV.

#[derive(Debug)]
pub enum PostgresError {
  Url(String),
  Socket(TcpSocketError),
  Protocol(String),
  MissingPassword,
  Server
}

impl Display for PostgresError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      PostgresError::Url(error) => write!(formatter, "Invalid connection string: {}", error),
      PostgresError::Socket(error) => write!(formatter, "Socket Error - {}", error),
      PostgresError::Protocol(error) => write!(formatter, "Protocol Error - {}", error),
      PostgresError::MissingPassword => write!(formatter, "The server requested a password, but the connection string has none"),
      PostgresError::Server => write!(formatter, "Postgres returned an error response")
    };
  }
}

pub struct Postgres {
  connection_string: String
}

impl Postgres {
  pub fn new(connection_string: &str) -> Self {
    return Self {
      connection_string: connection_string.to_string()
    };
  }

  /// Connects, runs a single SQL statement and returns the result as CSV (a
  /// header row of column names followed by one row per record), or `No Data!`
  /// for a statement with no rows.
  pub fn query(&self, sql: String) -> Result<String, PostgresError> {
    let connection = match Url::parse(&self.connection_string) {
      Ok(connection) => connection,
      Err(error) => {
        return Err(PostgresError::Url(error.to_string()));
      }
    };

    let host = match connection.host() {
      Some(host) => host.to_string(),
      None => "postgres".to_string()
    };

    let port = match connection.port() {
      Some(port) => port,
      None => 5432
    };

    let mut socket = match TcpSocket::new(host, port) {
      Ok(socket) => socket,
      Err(error) => {
        return Err(PostgresError::Socket(error));
      }
    };

    let mut startup_packet = BytesMut::new();
    match startup_message([
      ("user", connection.username()),
      ("database", &connection.path().replace("/", ""))
    ], &mut startup_packet) {
      Err(error) => {
        return Err(PostgresError::Protocol(error.to_string()));
      },
      _ => {}
    };

    match socket.write(&startup_packet) {
      Err(error) => {
        return Err(PostgresError::Socket(error));
      },
      _ => {}
    };

    let mut packet = BytesMut::new();
    let mut content = "".to_string();
    loop {
      match socket.read() {
        Ok(buffer) => {
          packet.extend_from_slice(&buffer);
        },
        Err(error) => {
          return Err(PostgresError::Socket(error));
        }
      };

      loop {
        let message = match Message::parse(&mut packet) {
          Ok(None) => break,
          Ok(Some(message)) => message,
          Err(error) => {
            return Err(PostgresError::Protocol(error.to_string()));
          }
        };

        match message {
          Message::AuthenticationCleartextPassword => {
            let password = match connection.password() {
              Some(password) => password,
              None => {
                return Err(PostgresError::MissingPassword);
              }
            };

            let mut password_packet = BytesMut::new();
            match password_message(password.as_bytes(), &mut password_packet) {
              Err(error) => {
                return Err(PostgresError::Protocol(error.to_string()));
              },
              _ => {}
            };

            match socket.write(&password_packet) {
              Err(error) => {
                return Err(PostgresError::Socket(error));
              },
              _ => {}
            };
          },
          Message::ReadyForQuery(_) => {
            let mut query_packet = BytesMut::new();
            match query_message(&sql, &mut query_packet) {
              Err(error) => {
                return Err(PostgresError::Protocol(error.to_string()));
              },
              _ => {}
            };

            match socket.write(&query_packet) {
              Err(error) => {
                return Err(PostgresError::Socket(error));
              },
              _ => {}
            };
          },
          Message::RowDescription(body) => {
            let mut fields = body.fields();
            let mut names: Vec<String> = Vec::new();
            loop {
              let field = match fields.next() {
                Ok(Some(field)) => field,
                Ok(None) => break,
                Err(error) => {
                  return Err(PostgresError::Protocol(format!("Unable to get field names: {}", error)));
                }
              };

              names.push(field.name().to_string());
            }

            content = format!("{}DATA AS CSV:\n{}\n", content, names.join(", "));
          },
          Message::DataRow(body) => {
            let data = body.buffer();
            let mut ranges = body.ranges();
            let mut values: Vec<String> = Vec::new();
            loop {
              let range = match ranges.next() {
                Ok(Some(range)) => range,
                Ok(None) => break,
                Err(error) => {
                  return Err(PostgresError::Protocol(format!("Unable to get data: {}", error)));
                }
              };

              let value = match range {
                Some(range) => String::from_utf8_lossy(&data[range]).into_owned(),
                None => String::new()
              };

              values.push(value);
            }

            content = format!("{}{}\n", content, values.join(", "));
          },
          Message::EmptyQueryResponse => {
            content = format!("{}No Data!", content);
          },
          Message::CommandComplete(_) => {
            return Ok(content);
          },
          Message::ErrorResponse(_) => {
            return Err(PostgresError::Server);
          },
          _ => {}
        }
      }

      packet = BytesMut::new();
    }
  }
}
