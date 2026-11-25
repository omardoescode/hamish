use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Once;
use std::time::{Duration, Instant};
use std::{env, fs};

use ka_teb_core::{AiAdapter, AiError, AiRequest, ClaudeBackend, Document, Level, RangeError};

/// A `claude` stand-in. The selection picks its behavior; by default it answers with the
/// args it got, the API key it saw, and the prompt, as three Variant texts.
const STUB: &str = r#"#!/bin/sh
prompt=$(cat)
case "$prompt" in
  *STUB_EXIT*) echo "stub failed" >&2; exit 3 ;;
  *STUB_GARBAGE*) echo "not json"; exit 0 ;;
  *STUB_DECLINED*) echo '{"type":"result","is_error":false,"result":"I will not."}'; exit 0 ;;
  *STUB_SLOW*) exec sleep 10 ;;
  *STUB_PADDED*) echo '{"is_error":false,"structured_output":{"variants":["  dog\n","fox"]}}'; exit 0 ;;
esac
esc() { printf '%s' "$1" | tr '\n' ' ' | sed 's/\\/\\\\/g; s/"/\\"/g'; }
args=""
for arg in "$@"; do args="$args \"$(esc "$arg")\""; done
args=$(printf '%s' "$args" | sed 's/^ //; s/" "/", "/g')
key="${ANTHROPIC_API_KEY-unset}"
printf '{"type":"result","is_error":false,"result":"","structured_output":{"variants":["%s","%s","%s"]}}' \
  "$(esc "$args")" "$(esc "$key")" "$(esc "$prompt")"
"#;

static STUB_WRITTEN: Once = Once::new();

/// A backend whose environment has the stub first on PATH and an API key set.
fn backend() -> std::io::Result<ClaudeBackend> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("claude-stub");
    let mut written = Ok(());
    STUB_WRITTEN.call_once(|| written = write_stub(&dir));
    written?;
    let path = format!("{}:{}", dir.display(), env::var("PATH").unwrap_or_default());
    let environment = vec![
        ("PATH".into(), path.into()),
        (
            "ANTHROPIC_API_KEY".into(),
            "sk-must-not-reach-claude".into(),
        ),
    ];
    Ok(ClaudeBackend::new(environment).with_timeout(Duration::from_secs(5)))
}

fn write_stub(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let stub = dir.join("claude");
    fs::write(&stub, STUB)?;
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755))
}

/// A request whose selection is all of `text`.
fn request(text: &str) -> Result<AiRequest, RangeError> {
    Document::new(text).ai_request(0..text.len(), Level::Sentence)
}

#[test]
fn backend_runs_claude_print_without_the_api_key_and_sends_the_request() {
    let request = Document::new("The cat sat.")
        .ai_request(4..7, Level::Word)
        .unwrap();

    let answer = backend().unwrap().variants(&request).unwrap();

    assert_eq!(answer.len(), 3);
    assert_eq!(
        answer[0],
        r#""-p", "--output-format", "json", "--json-schema", "{\"type\":\"object\",\"properties\":{\"variants\":{\"type\":\"array\",\"items\":{\"type\":\"string\"},\"minItems\":1}},\"required\":[\"variants\"]}", "--tools", "", "--no-session-persistence""#
    );
    assert_eq!(answer[1], "unset");
    assert!(answer[2].contains("cat"), "prompt: {}", answer[2]);
    assert!(answer[2].contains("The cat sat."), "prompt: {}", answer[2]);
}

#[test]
fn backend_trims_variant_texts() {
    assert_eq!(
        backend()
            .unwrap()
            .variants(&request("STUB_PADDED").unwrap()),
        Ok(vec!["dog".to_owned(), "fox".to_owned()])
    );
}

#[test]
fn backend_reports_a_non_zero_exit_with_its_stderr() {
    assert_eq!(
        backend().unwrap().variants(&request("STUB_EXIT").unwrap()),
        Err(AiError::Exit {
            code: Some(3),
            message: "stub failed".to_owned()
        })
    );
}

#[test]
fn backend_reports_output_that_is_not_json() {
    assert!(matches!(
        backend()
            .unwrap()
            .variants(&request("STUB_GARBAGE").unwrap()),
        Err(AiError::BadOutput(_))
    ));
}

#[test]
fn backend_reports_an_answer_without_variants_as_declined() {
    assert_eq!(
        backend()
            .unwrap()
            .variants(&request("STUB_DECLINED").unwrap()),
        Err(AiError::Declined("I will not.".to_owned()))
    );
}

#[test]
fn backend_stops_a_call_that_runs_past_the_timeout() {
    let backend = backend().unwrap().with_timeout(Duration::from_millis(200));
    let start = Instant::now();

    let answer = backend.variants(&request("STUB_SLOW").unwrap());

    assert_eq!(answer, Err(AiError::Timeout(Duration::from_millis(200))));
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn backend_reports_a_missing_claude_program() {
    let empty = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-claude");
    fs::create_dir_all(&empty).unwrap();
    let backend = ClaudeBackend::new(vec![("PATH".into(), empty.into())]);

    assert!(matches!(
        backend.variants(&request("The cat sat.").unwrap()),
        Err(AiError::Start(_))
    ));
}
