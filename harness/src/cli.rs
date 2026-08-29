const DEFAULT_CONFIG_PATH: &str = "/config.toml";
const DEFAULT_TOOLS_FOLDER: &str = "/tools";
const DEFAULT_SANDBOX_FOLDER: &str = "/sandbox";

pub struct CliOptions {
  pub config_path: String,
  pub tools_folder: String,
  pub sandbox_folder: String
}

pub fn resolve() -> CliOptions {
  let mut config_path = DEFAULT_CONFIG_PATH.to_string();
  let mut tools_folder = DEFAULT_TOOLS_FOLDER.to_string();
  let mut sandbox_folder = DEFAULT_SANDBOX_FOLDER.to_string();

  let mut args = std::env::args();

  let _ = args.next();

  loop {
    let arg = match args.next() {
      Some(arg) => arg,
      None => break
    };

    let pair = match arg.strip_prefix("--") {
      Some(pair) => pair,
      None => panic!("Invalid argument '{}': only --key=value options are allowed", arg)
    };

    let (key, value) = match pair.split_once('=') {
      Some(parts) => parts,
      None => panic!("Invalid argument '{}': options must be in --key=value format", arg)
    };

    match key {
      "config" => config_path = value.to_string(),
      "tools" => tools_folder = value.to_string(),
      "sandbox" => sandbox_folder = value.to_string(),
      _ => panic!("Unknown option '--{}'", key)
    };
  }

  return CliOptions {
    config_path,
    tools_folder,
    sandbox_folder
  };
}
