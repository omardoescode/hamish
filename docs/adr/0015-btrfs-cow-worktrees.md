---
status: accepted
---
# Agent worktrees use btrfs CoW

A WorktreeCreate hook makes agent worktrees and copies `target/` with `cp --reflink=always` (btrfs CoW). This saves disk and avoids cold Rust builds.
