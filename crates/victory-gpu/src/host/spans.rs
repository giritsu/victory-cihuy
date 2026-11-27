//! Diagnostics: how long the card itself works on our submissions, by its own
//! timestamps (`VICTORY_CARD_SPANS=1`), split across `card_sync`, `card_steps`,
//! `title`, and `card_check` phases. The clock of the frame line
//! (`card_clock_line`) counts from the call that sends a submission to the
//! moment a thread of ours wakes behind its last signal: the time the system
//! takes to hand a submission to the card and to report it done is in it, 40
//! times a frame.

use std::sync::Mutex;

use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

use super::Host;

/// Submissions/syncs stamped before the first stamps are read and used again.
const SPANS: usize = 512;
const TIMESTAMPS_PER_SPAN: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SpanKind {
    #[default]
    Empty,
    CardSync,
    Submission {
        has_steps: bool,
        has_check: bool,
    },
}

/// Pre-recorded timestamp lists bracketing one submission's GPU phases.
pub(super) struct SubSpans {
    pub(super) front: ID3D12CommandList,
    pub(super) after_steps: Option<ID3D12CommandList>,
    pub(super) before_check: Option<ID3D12CommandList>,
    pub(super) behind: ID3D12CommandList,
}

/// The stamps of one queue's submissions.
pub(super) struct Spans {
    _allocator: ID3D12CommandAllocator,
    _heap: ID3D12QueryHeap,
    _back: ID3D12Resource,
    /// The mapped results, `TIMESTAMPS_PER_SPAN` u64s per slot.
    data: usize,
    lists: Vec<ID3D12CommandList>,
    kinds: Vec<SpanKind>,
    labels: Vec<String>,
    ticks_per_ms: f64,
    next: usize,
    sums: Sums,
}

// SAFETY: free-threaded objects and a mapping only read behind a lock.
unsafe impl Send for Spans {}

/// What the stamps read so far add up to.
#[derive(Debug, Default, Clone, PartialEq)]
struct Sums {
    /// Submissions, ticks the card worked on them, ticks between them.
    count: u64,
    busy: u64,
    between: u64,
    /// Ticks spent inside each phase (best-effort additive split over `count`).
    card_sync: u64,
    card_steps: u64,
    title: u64,
    card_check: u64,
    /// Ticks of the longest one, its submission summary, and the last one's end.
    longest: u64,
    longest_label: String,
    last: u64,
}

impl Sums {
    /// A submission that ran from `start` to `end` (card ticks), after those
    /// before it. The start's stamp may be of before the last one's end: the
    /// part before that end is the last one's.
    fn add(&mut self, start: u64, end: u64, label: &str) -> u64 {
        if start == 0 || end < start {
            return 0;
        }
        let clamped_start = start.max(self.last);
        let took = end.saturating_sub(clamped_start);
        if self.last != 0 && clamped_start > self.last {
            self.between += clamped_start - self.last;
        }
        self.count += 1;
        self.busy += took;
        if took > self.longest {
            self.longest = took;
            self.longest_label.clear();
            self.longest_label.push_str(label);
        }
        self.last = end.max(self.last);
        took
    }

    fn record(&mut self, kind: SpanKind, stamps: [u64; TIMESTAMPS_PER_SPAN], label: &str) {
        match kind {
            SpanKind::Empty => {}
            SpanKind::CardSync => {
                let took = self.add(stamps[0], stamps[1], label);
                self.card_sync += took;
            }
            SpanKind::Submission { has_steps, has_check } => {
                let [t0, t1, t2, t3] = stamps;
                let total = self.add(t0, t3, label);
                if total == 0 {
                    return;
                }
                let raw_span = t3.saturating_sub(t0);
                let steps = if has_steps && t1 >= t0 && t1 <= t3 { t1 - t0 } else { 0 };
                let check = if has_check && t2 >= t0 && t3 >= t2 && t2 >= t1.max(t0) {
                    t3 - t2
                } else {
                    0
                };
                // Scale phase components proportionally if overlapping previous submission clamped `t0`.
                let (steps, check) = if raw_span > total && raw_span > 0 {
                    (
                        ((u128::from(steps) * u128::from(total)) / u128::from(raw_span)) as u64,
                        ((u128::from(check) * u128::from(total)) / u128::from(raw_span)) as u64,
                    )
                } else {
                    (steps, check)
                };
                let steps = steps.min(total);
                let check = check.min(total - steps);
                let title = total - steps - check;
                self.card_steps += steps;
                self.card_check += check;
                self.title += title;
            }
        }
    }
}

fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| match std::env::var_os("VICTORY_CARD_SPANS") {
        Some(v) => v != "0",
        None => cfg!(feature = "diagnostics"),
    })
}

static SPANS_MADE: Mutex<Option<Option<Spans>>> = Mutex::new(None);

impl Spans {
    fn alloc_slot(&mut self, kind: SpanKind, label: String) -> usize {
        let i = self.next;
        self.next = (i + 1) % SPANS;
        let prev_kind = std::mem::replace(&mut self.kinds[i], kind);
        let prev_label = std::mem::replace(&mut self.labels[i], label);
        // The stamps of the submission that had these lists, that many
        // submissions ago: long done.
        // SAFETY: inside the mapping; written by the card at whole words.
        let stamps = unsafe {
            let at = (self.data as *mut u64).add(TIMESTAMPS_PER_SPAN * i);
            let mut read = [0u64; TIMESTAMPS_PER_SPAN];
            for (k, out) in read.iter_mut().enumerate() {
                *out = at.add(k).read_volatile();
                at.add(k).write_volatile(0);
            }
            read
        };
        self.sums.record(prev_kind, stamps, &prev_label);
        i
    }
}

impl Host {
    /// Two timestamp lists wrapping `card_before`'s CPU->GPU sync pass.
    pub(super) fn span_sync_lists(&self) -> Option<[ID3D12CommandList; 2]> {
        if !on() {
            return None;
        }
        let mut made = SPANS_MADE.lock().unwrap_or_else(|e| e.into_inner());
        let spans = made
            .get_or_insert_with(|| {
                self.make_spans()
                    .inspect_err(|e| tracing::warn!(error = %e, "no stamps around submissions"))
                    .ok()
            })
            .as_mut()?;
        let i = spans.alloc_slot(SpanKind::CardSync, "card_sync".to_owned());
        let base = TIMESTAMPS_PER_SPAN * i;
        Some([spans.lists[base].clone(), spans.lists[base + 1].clone()])
    }

    /// Timestamp lists around (and optionally between) `card_steps`, title work, and `card_check`.
    pub(super) fn span_submission(&self, has_steps: bool, has_check: bool, label: String) -> Option<SubSpans> {
        if !on() {
            return None;
        }
        let mut made = SPANS_MADE.lock().unwrap_or_else(|e| e.into_inner());
        let spans = made
            .get_or_insert_with(|| {
                self.make_spans()
                    .inspect_err(|e| tracing::warn!(error = %e, "no stamps around submissions"))
                    .ok()
            })
            .as_mut()?;
        let i = spans.alloc_slot(SpanKind::Submission { has_steps, has_check }, label);
        let base = TIMESTAMPS_PER_SPAN * i;
        Some(SubSpans {
            front: spans.lists[base].clone(),
            after_steps: has_steps.then(|| spans.lists[base + 1].clone()),
            before_check: has_check.then(|| spans.lists[base + 2].clone()),
            behind: spans.lists[base + 3].clone(),
        })
    }

    fn make_spans(&self) -> windows::core::Result<Spans> {
        let device = &self.device;
        // SAFETY: plain object creation; lists of one allocator recorded one
        // after the other.
        unsafe {
            let mut heap: Option<ID3D12QueryHeap> = None;
            device.CreateQueryHeap(
                &D3D12_QUERY_HEAP_DESC {
                    Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
                    Count: (TIMESTAMPS_PER_SPAN * SPANS) as u32,
                    NodeMask: 0,
                },
                &mut heap,
            )?;
            let heap = heap.ok_or_else(windows::core::Error::empty)?;
            let back = self
                .read_back_buffer((8 * TIMESTAMPS_PER_SPAN * SPANS) as u64)
                .ok_or_else(windows::core::Error::empty)?;
            let mut data = std::ptr::null_mut();
            back.Map(0, None, Some(&mut data))?;
            std::ptr::write_bytes(data.cast::<u8>(), 0, 8 * TIMESTAMPS_PER_SPAN * SPANS);
            let allocator: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
            let mut lists = Vec::with_capacity(TIMESTAMPS_PER_SPAN * SPANS);
            for i in 0..(TIMESTAMPS_PER_SPAN * SPANS) as u32 {
                let list: ID3D12GraphicsCommandList =
                    device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)?;
                list.EndQuery(&heap, D3D12_QUERY_TYPE_TIMESTAMP, i);
                list.ResolveQueryData(&heap, D3D12_QUERY_TYPE_TIMESTAMP, i, 1, &back, u64::from(i) * 8);
                list.Close()?;
                lists.push(list.cast()?);
            }
            Ok(Spans {
                _allocator: allocator,
                _heap: heap,
                _back: back,
                data: data as usize,
                lists,
                kinds: vec![SpanKind::Empty; SPANS],
                labels: vec![String::new(); SPANS],
                ticks_per_ms: self.queue.GetTimestampFrequency()? as f64 / 1000.0,
                next: 0,
                sums: Sums::default(),
            })
        }
    }
}

/// The GPU timestamps the title was given since the last frame-rate line:
/// how many, how often a later one was 1 ms or more after the one before
/// (written with another submission), and where the first few lie.
static STAMPED: Mutex<(u64, u64, u64, Vec<usize>)> = Mutex::new((0, 0, 0, Vec::new()));

/// The title's GPU timestamp at `at` was given the value `now` (ticks of 10
/// ns).
pub(crate) fn stamped(at: usize, now: u64) {
    if !on() {
        return;
    }
    let mut s = STAMPED.lock().unwrap_or_else(|e| e.into_inner());
    s.0 += 1;
    s.1 += u64::from(now.saturating_sub(s.2) > 100_000);
    s.2 = now;
    if s.3.len() < 64 && !s.3.contains(&at) {
        s.3.push(at);
        tracing::info!(address = format_args!("{at:#x}"), now, "a GPU timestamp of the title");
    }
}

/// The viewports' sizes seen (each said once): whether the title draws its
/// scene at sizes of its choice.
pub(crate) fn viewport(width: u32, height: u32) {
    if !on() || width < 1024 || height < 576 {
        return;
    }
    static SEEN: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.len() < 200 && !seen.contains(&(width, height)) {
        seen.push((width, height));
        tracing::info!(width, height, "a viewport of the title");
    }
}

/// For the frame-rate line: what the stamps read since the last one say, per
/// frame (they are of some [`SPANS`] submissions ago).
pub(super) fn line(frames: f64) {
    {
        let mut s = STAMPED.lock().unwrap_or_else(|e| e.into_inner());
        if s.0 > 0 {
            tracing::info!(
                "GPU timestamps the title asked for per frame: {:.1}, in {:.1} groups written at one time",
                s.0 as f64 / frames,
                s.1 as f64 / frames
            );
        }
        (s.0, s.1) = (0, 0);
    }
    let mut made = SPANS_MADE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(Some(spans)) = made.as_mut() else {
        return;
    };
    let last = spans.sums.last;
    let s = std::mem::replace(&mut spans.sums, Sums { last, ..Sums::default() });
    let ms = |ticks: u64| ticks as f64 / spans.ticks_per_ms / frames;
    if s.longest_label.is_empty() {
        tracing::info!(
            "the card's own stamps per frame: {:.1} submissions, working {:.1} ms (card_sync {:.1}, card_steps {:.1}, title {:.1}, card_check {:.1}), idle between them {:.1} ms, the longest {:.1} ms",
            s.count as f64 / frames,
            ms(s.busy),
            ms(s.card_sync),
            ms(s.card_steps),
            ms(s.title),
            ms(s.card_check),
            ms(s.between),
            s.longest as f64 / spans.ticks_per_ms
        );
    } else {
        tracing::info!(
            "the card's own stamps per frame: {:.1} submissions, working {:.1} ms (card_sync {:.1}, card_steps {:.1}, title {:.1}, card_check {:.1}), idle between them {:.1} ms, the longest {:.1} ms [{}]",
            s.count as f64 / frames,
            ms(s.busy),
            ms(s.card_sync),
            ms(s.card_steps),
            ms(s.title),
            ms(s.card_check),
            ms(s.between),
            s.longest as f64 / spans.ticks_per_ms,
            s.longest_label
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{SpanKind, Sums};

    #[test]
    fn submissions_add_up_without_counting_anything_twice() {
        let mut s = Sums::default();
        s.add(100, 150, "first");
        s.add(140, 200, "second"); // began before the last one ended
        s.add(260, 270, "third");
        assert_eq!((s.count, s.busy, s.between, s.longest, s.last), (3, 110, 60, 50, 270));
        assert_eq!(s.longest_label, "first");
        // Never written, or nonsense: nothing.
        s.add(0, 0, "bad");
        s.add(300, 290, "bad");
        assert_eq!(s.count, 3);
        // Whole inside the last one.
        s.add(265, 268, "inside");
        assert_eq!((s.count, s.busy, s.between), (4, 110, 60));
    }

    #[test]
    fn per_phase_stamps_account_for_card_sync_steps_title_and_check() {
        let mut s = Sums::default();
        s.record(SpanKind::CardSync, [100, 120, 0, 0], "card_sync");
        s.record(SpanKind::Submission { has_steps: true, has_check: true }, [130, 145, 210, 230], "sub-longest");
        s.record(SpanKind::Submission { has_steps: false, has_check: false }, [240, 0, 0, 290], "sub-mid");
        s.record(SpanKind::Submission { has_steps: true, has_check: false }, [300, 310, 0, 340], "sub-steps");
        s.record(SpanKind::Submission { has_steps: false, has_check: true }, [350, 0, 380, 395], "sub-check");
        assert_eq!(s.count, 5);
        assert_eq!(s.card_sync, 20);
        assert_eq!(s.card_steps, 15 + 10);
        assert_eq!(s.title, 65 + 50 + 30 + 30);
        assert_eq!(s.card_check, 20 + 15);
        assert_eq!(s.busy, s.card_sync + s.card_steps + s.title + s.card_check);
        assert_eq!(s.longest_label, "sub-longest");
    }
}
