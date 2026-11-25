---
status: accepted
---
# Re-anchor sidecar on drift

The sidecar records a hash of the `.md`. If the `.md` changed outside the app, open re-anchors each Slot and Ghost range by its text, keeps matches, drops the rest, and tells the writer what was dropped.
