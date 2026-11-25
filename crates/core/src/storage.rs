//! Save and open (ADR 0007): one `.md` file holds the active text, then all other state
//! as JSON in the Hamish comment at its end.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::slot::SlotNode;
use crate::tree::Tree;
use crate::{Document, LabCheck, LabMark, LabMarkId, Level, Overflow, Provenance, SlotId, Variant};

/// The Hamish comment schema this build writes and reads.
const SCHEMA_VERSION: u32 = 1;

/// Slot and Lab mark ids above this are refused, so new ids never overflow.
const MAX_ID: u64 = 1 << 32;

/// The start of the Hamish comment, up to its first whitespace.
const COMMENT_OPEN: &str = "<!-- hamish";

/// The end of an HTML comment.
const COMMENT_CLOSE: &str = "-->";

/// Between the Document text and the Hamish comment, so Markdown tools see a new block.
const SEPARATOR: &str = "\n\n";

/// Why a save or open failed. A failed open gives no Document; a failed save leaves the
/// `.md` file as it was.
#[derive(Debug)]
pub enum StorageError {
    Io {
        path: PathBuf,
        error: io::Error,
    },
    /// The `.md` file is not UTF-8 text.
    NotUtf8(PathBuf),
    /// The Hamish comment does not hold its JSON.
    BadHamishComment {
        path: PathBuf,
        reason: String,
    },
    /// The Hamish comment has a schema version this build does not know.
    UnknownVersion {
        path: PathBuf,
        version: u32,
    },
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
            Self::NotUtf8(path) => write!(f, "{}: not UTF-8 text", path.display()),
            Self::BadHamishComment { path, reason } => {
                write!(f, "{}: bad Hamish comment: {reason}", path.display())
            }
            Self::UnknownVersion { path, version } => {
                write!(
                    f,
                    "{}: unknown Hamish comment version {version}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { error, .. } => Some(error),
            Self::NotUtf8(_) | Self::BadHamishComment { .. } | Self::UnknownVersion { .. } => None,
        }
    }
}

/// An item of the Hamish comment that open could not put back: its text is not in the
/// Document text any more, or it does not fit the other items (ADR 0006, ADR 0007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dropped {
    /// A Slot, with the text of its Active variant.
    Slot {
        level: Level,
        text: String,
    },
    Ghost {
        text: String,
    },
    LabMark {
        check: LabCheck,
        text: String,
    },
}

/// The result of an open: the Document and the items that were dropped.
#[derive(Debug, Clone)]
pub struct Opened {
    pub document: Document,
    pub dropped: Vec<Dropped>,
}

impl Document {
    /// Saves the active text to `path`, then all other state as JSON in the Hamish comment
    /// at the end of the file. The edit history is not saved.
    ///
    /// # Errors
    /// [`StorageError::Io`] when the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), StorageError> {
        let json = serde_json::to_string_pretty(&SavedState::of(self))
            .map_err(|error| io_error(path, error.into()))?;
        let contents = format!(
            "{}{SEPARATOR}{COMMENT_OPEN}\n{}\n{COMMENT_CLOSE}\n",
            self.text(),
            escape_comment_ends(&json)
        );
        write_whole(path, &contents)
    }

    /// Opens the `.md` file `path`. Only its last HTML comment, when it is the Hamish
    /// comment, holds state; all other content is the Document text. With no Hamish
    /// comment, the Document is the plain text. When the text changed since the save, each
    /// top-level Slot, Ghost range, and Lab mark is found again by its text; the ones not
    /// found are dropped and returned. The Document has no edit history.
    ///
    /// # Errors
    /// [`StorageError`] when the file cannot be read, is not UTF-8, or its Hamish comment
    /// does not hold JSON of a known version.
    pub fn open(path: &Path) -> Result<Opened, StorageError> {
        let bytes = fs::read(path).map_err(|error| io_error(path, error))?;
        let contents =
            String::from_utf8(bytes).map_err(|_utf8| StorageError::NotUtf8(path.to_owned()))?;
        let Some((text, json)) = split_hamish_comment(&contents) else {
            return Ok(Opened {
                document: Document::new(&contents),
                dropped: Vec::new(),
            });
        };
        Ok(SavedState::parse(path, json)?.restore(text.to_owned()))
    }
}

/// Writes each `-` of `json` as a JSON escape, so no `--` can end the comment.
/// The schema has no negative numbers, so each `-` is in a string.
fn escape_comment_ends(json: &str) -> String {
    json.replace('-', "\\u002d")
}

/// The Document text and the JSON of the Hamish comment, when the last thing in
/// `contents` (past trailing whitespace) is that comment. `None` when it is not.
fn split_hamish_comment(contents: &str) -> Option<(&str, &str)> {
    let head = contents.trim_end().strip_suffix(COMMENT_CLOSE)?;
    let start = head.rfind("<!--")?;
    let json = head.get(start..)?.strip_prefix(COMMENT_OPEN)?;
    let tagged = json.starts_with(char::is_whitespace);
    if !tagged || json.contains(COMMENT_CLOSE) {
        return None;
    }
    let text = contents.get(..start)?;
    let text = text
        .strip_suffix(SEPARATOR)
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    Some((text, json))
}

fn io_error(path: &Path, error: io::Error) -> StorageError {
    StorageError::Io {
        path: path.to_owned(),
        error,
    }
}

/// Writes `contents` to a new file next to `path`, then renames it to `path`,
/// so a failed write leaves the old file whole.
fn write_whole(path: &Path, contents: &str) -> Result<(), StorageError> {
    let mut name = OsString::from(path);
    name.push(".saving");
    let saving = PathBuf::from(name);
    fs::write(&saving, contents).map_err(|error| io_error(&saving, error))?;
    fs::rename(&saving, path).map_err(|error| io_error(path, error))
}

fn sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Only the schema version, read before the rest so a newer schema gives a clear error.
#[derive(Deserialize)]
struct Header {
    version: u32,
}

/// The JSON of the Hamish comment. Ranges of top-level items are byte ranges of the
/// Document text; ranges nested in a Variant are byte ranges of that Variant's text.
#[derive(Serialize, Deserialize)]
struct SavedState {
    version: u32,
    /// SHA-256 of the Document text at save, lowercase hex.
    md_sha256: String,
    slots: Vec<SlotRecord>,
    ghosts: Vec<GhostRecord>,
    overflow: String,
    lab_marks: Vec<LabMarkRecord>,
}

#[derive(Serialize, Deserialize)]
struct SlotRecord {
    id: u64,
    level: Level,
    range: Range<usize>,
    active: usize,
    variants: Vec<VariantRecord>,
}

#[derive(Serialize, Deserialize)]
struct VariantRecord {
    text: String,
    provenance: Provenance,
    slots: Vec<SlotRecord>,
    ghosts: Vec<Range<usize>>,
}

/// A top-level Ghost range with its text, to find it again when the text changed.
#[derive(Serialize, Deserialize)]
struct GhostRecord {
    range: Range<usize>,
    text: String,
}

#[derive(Serialize, Deserialize)]
struct LabMarkRecord {
    id: u64,
    check: LabCheck,
    range: Range<usize>,
    text: String,
    note: Option<String>,
    fix: Option<String>,
}

impl SlotRecord {
    fn of(node: &SlotNode) -> Self {
        Self {
            id: node.id.0,
            level: node.level,
            range: node.range.clone(),
            active: node.active,
            variants: node.variants.iter().map(VariantRecord::of).collect(),
        }
    }

    /// The text of the Active variant. Empty when there is no such Variant.
    fn active_text(&self) -> &str {
        self.variants
            .get(self.active)
            .map_or("", |variant| variant.text.as_str())
    }
}

impl VariantRecord {
    fn of(variant: &Variant) -> Self {
        Self {
            text: variant.text.clone(),
            provenance: variant.provenance,
            slots: variant.tree.slots().iter().map(SlotRecord::of).collect(),
            ghosts: variant.tree.ghosts().to_vec(),
        }
    }
}

impl SavedState {
    fn of(document: &Document) -> Self {
        let text = document.text();
        let text_of = |range: &Range<usize>| text.get(range.clone()).unwrap_or_default().to_owned();
        Self {
            version: SCHEMA_VERSION,
            md_sha256: sha256(text),
            slots: document.tree().slots().iter().map(SlotRecord::of).collect(),
            ghosts: document
                .tree()
                .ghosts()
                .iter()
                .map(|range| GhostRecord {
                    range: range.clone(),
                    text: text_of(range),
                })
                .collect(),
            overflow: document.overflow().text().to_owned(),
            lab_marks: document
                .lab_marks()
                .iter()
                .map(|mark| LabMarkRecord {
                    id: mark.id.0,
                    check: mark.check,
                    range: mark.range.clone(),
                    text: text_of(&mark.range),
                    note: mark.note.clone(),
                    fix: mark.fix.clone(),
                })
                .collect(),
        }
    }

    fn parse(path: &Path, json: &str) -> Result<Self, StorageError> {
        let bad = |error: serde_json::Error| StorageError::BadHamishComment {
            path: path.to_owned(),
            reason: error.to_string(),
        };
        let header: Header = serde_json::from_str(json).map_err(bad)?;
        if header.version != SCHEMA_VERSION {
            return Err(StorageError::UnknownVersion {
                path: path.to_owned(),
                version: header.version,
            });
        }
        serde_json::from_str(json).map_err(bad)
    }

    fn restore(self, text: String) -> Opened {
        let mut restore = Restore {
            unchanged: sha256(&text) == self.md_sha256,
            text: &text,
            slot_ids: HashSet::new(),
            tree: Tree::default(),
            dropped: Vec::new(),
        };
        for record in self.slots {
            restore.top_slot(record);
        }
        for record in self.ghosts {
            restore.top_ghost(record);
        }
        let marks = restore.lab_marks(self.lab_marks);
        let Restore { tree, dropped, .. } = restore;
        Opened {
            document: Document::restored(text, tree, Overflow::new(self.overflow), marks),
            dropped,
        }
    }
}

/// Puts the Hamish comment items back on the Document text, one at a time.
struct Restore<'a> {
    /// The Document text is the one the Hamish comment was saved with.
    unchanged: bool,
    text: &'a str,
    slot_ids: HashSet<u64>,
    tree: Tree,
    dropped: Vec<Dropped>,
}

impl Restore<'_> {
    /// Where a top-level item with `anchor` text, saved at `saved`, may go, best first.
    /// Unchanged text: the saved range when it still holds `anchor`. Changed text: each
    /// place of `anchor` in the text, nearest to the saved start first (ADR 0007).
    /// An empty `anchor` has no place in a changed text.
    fn places(&self, anchor: &str, saved: &Range<usize>) -> Vec<Range<usize>> {
        if self.unchanged {
            let fits = self.text.get(saved.clone()) == Some(anchor);
            return fits.then(|| saved.clone()).into_iter().collect();
        }
        if anchor.is_empty() {
            return Vec::new();
        }
        let mut places: Vec<Range<usize>> = self
            .text
            .match_indices(anchor)
            .map(|(start, _)| start..start + anchor.len())
            .collect();
        places.sort_by_key(|place| place.start.abs_diff(saved.start));
        places
    }

    fn top_slot(&mut self, record: SlotRecord) {
        let level = record.level;
        let anchor = record.active_text().to_owned();
        let places = self.places(&anchor, &record.range);
        let free = places.into_iter().find(|place| self.tree.is_free(place));
        let node = free.and_then(|place| Some((place, self.node(record)?)));
        let Some((place, mut node)) = node else {
            self.dropped.push(Dropped::Slot {
                level,
                text: anchor,
            });
            return;
        };
        node.range = place;
        self.tree.add(node);
    }

    fn top_ghost(&mut self, record: GhostRecord) {
        let placed = self
            .places(&record.text, &record.range)
            .into_iter()
            .filter(|place| !place.is_empty())
            .find_map(|place| {
                let mut tree = self.tree.clone();
                tree.place_ghost(place).ok()?;
                Some(tree)
            });
        let Some(tree) = placed else {
            self.dropped.push(Dropped::Ghost { text: record.text });
            return;
        };
        self.tree = tree;
    }

    fn lab_marks(&mut self, records: Vec<LabMarkRecord>) -> Vec<LabMark> {
        let mut ids = HashSet::new();
        let mut marks = Vec::new();
        for record in records {
            let place = self.places(&record.text, &record.range).into_iter().next();
            let fresh = record.id < MAX_ID && ids.insert(record.id);
            let Some(range) = place.filter(|place| fresh && !place.is_empty()) else {
                self.dropped.push(Dropped::LabMark {
                    check: record.check,
                    text: record.text,
                });
                continue;
            };
            marks.push(LabMark {
                id: LabMarkId(record.id),
                check: record.check,
                range,
                note: record.note,
                fix: record.fix,
            });
        }
        marks.sort_by_key(|mark| mark.range.start);
        marks
    }

    /// The Slot of `record` with all its Variants. `None` when the record or one nested in
    /// it does not hold together: a used id, fewer than two Variants, no Active variant,
    /// or a nested item off its Variant text or out of strict nesting (ADR 0006).
    fn node(&mut self, record: SlotRecord) -> Option<SlotNode> {
        let fresh = record.id < MAX_ID && self.slot_ids.insert(record.id);
        let whole = record.variants.len() >= 2 && record.active < record.variants.len();
        if !fresh || !whole {
            return None;
        }
        let variants = record
            .variants
            .into_iter()
            .map(|variant| self.variant(variant))
            .collect::<Option<Vec<_>>>()?;
        Some(SlotNode {
            id: SlotId(record.id),
            level: record.level,
            range: record.range,
            variants,
            active: record.active,
        })
    }

    fn variant(&mut self, record: VariantRecord) -> Option<Variant> {
        let mut tree = Tree::default();
        for child in record.slots {
            let on_text = record.text.get(child.range.clone()) == Some(child.active_text());
            if !on_text || !tree.is_free(&child.range) {
                return None;
            }
            tree.add(self.node(child)?);
        }
        for ghost in record.ghosts {
            if ghost.is_empty() || record.text.get(ghost.clone()).is_none() {
                return None;
            }
            tree.place_ghost(ghost).ok()?;
        }
        Some(Variant {
            text: record.text,
            provenance: record.provenance,
            tree,
        })
    }
}
