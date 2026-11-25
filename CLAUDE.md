# Ka-teb: Agent Instructions

## GitHub Issues workflow
For work that is a GitHub issue (skip for a small fix the user asks for directly):
- Pick an unblocked issue (no open `blocked_by`), assign yourself (`gh issue edit <n> --add-assignee @me`) before you write code.
- When the PR opens: comment a summary on the issue.
- Order issues with native blocking edges (blocked_by).

## Conflict prevention
- Before you touch a file, check for an open assigned issue in the same area. If one exists, pick other work.
- Changes to the sidecar JSON schema or the core document model are serialized: 1 in progress at a time.
- If the task needs changes to >3 files outside its scope, stop and ask the user.

## Session rules
- Code rules: read CODING_STANDARDS.md before you write code.
- Domain names: use the glossary (CONTEXT.md, canonical) for types, modules, tests.
- Decisions: docs/adr/README.md index first.
- Tests: use the tdd skill.
- Shell is zsh: quote globs. Use doas, never sudo. Python tools: uv.
- Questions: ask the user, never guess.
- Commits carry no Co-Authored-By trailer.

## Agent skills
(/setup-matt-pocock-skills fills this block)
