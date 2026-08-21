use schemars::JsonSchema;

use serde::Deserialize;

use common::define_tool;
use common::fs::Sandbox;

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
  let sandbox = Sandbox::new();

  let target = match sandbox.resolve(input.path.unwrap_or_default()) {
    Ok(target) => target,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: error.to_string()
      };
    }
  };

  match input.operation {
    FsOperation::List => {
      let entries = match sandbox.list(target.clone()) {
        Ok(entries) => entries,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error.to_string()
          };
        }
      };

      let mut listing = String::new();
      for entry in entries {
        let marker = if entry.is_dir { "dir " } else { "file" };
        listing = format!("{}{} {}\n", listing, marker, entry.name);
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
          return match sandbox.create_directory(target.clone()) {
            Ok(_) => ToolOutput {
              state: ToolOutputState::Done,
              content: format!("Created directory {}", target)
            },
            Err(error) => ToolOutput {
              state: ToolOutputState::Error,
              content: error.to_string()
            }
          };
        },
        FsKind::File => {
          return match sandbox.create_file(target.clone(), input.content.as_deref()) {
            Ok(bytes) => ToolOutput {
              state: ToolOutputState::Done,
              content: format!("Created file {} ({} bytes)", target, bytes)
            },
            Err(error) => ToolOutput {
              state: ToolOutputState::Error,
              content: error.to_string()
            }
          };
        }
      }
    },
    FsOperation::Delete => {
      return match sandbox.delete(target.clone()) {
        Ok(true) => ToolOutput {
          state: ToolOutputState::Done,
          content: format!("Deleted directory {}", target)
        },
        Ok(false) => ToolOutput {
          state: ToolOutputState::Done,
          content: format!("Deleted file {}", target)
        },
        Err(error) => ToolOutput {
          state: ToolOutputState::Error,
          content: error.to_string()
        }
      };
    }
  }
}

define_tool!(
  Fs,
  FsParams,
  "Manages files and directories inside the shared /sandbox working directory. Supports three operations selected via the `operation` field: \"list\" returns the entries (each prefixed with `dir ` or `file`) of the directory at `path` (defaults to the sandbox root); \"create\" makes a new file or directory at `path` depending on `kind` (\"file\" or \"directory\"), optionally writing `content` into a file and creating any missing parent directories; \"delete\" removes the file or directory at `path` (directories are removed recursively). All paths are relative to /sandbox, must not contain \"..\", and cannot escape the sandbox. Use it to inspect what other tools have written, to lay out folders, and to clean up files.",
  vec![Permission::FileSystem],
  run
);
