use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use std::path::Path;

use std::fs::create_dir_all;
use std::fs::metadata;
use std::fs::read as fs_read;
use std::fs::read_dir;
use std::fs::remove_dir_all;
use std::fs::remove_file;
use std::fs::write as fs_write;

// The shared working directory preopened for every tool.
const SANDBOX_PATH: &str = "/sandbox";

#[derive(Debug)]
pub enum FsError {
  InvalidPath(String),
  RootDelete,
  Io(String)
}

impl Display for FsError {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      FsError::InvalidPath(path) => write!(formatter, "Invalid path {}: it must not contain '..'", path),
      FsError::RootDelete => write!(formatter, "Refusing to delete the sandbox root"),
      FsError::Io(message) => write!(formatter, "{}", message)
    };
  }
}

pub struct Entry {
  pub name: String,
  pub is_dir: bool
}

pub struct Sandbox {
  root: String
}

impl Sandbox {
  pub fn new() -> Self {
    return Self {
      root: SANDBOX_PATH.to_string()
    };
  }

  pub fn resolve(&self, path: String) -> Result<String, FsError> {
    let trimmed = path.trim();

    if trimmed.is_empty() || trimmed == "." || trimmed == "/" {
      return Ok(self.root.clone());
    }

    if trimmed.contains("..") {
      return Err(FsError::InvalidPath(path.to_string()));
    }

    return Ok(format!("{}/{}", self.root, trimmed.trim_start_matches('/')));
  }

  pub fn read(&self, target: String) -> Result<Vec<u8>, FsError> {
    return match fs_read(&target) {
      Ok(bytes) => Ok(bytes),
      Err(error) => Err(FsError::Io(format!("Unable to read {}: {}", target, error)))
    };
  }

  pub fn write(&self, target: String, bytes: &[u8]) -> Result<(), FsError> {
    match Path::new(&target).parent() {
      Some(parent) => {
        match create_dir_all(parent) {
          Ok(_) => {},
          Err(error) => {
            return Err(FsError::Io(format!("Unable to create parent directory for {}: {}", target, error)));
          }
        };
      },
      None => {}
    };

    return match fs_write(&target, bytes) {
      Ok(_) => Ok(()),
      Err(error) => Err(FsError::Io(format!("Unable to write {}: {}", target, error)))
    };
  }

  pub fn list(&self, target: String) -> Result<Vec<Entry>, FsError> {
    let entries = match read_dir(&target) {
      Ok(entries) => entries,
      Err(error) => {
        return Err(FsError::Io(format!("Unable to list {}: {}", target, error)));
      }
    };

    let mut result: Vec<Entry> = Vec::new();

    for entry in entries {
      let entry = match entry {
        Ok(entry) => entry,
        Err(error) => {
          return Err(FsError::Io(format!("Unable to read an entry of {}: {}", target, error)));
        }
      };

      let file_type = match entry.file_type() {
        Ok(file_type) => file_type,
        Err(error) => {
          return Err(FsError::Io(format!("Unable to inspect an entry of {}: {}", target, error)));
        }
      };

      result.push(Entry {
        name: entry.file_name().to_string_lossy().into_owned(),
        is_dir: file_type.is_dir()
      });
    }

    return Ok(result);
  }

  pub fn create_directory(&self, target: String) -> Result<(), FsError> {
    return match create_dir_all(&target) {
      Ok(_) => Ok(()),
      Err(error) => Err(FsError::Io(format!("Unable to create directory {}: {}", target, error)))
    };
  }

  pub fn create_file(&self, target: String, content: Option<&str>) -> Result<usize, FsError> {
    let body = match content {
      Some(body) => body,
      None => ""
    };

    return match self.write(target, body.as_bytes()) {
      Ok(_) => Ok(body.len()),
      Err(error) => Err(error)
    };
  }

  pub fn delete(&self, target: String) -> Result<bool, FsError> {
    if target == self.root {
      return Err(FsError::RootDelete);
    }

    let meta = match metadata(&target) {
      Ok(meta) => meta,
      Err(error) => {
        return Err(FsError::Io(format!("Unable to delete {}: {}", target, error)));
      }
    };

    if meta.is_dir() {
      return match remove_dir_all(&target) {
        Ok(_) => Ok(true),
        Err(error) => Err(FsError::Io(format!("Unable to delete directory {}: {}", target, error)))
      };
    }

    return match remove_file(&target) {
      Ok(_) => Ok(false),
      Err(error) => Err(FsError::Io(format!("Unable to delete file {}: {}", target, error)))
    };
  }
}
