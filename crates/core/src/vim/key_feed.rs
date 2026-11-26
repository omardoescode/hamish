//! The keys typed and not yet read: the maps of a hamishrc act on them before the Vim mode
//! grammar reads them.

use std::collections::VecDeque;

use super::hamishrc::{Hamishrc, KeyMap, MapMode, MapTarget};
use super::{Leader, Vim, VimAction, VimKey, VimMode};

/// How many maps in a row a key can go through (Vim `maxmapdepth`).
pub const MAX_MAP_DEPTH: usize = 1000;

/// What the key feed gives the Editor next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fed {
    /// Give this key to [`Vim::key`].
    Key(VimKey),
    /// Run this Hamish action.
    Action(VimAction),
    /// A recursive map went through more than [`MAX_MAP_DEPTH`] maps in a row. The keys not
    /// yet read are gone.
    TooDeep,
}

/// One key not yet read.
#[derive(Debug, Clone, Copy)]
struct Queued {
    key: VimKey,
    /// The maps apply to the key: a typed key, or a key of a recursive map.
    recursive: bool,
    /// The maps the key came through.
    depth: usize,
}

/// The keys typed and not yet read, oldest first.
#[derive(Debug, Clone, Default)]
pub(crate) struct KeyFeed {
    queue: VecDeque<Queued>,
}

impl Vim {
    /// Adds a typed key to the keys not yet read. [`Vim::next_key`] gives them out.
    pub fn press(&mut self, key: VimKey) {
        self.feed.queue.push_back(Queued {
            key,
            recursive: true,
            depth: 0,
        });
    }

    /// No typed key waits to be read.
    #[must_use]
    pub fn feed_is_empty(&self) -> bool {
        self.feed.queue.is_empty()
    }

    /// The next thing to do for the keys not yet read, with the maps of `rc` in the current
    /// mode, and the leader of `rc`. `None` when no key waits, or when the keys can still
    /// grow into a longer map (Vim mode waits with no time limit). Give a `Key` to
    /// [`Vim::key`] before the next call: the mode it gives selects the maps.
    pub fn next_key(&mut self, rc: &Hamishrc) -> Option<Fed> {
        self.feed_next(rc, true)
    }

    /// As [`Vim::next_key`], but keys that can still grow into a longer map do not wait:
    /// the longest map they hold applies, else they go to Vim mode as typed. Vim does the
    /// same when a key comes that a map cannot take, such as Escape or Tab.
    pub fn flush_key(&mut self, rc: &Hamishrc) -> Option<Fed> {
        self.feed_next(rc, false)
    }

    /// `wait`: keys that can still grow into a longer map give `None`.
    fn feed_next(&mut self, rc: &Hamishrc, wait: bool) -> Option<Fed> {
        self.leader = Leader(rc.leader());
        loop {
            let front = *self.feed.queue.front()?;
            let mode = self.map_mode().filter(|_| front.recursive);
            let Some(mode) = mode else {
                self.feed.queue.pop_front();
                return Some(Fed::Key(front.key));
            };
            let typed: Vec<VimKey> = self
                .feed
                .queue
                .iter()
                .take_while(|queued| queued.recursive)
                .map(|queued| queued.key)
                .collect();
            let all_typed = typed.len() == self.feed.queue.len();
            if wait
                && all_typed
                && rc
                    .maps(mode)
                    .any(|map| map.keys.len() > typed.len() && map.keys.starts_with(&typed))
            {
                return None;
            }
            let found = rc
                .maps(mode)
                .filter(|map| typed.starts_with(&map.keys))
                .max_by_key(|map| map.keys.len());
            let Some(map) = found else {
                self.feed.queue.pop_front();
                return Some(Fed::Key(front.key));
            };
            if let Some(fed) = self.feed.expand(map) {
                return Some(fed);
            }
        }
    }

    /// The maps that apply now; `None` while an operator waits for its motion.
    fn map_mode(&self) -> Option<MapMode> {
        match self.mode {
            VimMode::Normal if self.operator.is_some() => None,
            VimMode::Normal => Some(MapMode::Normal),
            VimMode::Insert => Some(MapMode::Insert),
            VimMode::Visual | VimMode::VisualLine => Some(MapMode::Visual),
        }
    }
}

impl KeyFeed {
    /// Puts the target of `map` in place of its keys at the front. Gives an action, or
    /// `TooDeep`; `None` after target keys.
    fn expand(&mut self, map: &KeyMap) -> Option<Fed> {
        let depth = self
            .queue
            .drain(..map.keys.len())
            .map(|queued| queued.depth)
            .max()
            .unwrap_or_default()
            + 1;
        if depth > MAX_MAP_DEPTH {
            self.queue.clear();
            return Some(Fed::TooDeep);
        }
        let keys = match &map.target {
            MapTarget::Action(action) => return Some(Fed::Action(*action)),
            MapTarget::Keys(keys) => keys,
        };
        // Vim: when the target starts with the keys of the map, those keys do not map again.
        let own_start = if keys.starts_with(&map.keys) {
            map.keys.len()
        } else {
            0
        };
        for (index, key) in keys.iter().enumerate().rev() {
            self.queue.push_front(Queued {
                key: *key,
                recursive: map.recursive && index >= own_start,
                depth,
            });
        }
        None
    }
}
