use extism_pdk::plugin_fn;
use extism_pdk::FnResult;

#[plugin_fn]
pub fn info() -> FnResult<()> {
  return Ok(());
}

#[plugin_fn]
pub fn execute() -> FnResult<()> {
  return Ok(());
}
