---
status: accepted
---
# Own GPUI editor element

We build our own text element on GPUI text layout and the platform input handler. The Zed `editor` crate is large, tied to Zed, and hard to fit with inline Slot markers, Ghost opacity, and a paragraph gutter.
