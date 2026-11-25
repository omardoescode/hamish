---
status: accepted
---
# `core` crate has no GPUI dependency

The workspace has a `core` crate (all testable logic, no gpui) and an `app` crate (GPUI views, thin). Logic tickets land in `core` before UI tickets.
