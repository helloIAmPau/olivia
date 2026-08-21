use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FormatterResult;

use std::time::Duration;

use url::Url;

use rusty_s3::Bucket;
use rusty_s3::Credentials;
use rusty_s3::UrlStyle;
use rusty_s3::S3Action;
use rusty_s3::actions::ListObjectsV2;

use crate::http::HttpClient;
use crate::http::HttpError;

const EXPIRES: Duration = Duration::from_secs(300);

#[derive(Debug)]
pub enum S3Error {
  Endpoint(String),
  Bucket(String),
  Http(HttpError),
  Server(u16, String),
  Parse(String)
}

impl Display for S3Error {
  fn fmt(&self, formatter: &mut Formatter) -> FormatterResult {
    return match self {
      S3Error::Endpoint(error) => write!(formatter, "Invalid endpoint: {}", error),
      S3Error::Bucket(error) => write!(formatter, "Invalid bucket configuration: {}", error),
      S3Error::Http(error) => write!(formatter, "{}", error),
      S3Error::Server(status, body) => write!(formatter, "S3 returned status {}: {}", status, body),
      S3Error::Parse(error) => write!(formatter, "Could not parse the S3 response: {}", error)
    };
  }
}

pub struct S3 {
  bucket: Bucket,
  credentials: Credentials
}

impl S3 {
  pub fn new(endpoint: &str, bucket: &str, region: &str, access_key: &str, secret_key: &str) -> Result<Self, S3Error> {
    let endpoint = match Url::parse(endpoint) {
      Ok(endpoint) => endpoint,
      Err(error) => {
        return Err(S3Error::Endpoint(error.to_string()));
      }
    };

    let bucket = match Bucket::new(endpoint, UrlStyle::Path, bucket.to_string(), region.to_string()) {
      Ok(bucket) => bucket,
      Err(error) => {
        return Err(S3Error::Bucket(error.to_string()));
      }
    };

    let credentials = Credentials::new(access_key.to_string(), secret_key.to_string());

    return Ok(Self {
      bucket,
      credentials
    });
  }

  pub fn list(&self) -> Result<String, S3Error> {
    let action = self.bucket.list_objects_v2(Some(&self.credentials));
    let url = action.sign(EXPIRES);

    let client = HttpClient::new(self.bucket.base_url().to_string());
    let response = match client.get(url.path(), vec![], url.query_pairs()) {
      Ok(response) => response,
      Err(error) => {
        return Err(S3Error::Http(error));
      }
    };

    if response.is_success() == false {
      return Err(S3Error::Server(response.status, response.text()));
    }

    let parsed = match ListObjectsV2::parse_response(&response.text()) {
      Ok(parsed) => parsed,
      Err(error) => {
        return Err(S3Error::Parse(error.to_string()));
      }
    };

    let mut lines: Vec<String> = Vec::new();
    for prefix in parsed.common_prefixes {
      lines.push(prefix.prefix);
    }
    for object in parsed.contents {
      lines.push(format!("{} ({} bytes)", object.key, object.size));
    }

    return match lines.is_empty() {
      true => Ok("(no objects)".to_string()),
      false => Ok(lines.join("\n"))
    };
  }

  pub fn upload(&self, key: &str, content: Vec<u8>) -> Result<(), S3Error> {
    let action = self.bucket.put_object(Some(&self.credentials), key);
    let url = action.sign(EXPIRES);

    let length = content.len().to_string();
    let headers: Vec<(&'static str, &str)> = vec![("Content-Length", length.as_str())];

    let client = HttpClient::new(self.bucket.base_url().to_string());
    let response = match client.put_raw(url.path(), headers, url.query_pairs(), content) {
      Ok(response) => response,
      Err(error) => {
        return Err(S3Error::Http(error));
      }
    };

    if response.is_success() == false {
      return Err(S3Error::Server(response.status, response.text()));
    }

    return Ok(());
  }

  pub fn delete(&self, key: &str) -> Result<(), S3Error> {
    let action = self.bucket.delete_object(Some(&self.credentials), key);
    let url = action.sign(EXPIRES);

    let client = HttpClient::new(self.bucket.base_url().to_string());
    let response = match client.delete(url.path(), vec![], url.query_pairs(), None::<&()>) {
      Ok(response) => response,
      Err(error) => {
        return Err(S3Error::Http(error));
      }
    };

    if response.is_success() == false {
      return Err(S3Error::Server(response.status, response.text()));
    }

    return Ok(());
  }

  pub fn download(&self, key: &str) -> Result<Vec<u8>, S3Error> {
    let action = self.bucket.get_object(Some(&self.credentials), key);
    let url = action.sign(EXPIRES);

    let client = HttpClient::new(self.bucket.base_url().to_string());
    let response = match client.get(url.path(), vec![], url.query_pairs()) {
      Ok(response) => response,
      Err(error) => {
        return Err(S3Error::Http(error));
      }
    };

    if response.is_success() == false {
      return Err(S3Error::Server(response.status, response.text()));
    }

    return Ok(response.bytes().to_vec());
  }
}
