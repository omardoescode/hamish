---
status: accepted
supersedes: 0003
---
# Storage: one .md file with a ka-teb comment

A Document saves as one `.md` file: the active text, then a blank line, then the ka-teb comment as the last thing in the file:

```
<!-- ka-teb
{json}
-->
```

The JSON holds Slots, Variants, Provenance, Ghost ranges, Overflow, Lab marks, a schema version, and a hash of the active text. Each `-` in the JSON is written as the JSON escape `\u002d`, so no text in it can end the comment. The file is self-contained: it moves, copies, and syncs as one file. Markdown tools do not show the comment.

## Reading

Only the last HTML comment of the file counts, and only when it carries the `ka-teb` tag and nothing but whitespace follows it. All other content (other HTML comments, a comment with no tag) is Document text and shows in the editor. The ka-teb comment is never Document text: the editor, the Counter, and AI requests do not see it. A file with no ka-teb comment opens as plain text with no Slots.

## Consequences

Sidecar JSON files of ADR 0003 are not read and not deleted. When the text changed outside the app, open re-anchors each Slot, Ghost range, and Lab mark by its text, keeps matches, drops the rest, and tells the writer what was dropped. Another tool can delete or move the comment; then the state is lost or the comment is read as text.
