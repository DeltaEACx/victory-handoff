//! CPU-only guest present tags, joined by frame number to RGB grid logs.

use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Mutex, OnceLock};

const LIMIT: u64 = 768;
const SLOTS: usize = 128;
static TRACE: Mutex<Trace> = Mutex::new(Trace { entries: [None; SLOTS], calls: 0 });

pub(super) fn active() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_PRESENT_TOKEN_TRACE").is_some_and(|v| v == "1"))
        && super::FRAMES.load(Relaxed) < LIMIT
}

fn known(raw: Option<u64>) -> Option<u64> {
    raw.filter(|&value| value != 0 && value != u64::MAX)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExecuteTag {
    token: u64,
    seq: u64,
    queue: usize,
    lists: u32,
    flags: u32,
}

struct Trace {
    entries: [Option<ExecuteTag>; SLOTS],
    calls: u64,
}

impl Trace {
    fn execute(&mut self, queue: usize, token: u64, lists: u32, flags: u32) {
        self.calls += 1;
        if let Some(token) = known(Some(token)) {
            self.entries[token as usize % SLOTS] = Some(ExecuteTag { token, seq: self.calls, queue, lists, flags });
        }
    }

    fn at_entry(&self, queue: usize, raw: Option<u64>, planes: u32) -> Entry {
        let token = known(raw);
        let matching_call = token.and_then(|tag| {
            self.entries[tag as usize % SLOTS].filter(|entry| entry.token == tag)
        });
        Entry { queue, raw, token, planes, matching_call }
    }
}

pub(super) fn execute(queue: usize, token: u64, lists: u32, flags: u32) {
    super::lock(&TRACE).execute(queue, token, lists, flags);
}

pub(super) fn at_entry(queue: usize, raw: Option<u64>, planes: u32) -> Entry {
    super::lock(&TRACE).at_entry(queue, raw, planes)
}

pub(super) struct Entry {
    queue: usize,
    raw: Option<u64>,
    token: Option<u64>,
    planes: u32,
    matching_call: Option<ExecuteTag>,
}

impl Entry {
    pub(super) fn report(self, frame: u64, resource: usize, resources: u32) {
        if frame > LIMIT {
            return;
        }
        // This snapshot predates draining queues. A match means the call
        // happened, not that its lists wrote the source or completed.
        let call = self.matching_call;
        tracing::info!(
            frame,
            queue = ?(self.queue as *const ()),
            token_raw = ?self.raw,
            token = ?self.token,
            planes = self.planes,
            resources,
            source = ?(resource as *const ()),
            matching_execute_x_seq = ?call.map(|tag| tag.seq),
            matching_execute_x_queue = ?call.map(|tag| tag.queue as *const ()),
            matching_execute_x_lists = ?call.map(|tag| tag.lists),
            matching_execute_x_flags = ?call.map(|tag| tag.flags),
            "PresentX guest token/source (ExecuteX call tag, not resource writer)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_keeps_prior_exact_valid_tag() {
        let mut trace = Trace { entries: [None; SLOTS], calls: 0 };
        trace.execute(1, 40, 2, 7);
        trace.execute(2, 41, 1, 0);
        let prior = trace.at_entry(2, Some(40), 1);
        trace.execute(2, 40, 5, 0);
        assert_eq!(
            prior.matching_call,
            Some(ExecuteTag { token: 40, seq: 1, queue: 1, lists: 2, flags: 7 })
        );
        assert_eq!(trace.at_entry(2, Some(40), 1).matching_call.unwrap().seq, 3);
        trace.execute(3, 40 + SLOTS as u64, 1, 0);
        assert!(trace.at_entry(2, Some(40), 1).matching_call.is_none());
    }

    #[test]
    fn missing_and_sentinel_tags_remain_unknown() {
        let mut trace = Trace { entries: [None; SLOTS], calls: 0 };
        trace.execute(1, 0, 1, 0);
        trace.execute(1, u64::MAX, 1, 0);
        for raw in [None, Some(0), Some(u64::MAX)] {
            let entry = trace.at_entry(1, raw, 0);
            assert_eq!(entry.raw, raw);
            assert_eq!(entry.token, None);
            assert_eq!(entry.matching_call, None);
        }
    }
}
