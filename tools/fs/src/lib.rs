use std::path::Path;

use std::fs::create_dir_all;
use std::fs::metadata;
use std::fs::read_dir;
use std::fs::remove_dir_all;
use std::fs::remove_file;
use std::fs::write;

use schemars::JsonSchema;

use serde::Deserialize;

use common::define_tool;

// Shared working directory preopened for every tool; it is the only writable
// location, so every path this tool touches is resolved inside it.
const SANDBOX_PATH: &str = "/sandbox";

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum FsOperation {
  List,
  Create,
  Delete
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum FsKind {
  File,
  Directory
}

#[derive(Deserialize, JsonSchema)]
struct FsParams {
  /// The operation to perform: "list" (list the entries of a directory), "create" (create a file or directory) or "delete" (remove a file or directory, recursively for directories).
  operation: FsOperation,
  /// The path to operate on, relative to the /sandbox directory (e.g. reports or reports/2026.txt). Leading slashes are ignored and it must not contain "..". An empty path, "." or "/" refers to the sandbox root. For "list" it defaults to the sandbox root when omitted.
  path: Option<String>,
  /// What to create: "file" or "directory". Required for the "create" operation; ignored otherwise.
  kind: Option<FsKind>,
  /// The text content to write when creating a file. Optional: when omitted an empty file is created. Ignored for directories and for other operations.
  content: Option<String>
}

fn run(input: FsParams) -> ToolOutput {
  let raw = match input.path {
    Some(path) => path,
    None => String::new()
  };

  let trimmed = raw.trim();
  let target = if trimmed.is_empty() || trimmed == "." || trimmed == "/" {
    SANDBOX_PATH.to_string()
  } else if trimmed.contains("..") {
    return ToolOutput {
      state: ToolOutputState::Error,
      content: format!("Invalid path {}: it must not contain '..'", raw)
    };
  } else {
    format!("{}/{}", SANDBOX_PATH, trimmed.trim_start_matches('/'))
  };

  match input.operation {
    FsOperation::List => {
      let entries = match read_dir(&target) {
        Ok(entries) => entries,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Unable to list {}: {}", target, error)
          };
        }
      };

      let mut listing = String::new();

      for entry in entries {
        let entry = match entry {
          Ok(entry) => entry,
          Err(error) => {
            return ToolOutput {
              state: ToolOutputState::Error,
              content: format!("Unable to read an entry of {}: {}", target, error)
            };
          }
        };

        let file_type = match entry.file_type() {
          Ok(file_type) => file_type,
          Err(error) => {
            return ToolOutput {
              state: ToolOutputState::Error,
              content: format!("Unable to inspect an entry of {}: {}", target, error)
            };
          }
        };

        let marker = if file_type.is_dir() { "dir " } else { "file" };
        let name = entry.file_name().to_string_lossy().into_owned();

        listing = format!("{}{} {}\n", listing, marker, name);
      }

      if listing.is_empty() {
        return ToolOutput {
          state: ToolOutputState::Done,
          content: format!("{} is empty", target)
        };
      }

      return ToolOutput {
        state: ToolOutputState::Done,
        content: listing
      };
    },
    FsOperation::Create => {
      let kind = match input.kind {
        Some(kind) => kind,
        None => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: "The 'kind' field (\"file\" or \"directory\") is required for the create operation".to_string()
          };
        }
      };

      match kind {
        FsKind::Directory => {
          match create_dir_all(&target) {
            Ok(_) => {},
            Err(error) => {
              return ToolOutput {
                state: ToolOutputState::Error,
                content: format!("Unable to create directory {}: {}", target, error)
              };
            }
          };

          return ToolOutput {
            state: ToolOutputState::Done,
            content: format!("Created directory {}", target)
          };
        },
        FsKind::File => {
          // Make sure the parent directory exists so nested files can be created.
          match Path::new(&target).parent() {
            Some(parent) => {
              match create_dir_all(parent) {
                Ok(_) => {},
                Err(error) => {
                  return ToolOutput {
                    state: ToolOutputState::Error,
                    content: format!("Unable to create parent directory for {}: {}", target, error)
                  };
                }
              };
            },
            None => {}
          };

          let body = match input.content {
            Some(body) => body,
            None => String::new()
          };

          match write(&target, body.as_bytes()) {
            Ok(_) => {},
            Err(error) => {
              return ToolOutput {
                state: ToolOutputState::Error,
                content: format!("Unable to create file {}: {}", target, error)
              };
            }
          };

          return ToolOutput {
            state: ToolOutputState::Done,
            content: format!("Created file {} ({} bytes)", target, body.len())
          };
        }
      }
    },
    FsOperation::Delete => {
      if target == SANDBOX_PATH {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: "Refusing to delete the sandbox root".to_string()
        };
      }

      let meta = match metadata(&target) {
        Ok(meta) => meta,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Unable to delete {}: {}", target, error)
          };
        }
      };

      if meta.is_dir() {
        match remove_dir_all(&target) {
          Ok(_) => {},
          Err(error) => {
            return ToolOutput {
              state: ToolOutputState::Error,
              content: format!("Unable to delete directory {}: {}", target, error)
            };
          }
        };

        return ToolOutput {
          state: ToolOutputState::Done,
          content: format!("Deleted directory {}", target)
        };
      }

      match remove_file(&target) {
        Ok(_) => {},
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Unable to delete file {}: {}", target, error)
          };
        }
      };

      return ToolOutput {
        state: ToolOutputState::Done,
        content: format!("Deleted file {}", target)
      };
    }
  }
}

define_tool!(
  Fs,
  FsParams,
  "Manages files and directories inside the shared /sandbox working directory. Supports three operations selected via the `operation` field: \"list\" returns the entries (each prefixed with `dir ` or `file`) of the directory at `path` (defaults to the sandbox root); \"create\" makes a new file or directory at `path` depending on `kind` (\"file\" or \"directory\"), optionally writing `content` into a file and creating any missing parent directories; \"delete\" removes the file or directory at `path` (directories are removed recursively). All paths are relative to /sandbox, must not contain \"..\", and cannot escape the sandbox. Use it to inspect what other tools have written, to lay out folders, and to clean up files.",
  run
);
