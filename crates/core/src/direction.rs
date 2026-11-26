//! Direction: which way a step goes, for jumps, row moves and setting steps.

/// Which way a step goes: forward (towards the text end, the next row, a higher value) or
/// back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Back,
}
