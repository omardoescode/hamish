//! The `claude -p` AI backend (ADR 0004): prompts, the subprocess, and its structured answer.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::ops::Range;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::{AiAdapter, AiError, AiRequest, AiTrigger, CancelToken};
use crate::{Counter, LabCheck, LabFinding, LabRequest, Level, TrimRequest};

/// Asks for an object with a non-empty list of Variant texts.
const VARIANTS_SCHEMA: &str = r#"{"type":"object","properties":{"variants":{"type":"array","items":{"type":"string"},"minItems":1}},"required":["variants"]}"#;

/// Asks for an object with a list of marks, each an exact quote of the document.
/// Models are not reliable with offsets, so the backend finds each quote in the text.
const LAB_SCHEMA: &str = r#"{"type":"object","properties":{"marks":{"type":"array","items":{"type":"object","properties":{"quote":{"type":"string"},"note":{"type":"string"},"fix":{"type":"string"}},"required":["quote"]}}},"required":["marks"]}"#;

/// Asks for an object with a list of cuts, each an exact quote of the document.
const TRIM_SCHEMA: &str = r#"{"type":"object","properties":{"cuts":{"type":"array","items":{"type":"string"}}},"required":["cuts"]}"#;

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

    fn spawn(&self, schema: &str) -> std::io::Result<Child> {
        #[expect(
            clippy::disallowed_methods,
            reason = "the claude -p AI backend runs a subprocess (ADR 0004)"
        )]
        let mut command = Command::new("claude");
        command
            .args(["-p", "--output-format", "json", "--json-schema", schema])
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

    /// Runs `claude` with `prompt` on stdin and the answer shape `schema`, and reads
    /// its structured answer.
    fn ask<T: DeserializeOwned>(
        &self,
        prompt: String,
        schema: &str,
        cancel: &CancelToken,
    ) -> Result<T, AiError> {
        let (status, stdout, stderr) = self.run(prompt, schema, cancel)?;
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

    /// Runs `claude` with `prompt` on stdin. Returns its exit status, stdout, and stderr.
    /// A timeout or a cancel kills the child.
    fn run(
        &self,
        prompt: String,
        schema: &str,
        cancel: &CancelToken,
    ) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), AiError> {
        if cancel.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        let mut child = self
            .spawn(schema)
            .map_err(|error| AiError::Start(error.to_string()))?;
        let stdin = child.stdin.take();
        thread::spawn(move || stdin.map(|mut stdin| stdin.write_all(prompt.as_bytes())));
        let stdout = read_all(child.stdout.take());
        let stderr = read_all(child.stderr.take());
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if cancel.is_cancelled() => {
                    stop(&mut child);
                    return Err(AiError::Cancelled);
                }
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
    fn variants(&self, request: &AiRequest, cancel: &CancelToken) -> Result<Vec<String>, AiError> {
        let answer: Variants = self.ask(prompt(request), VARIANTS_SCHEMA, cancel)?;
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

    fn lab_check(
        &self,
        request: &LabRequest,
        cancel: &CancelToken,
    ) -> Result<Vec<LabFinding>, AiError> {
        let answer: Marks = self.ask(lab_prompt(request), LAB_SCHEMA, cancel)?;
        Ok(findings(request.text(), answer.marks))
    }

    fn trim(
        &self,
        request: &TrimRequest,
        cancel: &CancelToken,
    ) -> Result<Vec<Range<usize>>, AiError> {
        let answer: Cuts = self.ask(trim_prompt(request), TRIM_SCHEMA, cancel)?;
        Ok(cut_ranges(request.text(), answer.cuts))
    }
}

/// The `--output-format json` result of `claude -p`.
#[derive(Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    result: Option<String>,
    structured_output: Option<T>,
}

#[derive(Deserialize)]
struct Variants {
    variants: Vec<String>,
}

#[derive(Deserialize)]
struct Marks {
    marks: Vec<QuotedMark>,
}

#[derive(Deserialize)]
struct Cuts {
    cuts: Vec<String>,
}

#[derive(Deserialize)]
struct QuotedMark {
    quote: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    fix: Option<String>,
}

fn parse<T: DeserializeOwned>(stdout: &[u8]) -> Result<T, AiError> {
    let envelope: Envelope<T> =
        serde_json::from_slice(stdout).map_err(|error| AiError::BadOutput(error.to_string()))?;
    envelope
        .structured_output
        .filter(|_| !envelope.is_error)
        .ok_or_else(|| AiError::Declined(envelope.result.unwrap_or_default()))
}

/// Finds each quote in `text`: a repeated quote takes its next place. A quote that is
/// empty or not in the text is dropped. A blank note becomes none.
fn findings(text: &str, quotes: Vec<QuotedMark>) -> Vec<LabFinding> {
    let mut next_search: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for QuotedMark { quote, note, fix } in quotes {
        let from = next_search.get(&quote).copied().unwrap_or(0);
        let found = text
            .get(from..)
            .and_then(|rest| rest.find(&quote))
            .filter(|_| !quote.is_empty());
        let Some(at) = found else {
            continue;
        };
        let range = from + at..from + at + quote.len();
        next_search.insert(quote, range.end);
        let note = note
            .map(|note| note.trim().to_owned())
            .filter(|note| !note.is_empty());
        out.push(LabFinding { range, note, fix });
    }
    out
}

/// Finds each quote in `text` at its next place that starts and ends on a word edge, so a
/// cut never takes part of a word. A quote that is empty or not found is dropped.
fn cut_ranges(text: &str, quotes: Vec<String>) -> Vec<Range<usize>> {
    let mut next_search: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for quote in quotes {
        let from = next_search.get(&quote).copied().unwrap_or(0);
        let Some(range) = find_on_word_edges(text, &quote, from) else {
            continue;
        };
        next_search.insert(quote, range.end);
        out.push(range);
    }
    out
}

fn find_on_word_edges(text: &str, quote: &str, from: usize) -> Option<Range<usize>> {
    if quote.is_empty() {
        return None;
    }
    text.get(from..)?
        .match_indices(quote)
        .map(|(at, _)| from + at..from + at + quote.len())
        .find(|range| {
            let before = text
                .get(..range.start)
                .and_then(|head| head.chars().next_back());
            let after = text.get(range.end..).and_then(|tail| tail.chars().next());
            word_edge(before, quote.chars().next()) && word_edge(quote.chars().next_back(), after)
        })
}

/// Whether a cut may fall between `left` and `right`: not inside a word.
fn word_edge(left: Option<char>, right: Option<char>) -> bool {
    !matches!((left, right), (Some(left), Some(right)) if left.is_alphanumeric() && right.is_alphanumeric())
}

fn trim_prompt(request: &TrimRequest) -> String {
    let percent = request.level().target_percent();
    let words = Counter::of(request.text()).words * usize::from(percent) / 100;
    format!(
        "You help a writer revise a draft. Mark words to cut so the document is about \
         {percent}% shorter: about {words} words. You only cut; you never rewrite, reorder, \
         or add text. Cut whole words, phrases, or sentences, so the text that stays still \
         reads well. Copy each cut exactly from the document, in document order.\n\n\
         <document>\n{}\n</document>\n",
        request.text()
    )
}

fn lab_prompt(request: &LabRequest) -> String {
    let task = match request.check() {
        LabCheck::PunctuationTypos => {
            "Find punctuation errors and typos. Quote the smallest span that holds each error, \
             and give the corrected span as the fix."
        }
        LabCheck::WeakestSentences => {
            "Find the weakest sentences: the ones that add the least or say it most weakly. \
             Quote each whole sentence. Give no fix."
        }
        LabCheck::LongSentences => {
            "Find sentences that run long. Quote each whole sentence. Give no fix."
        }
        LabCheck::ConvolutedSentences => {
            "Find convoluted sentences: ones that are hard to follow on a first read. \
             Quote each whole sentence. Give no fix."
        }
        LabCheck::OffToneWords => {
            "Find words that do not fit the tone of the document. \
             Quote each word or short phrase. Give no fix."
        }
        LabCheck::HedgesFiller => {
            "Find hedges and filler: words that weaken a claim or add nothing. \
             Quote each word or phrase. Give no fix."
        }
    };
    format!(
        "You help a writer revise a draft. You only mark text; you never rewrite it. {task} \
         Copy each quote exactly from the document. Give a short note on why it is marked. \
         Answer with an empty list when nothing matches.\n\n\
         <document>\n{}\n</document>\n",
        request.text()
    )
}

fn prompt(request: &AiRequest) -> String {
    let unit = match request.level() {
        Level::Word => "word or phrase",
        Level::Sentence => "sentence",
        Level::Paragraph => "paragraph",
    };
    let count = match request.trigger() {
        AiTrigger::ContextMenu => 3,
        AiTrigger::QuickFill => 5,
    };
    let known: String = request
        .variants()
        .iter()
        .map(|variant| ["<variant>", variant, "</variant>\n"].concat())
        .collect();
    format!(
        "You help a writer revise a draft. Write {count} alternatives for the selected {unit}. \
         Each alternative must fit in place of the selection in its sentence and paragraph, \
         in grammar and meaning, and differ from it in wording. Phrases are fine. Repeat none \
         of the known variants. Answer only with the alternatives.\n\n\
         <paragraph>\n{}\n</paragraph>\n\n<sentence>\n{}\n</sentence>\n\n\
         <selection>\n{}\n</selection>\n\n<known_variants>\n{known}</known_variants>\n",
        request.paragraph(),
        request.sentence(),
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
