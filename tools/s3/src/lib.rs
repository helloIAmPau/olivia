use std::time::Duration;
use std::fs::read;
use std::fs::write;

use schemars::JsonSchema;
use serde::Deserialize;

use url::Url;

use waki::Client;
use waki::Response;

use rusty_s3::Bucket;
use rusty_s3::Credentials;
use rusty_s3::UrlStyle;
use rusty_s3::S3Action;
use rusty_s3::actions::ListObjectsV2;

use common::define_tool;

// Shared working directory preopened for every tool; uploads are read from here
// and downloads are written here.
const SANDBOX_PATH: &str = "/sandbox";

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
  /// The name of the file inside the /sandbox directory to upload (for create) or to save the downloaded object as (for download), e.g. report.pdf. Required for create and download; ignored otherwise. Must be a plain file name: no directory separators and no "..".
  filename: Option<String>
}

const EXPIRES: Duration = Duration::from_secs(300);

fn read_ok(response: Response) -> Result<Vec<u8>, String> {
  let status = response.status_code();

  let bytes = match response.body() {
    Ok(bytes) => bytes,
    Err(error) => return Err(format!("Could not read the S3 response: {}", error))
  };

  if status < 200 || status >= 300 {
    let body = String::from_utf8_lossy(&bytes).into_owned();

    return Err(format!("S3 returned status {}: {}", status, body));
  }

  return Ok(bytes);
}

fn run(input: S3ClientParams) -> ToolOutput {
  println!("[s3_client] Enabling tool");

  let endpoint = match Url::parse(&input.endpoint) {
    Ok(endpoint) => endpoint,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Invalid endpoint {}: {}", input.endpoint, error)
      };
    }
  };

  let bucket = match Bucket::new(endpoint, UrlStyle::Path, input.bucket.clone(), input.region.clone()) {
    Ok(bucket) => bucket,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Invalid bucket configuration for {}: {}", input.bucket, error)
      };
    }
  };

  let credentials = Credentials::new(input.access_key, input.secret_key);

  match input.operation {
    S3Operation::List => {
      let action = bucket.list_objects_v2(Some(&credentials));
      let url = action.sign(EXPIRES);

      let response = match Client::new().get(url.as_str()).send() {
        Ok(response) => response,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Request to S3 failed: {}", error)
          };
        }
      };

      let bytes = match read_ok(response) {
        Ok(bytes) => bytes,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error
          };
        }
      };

      let body = String::from_utf8_lossy(&bytes).into_owned();

      let parsed = match ListObjectsV2::parse_response(&body) {
        Ok(parsed) => parsed,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Could not parse the S3 listing: {}", error)
          };
        }
      };

      let mut lines: Vec<String> = Vec::new();
      for prefix in parsed.common_prefixes {
        lines.push(prefix.prefix);
      }
      for object in parsed.contents {
        lines.push(format!("{} ({} bytes)", object.key, object.size));
      }

      let content = match lines.is_empty() {
        true => "(no objects)".to_string(),
        false => lines.join("\n")
      };

      return ToolOutput {
        state: ToolOutputState::Done,
        content
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

      if filename.is_empty() || filename.contains('/') || filename.contains("..") {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Invalid filename {}: it must be a plain file name with no '/' or '..'", filename)
        };
      }

      let path = format!("{}/{}", SANDBOX_PATH, filename);
      let content = match read(&path) {
        Ok(content) => content,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Unable to read the sandbox file {}: {}", path, error)
          };
        }
      };

      let action = bucket.put_object(Some(&credentials), &key);
      let url = action.sign(EXPIRES);

      let length = content.len();

      let response = match Client::new().put(url.as_str()).header("Content-Length", length.to_string()).body(content).send() {
        Ok(response) => response,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Request to S3 failed: {}", error)
          };
        }
      };

      match read_ok(response) {
        Ok(_) => {
          return ToolOutput {
            state: ToolOutputState::Done,
            content: format!("Uploaded {} to object {}", path, key)
          };
        },
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error
          };
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

      let action = bucket.delete_object(Some(&credentials), &key);
      let url = action.sign(EXPIRES);

      let response = match Client::new().delete(url.as_str()).send() {
        Ok(response) => response,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Request to S3 failed: {}", error)
          };
        }
      };

      match read_ok(response) {
        Ok(_) => {
          return ToolOutput {
            state: ToolOutputState::Done,
            content: format!("Deleted object {}", key)
          };
        },
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error
          };
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

      if filename.is_empty() || filename.contains('/') || filename.contains("..") {
        return ToolOutput {
          state: ToolOutputState::Error,
          content: format!("Invalid filename {}: it must be a plain file name with no '/' or '..'", filename)
        };
      }

      let action = bucket.get_object(Some(&credentials), &key);
      let url = action.sign(EXPIRES);

      let response = match Client::new().get(url.as_str()).send() {
        Ok(response) => response,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Request to S3 failed: {}", error)
          };
        }
      };

      let bytes = match read_ok(response) {
        Ok(bytes) => bytes,
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: error
          };
        }
      };

      let path = format!("{}/{}", SANDBOX_PATH, filename);
      let size = bytes.len();

      match write(&path, &bytes) {
        Ok(_) => {},
        Err(error) => {
          return ToolOutput {
            state: ToolOutputState::Error,
            content: format!("Unable to write the downloaded object to {}: {}", path, error)
          };
        }
      };

      return ToolOutput {
        state: ToolOutputState::Done,
        content: format!("Downloaded object {} ({} bytes) to {}", key, size, path)
      };
    }
  }
}

define_tool!(
  S3Client,
  S3ClientParams,
  "Manages files in an S3-compatible object store (AWS S3, RustFS, MinIO, ...) over its HTTP API using presigned requests, exchanging file contents through the /sandbox directory. Pass the exact bucket, region, endpoint, access_key and secret_key from the AVAILABLE DATA STORES section plus an operation: 'list' lists the objects in the bucket, 'create' uploads the sandbox file named by 'filename' to object 'key', 'delete' removes object 'key', and 'download' saves object 'key' into the sandbox as 'filename'. Use it to persist, retrieve or manage documents, blobs and other files rather than structured relational data.",
  run
);
