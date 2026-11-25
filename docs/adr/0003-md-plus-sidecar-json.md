---
status: accepted
---
# Storage: plain .md + sidecar JSON

A Document saves as a plain `.md` file (active text) plus a sidecar JSON file (Slots, Variants, Provenance, Ghost ranges, Overflow). The `.md` stays readable and editable in any tool, at the cost of two files that can drift apart.

## Consequences

The sidecar records a hash of the `.md`. When the `.md` changed outside the app, open re-anchors each Slot and Ghost range by its text, keeps matches, drops the rest, and tells the writer what was dropped.
