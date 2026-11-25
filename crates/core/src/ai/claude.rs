use std::ffi::OsString;
use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::{AiAdapter, AiError, AiRequest};
use crate::Level;

/// Asks for an object with a non-empty list of Variant texts.
const SCHEMA: &str = r#"{"type":"object","properties":{"variants":{"type":"array","items":{"type":"string"},"minItems":1}},"required":["variants"]}"#;

/// With one of these set, `claude` bills the API, not the user subscription (ADR 0004).
const API_CREDENTIALS: [&str; 2] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

const POLL: Duration = Duration::from_millis(10);

/// Development AI backend: runs `claude -p` as a subprocess (ADR 0004).
///
/// The child gets the given environment without `ANTHROPIC_API_KEY` and
/// `ANTHROPIC_AUTH_TOKEN`, and finds `claude`
/// on its `PATH`.
#[derive(Debug, Clone)]
pub struct ClaudeBackend {
    environment: Vec<(OsString, OsString)>,
    timeout: Duration,
}

impl ClaudeBackend {
    /// A backend whose child process gets `environment` (less the API credentials). Timeout: 2 minutes.
    #[must_use]
    pub fn new(environment: Vec<(OsString, OsString)>) -> Self {
        Self {
            environment,
            timeout: Duration::from_secs(120),
        }
    }

    /// Stops the call and gives [`AiError::Timeout`] after `timeout`.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn spawn(&self) -> std::io::Result<Child> {
        #[expect(
            clippy::disallowed_methods,
            reason = "the claude -p AI backend runs a subprocess (ADR 0004)"
        )]
        let mut command = Command::new("claude");
        command
            .args(["-p", "--output-format", "json", "--json-schema", SCHEMA])
            .args(["--tools", "", "--no-session-persistence"])
            .env_clear()
            .envs(
                self.environment
                    .iter()
                    .filter(|(key, _)| !API_CREDENTIALS.iter().any(|credential| key == credential))
                    .map(|(key, value)| (key, value)),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.spawn()
    }

    /// Runs `claude` with `prompt` on stdin. Returns its exit status, stdout, and stderr.
    fn run(&self, prompt: String) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), AiError> {
        let mut child = self
            .spawn()
            .map_err(|error| AiError::Start(error.to_string()))?;
        let stdin = child.stdin.take();
        thread::spawn(move || stdin.map(|mut stdin| stdin.write_all(prompt.as_bytes())));
        let stdout = read_all(child.stdout.take());
        let stderr = read_all(child.stderr.take());
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    stop(&mut child);
                    return Err(AiError::Timeout(self.timeout));
                }
                Ok(None) => thread::sleep(POLL),
                Err(error) => {
                    stop(&mut child);
                    return Err(AiError::Start(error.to_string()));
                }
            }
        };
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        Ok((status, stdout, stderr))
    }
}

impl Default for ClaudeBackend {
    /// A backend whose child gets this process's environment (less the API credentials).
    fn default() -> Self {
        Self::new(std::env::vars_os().collect())
    }
}

impl AiAdapter for ClaudeBackend {
    fn variants(&self, request: &AiRequest) -> Result<Vec<String>, AiError> {
        let (status, stdout, stderr) = self.run(prompt(request))?;
        if status.success() {
            return parse(&stdout);
        }
        let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
        let message = if stderr.is_empty() {
            String::from_utf8_lossy(&stdout).trim().to_owned()
        } else {
            stderr
        };
        Err(AiError::Exit {
            code: status.code(),
            message,
        })
    }
}

/// The `--output-format json` result of `claude -p`.
#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    result: Option<String>,
    structured_output: Option<Answer>,
}

#[derive(Deserialize)]
struct Answer {
    variants: Vec<String>,
}

fn parse(stdout: &[u8]) -> Result<Vec<String>, AiError> {
    let envelope: Envelope =
        serde_json::from_slice(stdout).map_err(|error| AiError::BadOutput(error.to_string()))?;
    let answer = envelope
        .structured_output
        .filter(|_| !envelope.is_error)
        .ok_or_else(|| AiError::Declined(envelope.result.unwrap_or_default()))?;
    let variants: Vec<String> = answer
        .variants
        .iter()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .collect();
    if variants.is_empty() {
        return Err(AiError::BadOutput("no Variant text".to_owned()));
    }
    Ok(variants)
}

fn prompt(request: &AiRequest) -> String {
    let unit = match request.level() {
        Level::Word => "word or phrase",
        Level::Sentence => "sentence",
        Level::Paragraph => "paragraph",
    };
    format!(
        "You help a writer revise a draft. Write 3 alternatives for the selected {unit}. \
         Each alternative must fit in place of the selection in the document, keep its meaning, \
         and differ from it in wording. Answer only with the alternatives.\n\n\
         <document>\n{}\n</document>\n\n<selection>\n{}\n</selection>\n",
        request.context(),
        request.selection()
    )
}

fn read_all(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut bytes).ok();
        }
        bytes
    })
}

/// Kills `child` and reaps it. A child that already ended needs neither.
fn stop(child: &mut Child) {
    if child.kill().is_ok() {
        child.wait().ok();
    }
}
