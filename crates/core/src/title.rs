//! The Document title the window shows: the file name, the first line, or "Untitled".

use std::path::Path;

use crate::{Document, markdown};

/// The title of `document` saved at `path`: the file name without `.md`. With no
/// `path`, the Document title (its first line without Markdown marks); with no text
/// left, "Untitled".
#[must_use]
pub fn document_title(path: Option<&Path>, document: &Document) -> String {
    path.and_then(Path::file_stem)
        .map(|stem| stem.to_string_lossy().into_owned())
        .or_else(|| {
            document
                .title()
                .map(|title| markdown::plain_text(title).trim().to_owned())
                .filter(|title| !title.is_empty())
        })
        .unwrap_or_else(|| "Untitled".to_owned())
}
