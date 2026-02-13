use std::path::Path;

#[test]
fn repo_has_no_prompt_corruption_junk_files() {
  let bad = [
    "J::Bool",
    "J::Null",
    "match",
    "use",
    "}",
    "src/bin/faPassword",
  ];
  for p in bad {
    assert!(!Path::new(p).exists(), "JUNK_FILE_PRESENT {}", p);
  }
}
