use std::env::var;

use schemars::JsonSchema;

use serde::Deserialize;
use serde::Serialize;

use common::define_tool;
use common::http::HttpClient;
use common::fs::Sandbox;

#[derive(Deserialize, JsonSchema)]
struct DownloadParams {
  /// The body of a JavaScript module that default-exports an async function
  /// receiving `{ page }` (a Puppeteer Page). It MUST trigger a real browser
  /// download event so browserless can capture the file. A plain
  /// `page.goto(fileUrl)` does NOT work: Chrome renders PDFs/images inline
  /// instead of downloading them. Instead, either click an existing download
  /// link on the page, or create an anchor with a `download` attribute and
  /// click it, then wait for the download to finish.
  /// Example (click a link):
  /// export default async function ({ page }) { await page.goto('https://example.com', { waitUntil: 'networkidle2' }); await page.click('a[href$=".pdf"]'); await new Promise(r => setTimeout(r, 5000)); }
  /// Example (force-download a URL):
  /// export default async function ({ page }) { await page.goto('https://example.com'); await page.evaluate(() => { const a = document.createElement('a'); a.href = 'https://example.com/report.pdf'; a.download = ''; document.body.appendChild(a); a.click(); }); await new Promise(r => setTimeout(r, 5000)); }
  code: String,
  /// The name to save the downloaded file under, inside the /sandbox directory
  /// (e.g. report.pdf). It is relative to /sandbox and must not contain "..".
  filename: String
}

#[derive(Serialize)]
struct FunctionRequest {
  code: String
}

fn run(input: DownloadParams) -> ToolOutput {
  let token = match var("BROWSERLESS_TOKEN") {
    Ok(token) => token,
    Err(_) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: "BROWSERLESS_TOKEN not set".to_string()
      };
    }
  };

  let host = match var("BROWSERLESS_HOST") {
    Ok(host) => host,
    Err(_) => "http://browserless:3000".to_string()
  };

  let sandbox = Sandbox::new();
  let target = match sandbox.resolve(input.filename) {
    Ok(target) => target,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: error.to_string()
      };
    }
  };

  let payload = FunctionRequest {
    code: input.code
  };

  let client = HttpClient::new(host);
  let query = vec![
    ("token", token.as_str()),
    ("launch", "{\"stealth\":true}")
  ];

  let response = match client.post("/download", vec![], query, Some(&payload)) {
    Ok(response) => response,
    Err(error) => {
      return ToolOutput {
        state: ToolOutputState::Error,
        content: format!("Request to browserless failed: {}", error)
      };
    }
  };

  if response.is_success() == false {
    return ToolOutput {
      state: ToolOutputState::Error,
      content: format!("browserless /download returned status {}: {}", response.status, response.text())
    };
  }

  let bytes = response.bytes();
  let size = bytes.len();

  println!("[download] Saving {} bytes to {}", size, target);

  match sandbox.write(target.clone(), bytes) {
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
    content: format!("Downloaded {} bytes to {}", size, target)
  };
}

define_tool!(
  Download,
  DownloadParams,
  "Downloads a file through the browserless /download API and saves it into the /sandbox directory. Provide the body of a JavaScript module that default-exports an async function receiving { page } (a Puppeteer Page) plus the filename to save it under in /sandbox. The function MUST trigger a real browser download event — click an existing download link, or create an anchor with a `download` attribute and click it — because a plain page.goto(fileUrl) does NOT download (Chrome renders PDFs/images inline). Browserless captures whatever file the page downloads and this tool writes its bytes to /sandbox/<filename>, returning the saved path. Use it to fetch documents, images, PDFs or other binary files — especially ones behind JavaScript, forms or auth — so other tools can read them from the sandbox afterwards.",
  vec![Permission::Network, Permission::FileSystem],
  run
);
