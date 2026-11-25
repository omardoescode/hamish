---
status: accepted
---
# Nested Slots with strict nesting

Each Variant of a Paragraph Slot may hold its own Word and Sentence Slots, so the Document model is a tree, not flat ranges. Any two Slots or Ghost ranges are disjoint or one fully contains the other; an operation that would make a partial overlap is refused. Nesting costs model, persistence, and Marker complexity; we accept it so paragraph rewrites keep their own word choices.
