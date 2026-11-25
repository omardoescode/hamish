---
status: accepted
---
# Pluggable AI adapter; `claude -p` dev backend

All AI calls go through one Rust adapter trait. The development backend runs `claude -p` as a subprocess on the user subscription. `ANTHROPIC_API_KEY` must be unset, or the call bills the API. The production provider is open.
