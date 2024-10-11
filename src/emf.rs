use serde_json::Value;

/// Return if the line can be parsed as a valid JSON object
/// with a top level key `"_aws"`.
pub fn is_emf(line: &str) -> bool {
  // perf: check if the line is wrapped with `{}` before parsing it as JSON
  // so we can fast fail if it's not a JSON object.
  // we trim the line in 2 steps to avoid unnecessary trimming.
  let trimmed = line.trim_start();
  if !trimmed.starts_with('{') {
    return false;
  }
  let trimmed = trimmed.trim_end();
  if !trimmed.ends_with('}') {
    return false;
  }

  serde_json::from_str(trimmed)
    .ok()
    .map(|value: Value| value.get("_aws").is_some())
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn check_emf() {
    // compact
    assert!(is_emf(r#"{"_aws":{"key":"value"}}"#));
    // with whitespace
    assert!(is_emf(r#"{"_aws": {"key": "value"}}"#));
    assert!(is_emf(r#"  {  "_aws"  : {"key": "value"}  }  "#));

    // missing "_aws"
    assert!(!is_emf(r#"{"key": "value"}"#));
    assert!(!is_emf(r#"{"  _aws":{"key":"value"}}"#));
    // invalid JSON
    assert!(!is_emf(r#"{"_aws": {"key": "value"}"#));
    // not a JSON object
    assert!(!is_emf("123"));
    assert!(!is_emf("{"));
  }
}
