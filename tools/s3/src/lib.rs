use schemars::JsonSchema;
use serde::Deserialize;

use common::define_tool;
use common::s3::S3;
use common::fs::Sandbox;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum S3Operation {
  List,
  Create,
  Delete,
  Download
}

#[derive(Deserialize, JsonSchema)]
struct S3ClientParams {
  /// The exact `bucket` of the target data store, copied verbatim from the AVAILABLE DATA STORES section.
  bucket: String,
  /// The exact `region` of the target data store, copied verbatim from the AVAILABLE DATA STORES section (e.g. us-east-1).
  region: String,
  /// The exact `endpoint` of the target data store, copied verbatim from the AVAILABLE DATA STORES section. It must include an explicit http:// or https:// scheme, e.g. http://minio:9000.
  endpoint: String,
  /// The exact `access_key` of the target data store, copied verbatim from the AVAILABLE DATA STORES section.
  access_key: String,
  /// The exact `secret_key` of the target data store, copied verbatim from the AVAILABLE DATA STORES section.
  secret_key: String,
  /// The operation to perform: "list" (list the objects in the bucket), "create" (upload a sandbox file), "delete" (remove a file) or "download" (save an object into the sandbox).
  operation: S3Operation,
  /// The object key (its path within the bucket), e.g. reports/2026.txt. Required for create, delete and download; ignored for list.
  key: Option<String>,
  /// The file inside the /sandbox directory to upload (for create) or to save the downloaded object as (for download), e.g. report.pdf. Required for create and download; ignored otherwise. It is relative to /sandbox and must not contain "..".
  filename: Option<String>
}

fn run(input: S3ClientParams) -> ToolOutput {
  println!("[s3_client] Enabling tool");

  let s3 = match S3::new(&input.endpoint, &input.bucket, &input.region, &input.access_key, &input.secret_key) {
    Ok(s3) => s3,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: error.to_string()
      };
    }
  };

  let sandbox = Sandbox::new();

  match input.operation {
    S3Operation::List => {
      return match s3.list() {
        Ok(content) => ToolOutput {
          state: ToolOutputState::Done,
          content
        },
        Err(error) => ToolOutput {
          state: ToolOutputState::Error,
          content: error.to_string()
        }
      };
    },
    S3Operation::Create => {
      let key = match input.key {
        Some(key) => key,
        None => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: "The 'create' operation requires a 'key'".to_string()
          };
        }
      };

      let filename = match input.filename {
        Some(filename) => filename,
        None => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: "The 'create' operation requires a 'filename' to upload from the sandbox".to_string()
          };
        }
      };

      let target = match sandbox.resolve(filename) {
        Ok(target) => target,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error.to_string()
          };
        }
      };

      let content = match sandbox.read(target.clone()) {
        Ok(content) => content,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error.to_string()
          };
        }
      };

      return match s3.upload(&key, content) {
        Ok(_) => ToolOutput {
          state: ToolOutputState::Done,
          content: format!("Uploaded {} to object {}", target, key)
        },
        Err(error) => ToolOutput {
          state: ToolOutputState::Error,
          content: error.to_string()
        }
      };
    },
    S3Operation::Delete => {
      let key = match input.key {
        Some(key) => key,
        None => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: "The 'delete' operation requires a 'key'".to_string()
          };
        }
      };

      return match s3.delete(&key) {
        Ok(_) => ToolOutput {
          state: ToolOutputState::Done,
          content: format!("Deleted object {}", key)
        },
        Err(error) => ToolOutput {
          state: ToolOutputState::Error,
          content: error.to_string()
        }
      };
    },
    S3Operation::Download => {
      let key = match input.key {
        Some(key) => key,
        None => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: "The 'download' operation requires a 'key'".to_string()
          };
        }
      };

      let filename = match input.filename {
        Some(filename) => filename,
        None => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: "The 'download' operation requires a 'filename' to save into the sandbox".to_string()
          };
        }
      };

      let target = match sandbox.resolve(filename) {
        Ok(target) => target,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error.to_string()
          };
        }
      };

      let bytes = match s3.download(&key) {
        Ok(bytes) => bytes,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error.to_string()
          };
        }
      };

      let size = bytes.len();

      match sandbox.write(target.clone(), &bytes) {
        Ok(_) => {},
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error.to_string()
          };
        }
      };

      return ToolOutput {
        state: ToolOutputState::Done,
        content: format!("Downloaded object {} ({} bytes) to {}", key, size, target)
      };
    }
  }
}

define_tool!(
  S3Client,
  S3ClientParams,
  "Manages files in an S3-compatible object store (AWS S3, RustFS, MinIO, ...) over its HTTP API using presigned requests, exchanging file contents through the /sandbox directory. Pass the exact bucket, region, endpoint, access_key and secret_key from the AVAILABLE DATA STORES section plus an operation: 'list' lists the objects in the bucket, 'create' uploads the sandbox file named by 'filename' to object 'key', 'delete' removes object 'key', and 'download' saves object 'key' into the sandbox as 'filename'. Use it to persist, retrieve or manage documents, blobs and other files rather than structured relational data.",
  vec![Permission::Network, Permission::FileSystem],
  run
);
