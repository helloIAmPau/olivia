use std::net::TcpStream;
use std::io::Write;
use std::io::Read;

use std::io::Error as IoError;
use std::fmt::Result as FormatterResult;
use std::fmt::Formatter;
use std::fmt::Display;

#[derive(Debug)]
pub enum TcpSocketError {
  Io(IoError),
  ServerClosedConnection
}

impl Display for TcpSocketError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      TcpSocketError::Io(error) => write!(formatter, "Io Error - {}", error),
      TcpSocketError::ServerClosedConnection => write!(formatter, "Server Closed Connection")
    }
  }
}

pub struct TcpSocket {
  stream: TcpStream
}

impl TcpSocket {
  pub fn new(host: String, port: u16) -> Result<Self, TcpSocketError> {
    let stream = match TcpStream::connect((host, port)) {
      Ok(stream) => stream,
      Err(error) => {
        return Err(TcpSocketError::Io(error));
      }
    };

    return Ok(Self {
      stream
    });
  }

  pub fn write(&mut self, bytes: &[u8]) -> Result<(), TcpSocketError> {
    match self.stream.write_all(bytes) {
      Err(error) => {
        return Err(TcpSocketError::Io(error));
      },
      _ => {}
    };

    match self.stream.flush() {
      Err(error) => {
        return Err(TcpSocketError::Io(error));
      },
      _ => {}
    };

    return Ok(());
  }

  pub fn read(&mut self) -> Result<Vec<u8>, TcpSocketError> {
    let mut chunk = [0u8; 8192];

    let read = match self.stream.read(&mut chunk) {
      Ok(read) => read,
      Err(error) => {
        return Err(TcpSocketError::Io(error));
      }
    };

    if read == 0 {
      return Err(TcpSocketError::ServerClosedConnection);
    }

    return Ok(chunk[..read].to_vec());
  }
}
