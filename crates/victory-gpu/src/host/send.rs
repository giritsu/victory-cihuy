//! What goes up to copies in the card's memory goes up as it was when its
//! submission was prepared (`VICTORY_SEND_OWN`, on by default; `0`: as
//! before).
//!
//! The sends were copies out of the copies' CPU sides, which the card makes
//! when it comes to the submission. By then later submissions are prepared
//! (two frames of them with `VICTORY_RELAY`), and their refreshes have
//! written the same CPU sides: the earlier submission's shaders then read a
//! later one's bytes. For what the title's CPU writes that is as on the
//! console (it does not write what the card still reads). But the title's
//! own buffer copies are commands in its lists, and those are done by us
//! when a submission is prepared: "a value to a buffer, a draw that reads
//! it" in one submission and the same with another value in the next is
//! ordinary, and both draws read the second value.
//!
//! So each submission's bytes are taken out of the CPU sides into memory of
//! the submission's own when they are taken from the dirty lists, and the
//! card copies them from there. Runs of a few words, which a refresh that
//! sends only what changed makes many of, go as pairs of offset and value,
//! stored by one dispatch per copy; the others as a copy each.

use super::*;
use crate::shadow::Shadow;

pub(crate) fn own() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_SEND_OWN").is_none_or(|v| v != "0"))
}

/// Runs up to this many bytes go as pairs.
const SHORT: usize = 256;

/// Whether a run of `len` bytes at offset `at` of a copy goes as pairs.
fn short(at: usize, len: usize) -> bool {
    len <= SHORT && at % 4 == 0 && len % 4 == 0
}

/// Whether a run of `len` bytes at offset `at` of a copy in the card's memory
/// is no copy command of its own, but pairs of one dispatch.
pub(crate) fn goes_as_words(at: usize, len: usize) -> bool {
    own() && short(at, len)
}

/// Whether the title's bytes that go in under flights' writes
/// (`Shadow::merge_under_writes`) go in by whole words
/// (`VICTORY_UNDER_WORDS=0`: by the bytes that changed, as before).
///
/// By bytes nearly every run (a float's three low bytes) was a copy command
/// of its own, and a submission with more than 4,096 of them stood until the
/// flights had landed: 4 to 14 ms of a steady frame (session play-1125-0203:
/// "more changed runs than a submission sends up"), for some 20 KB. Whole
/// words are pairs of one dispatch, and are not counted.
///
/// A word is taken to be the title's or the shaders', not shared: where the
/// title changed a byte of a word of which a flight's shader writes another
/// byte, that byte is as it was before the flight. The landing rests on the
/// same ([`crate::shadow::exact_back`]).
pub(crate) fn words_under() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| own() && std::env::var_os("VICTORY_UNDER_WORDS").is_none_or(|v| v != "0"))
}

const SCATTER: &str = r#"
#define RS "RootConstants(num32BitConstants=2, b0), SRV(t0), UAV(u0)"
cbuffer Part : register(b0) { uint at; uint count; };
ByteAddressBuffer pairs : register(t0);
RWByteAddressBuffer card : register(u0);
[RootSignature(RS)]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    if (id.x >= count) return;
    uint2 p = pairs.Load2(at + id.x * 8u);
    card.Store(p.x, p.y);
}
"#;

struct Pass {
    root: ID3D12RootSignature,
    pso: ID3D12PipelineState,
}

fn pass(host: &Host) -> Option<&'static Pass> {
    static PASS: OnceLock<Option<Pass>> = OnceLock::new();
    PASS.get_or_init(|| {
        let code = compile(SCATTER, windows::core::s!("main"), windows::core::s!("cs_5_1")).ok()?;
        // SAFETY: the code lives through the calls; the root signature is
        // inside it.
        unsafe {
            let root: ID3D12RootSignature = host.device.CreateRootSignature(0, &code).ok()?;
            let pso: ID3D12PipelineState = host
                .device
                .CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                    pRootSignature: std::mem::transmute_copy(&root),
                    CS: D3D12_SHADER_BYTECODE {
                        pShaderBytecode: code.as_ptr().cast(),
                        BytecodeLength: code.len(),
                    },
                    ..Default::default()
                })
                .ok()?;
            name(&pso, || "victory: changed words sent to the card".into());
            Some(Pass { root, pso })
        }
    })
    .as_ref()
}

/// Memory a submission's sends come from, mapped.
pub(super) struct Memory {
    buffer: ID3D12Resource,
    data: usize,
    size: u64,
}

// SAFETY: a D3D12 object (free-threaded) and its mapping, with one owner.
unsafe impl Send for Memory {}

/// What submissions that have run gave back.
static SPARE: Mutex<Vec<Memory>> = Mutex::new(Vec::new());

/// At most this much is kept (of the card's memory, with [`on_card`]).
fn kept() -> u64 {
    if on_card() {
        256 << 20
    } else if crate::residency::compact_host_ram() {
        if crate::residency::critical_commit() {
            0
        } else if crate::residency::shared_pressured() {
            128 << 20
        } else {
            256 << 20
        }
    } else {
        768 << 20
    }
}

/// Whether the memory lies in the card's memory, written by the CPU through
/// the bus (`VICTORY_SEND_BAR=1`): the copies on the card's drawing queue, which
/// nothing overlaps, then cross no bus. In a captured frame they took 5.4 ms of
/// the card's 30 (342 copies), and take 0.4 this way; but filling it takes the
/// submitting thread 8-9 ms a frame instead of 3 (73-93 MB, run card-3 against
/// card-1), so it is a loss while the card waits for that thread: off by
/// default.
fn on_card() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_SEND_BAR").is_some_and(|v| v == "1"))
}

fn memory(host: &Host, size: u64) -> Option<Memory> {
    {
        let mut spare = SPARE.lock().unwrap_or_else(|e| e.into_inner());
        let fit = spare.iter().enumerate().filter(|(_, m)| m.size >= size).min_by_key(|(_, m)| m.size).map(|(i, _)| i);
        if let Some(i) = fit {
            return Some(spare.swap_remove(i));
        }
    }
    // By doubles, so that the next of about this size finds it: what
    // submissions send differs from frame to frame.
    let size = size.max(2 << 20).next_power_of_two();
    let mapped = if on_card() { host.mapped_card_buffer(size, false) } else { None };
    let (buffer, data) = match mapped {
        Some(made) => made,
        None => host.upload_buffer(size)?,
    };
    name(&buffer, || "bytes a submission sends up".into());
    Some(Memory {
        buffer,
        data: data as usize,
        size,
    })
}

fn trim_spare_vec<T>(spare: &mut Vec<T>, size_of: impl Fn(&T) -> u64, limit: u64) {
    const SPIKE_BYTES: u64 = 32 << 20;
    spare.sort_by_key(|m| std::cmp::Reverse(size_of(m)));
    let mut total: u64 = spare.iter().map(&size_of).sum();
    while total > limit && !spare.is_empty() {
        let removed = if size_of(&spare[0]) > SPIKE_BYTES {
            spare.remove(0)
        } else {
            spare.pop().expect("non-empty spare")
        };
        total -= size_of(&removed);
    }
}

/// The submission has run.
pub(super) fn give_back(memory: Memory) {
    let mut spare = SPARE.lock().unwrap_or_else(|e| e.into_inner());
    spare.push(memory);
    trim_spare_vec(&mut spare, |m| m.size, kept());
}

/// Drops spare submission-send upload buffers under critical memory pressure or OOM.
pub(super) fn trim_spare() {
    let freed = std::mem::take(&mut *SPARE.lock().unwrap_or_else(|e| e.into_inner()));
    drop(freed);
}

/// Since the last frame-rate line: bytes copied, runs copied, words stored,
/// microseconds of taking them out of the CPU sides.
pub(super) static STATS: [std::sync::atomic::AtomicU64; 4] = [const { std::sync::atomic::AtomicU64::new(0) }; 4];

/// One copy's sends: ranges copied (offset in the copy, in the memory,
/// bytes), and its pairs (where in the memory, how many).
struct Copy {
    runs: Vec<(usize, u64, usize)>,
    pairs: (u64, u32),
}

/// Where a submission's bytes are taken to.
pub(super) struct Own {
    memory: Memory,
    copies: Vec<Copy>,
}

/// Runs whose pairs are one piece of work.
const RUNS_AT_A_TIME: usize = 1024;

/// Splits large moves (`> chunk_bytes`) and packs small moves into `~chunk_bytes` groups.
fn build_copy_groups(
    moves: &[(usize, u64, usize)],
    chunk_bytes: usize,
) -> (Vec<(usize, u64, usize)>, Vec<std::ops::Range<usize>>) {
    let chunk_bytes = chunk_bytes.max(1);
    let mut slices = Vec::with_capacity(moves.len());
    let mut groups = Vec::new();
    let mut group_start = 0usize;
    let mut group_bytes = 0usize;
    for &(src, dst, len) in moves {
        if len == 0 {
            continue;
        }
        let mut off = 0usize;
        while off < len {
            let take = (len - off).min(chunk_bytes - group_bytes);
            slices.push((src + off, dst + off as u64, take));
            off += take;
            group_bytes += take;
            if group_bytes >= chunk_bytes {
                groups.push(group_start..slices.len());
                group_start = slices.len();
                group_bytes = 0;
            }
        }
    }
    if group_start < slices.len() {
        groups.push(group_start..slices.len());
    }
    (slices, groups)
}

/// Takes what `dirty` sends up out of the CPU sides.
pub(super) fn take(host: &Host, dirty: &[(std::sync::Arc<Shadow>, std::sync::Arc<Vec<(usize, usize)>>)]) -> Option<Own> {
    let began = std::time::Instant::now();
    pass(host)?;
    // Runs of whole words of the copy: only those are pairs. (A byte beside
    // them in a word may be the card's.)
    let is_short = |&(a, b): &(usize, usize)| short(a, b - a);
    let mut at = 0u64;
    let mut copies = Vec::with_capacity(dirty.len());
    let mut raw_moves: Vec<(usize, u64, usize)> = Vec::new();
    let mut total_move_bytes = 0usize;
    let mut total_words = 0usize;
    let mut chunk_words: Vec<(usize, &[(usize, usize)], usize)> = Vec::new();
    let mut scatter_work: Vec<(usize, &[(usize, usize)], u64)> = Vec::new();
    for (shadow, ranges) in dirty {
        let from = shadow.card_data() as usize;
        let mut runs = Vec::new();
        let chunk_begin = chunk_words.len();
        let mut copy_words = 0usize;
        for chunk in ranges.chunks(RUNS_AT_A_TIME) {
            let mut words = 0usize;
            for &(a, b) in chunk {
                let len = b - a;
                if short(a, len) {
                    words += len / 4;
                } else {
                    runs.push((a, at, len));
                    raw_moves.push((from + a, at, len));
                    total_move_bytes += len;
                    at += (len as u64).next_multiple_of(16);
                }
            }
            if words > 0 {
                chunk_words.push((from, chunk, words));
                copy_words += words;
            }
        }
        copies.push(Copy {
            runs,
            pairs: (at, copy_words as u32),
        });
        let mut pair_at = at;
        for &(c_from, chunk, words) in &chunk_words[chunk_begin..] {
            scatter_work.push((c_from, chunk, pair_at));
            pair_at += words as u64 * 8;
        }
        chunk_words.truncate(chunk_begin);
        total_words += copy_words;
        at += (copy_words as u64 * 8).next_multiple_of(16);
    }
    if at == 0 {
        return None;
    }
    let memory = memory(host, at)?;
    let mem_data = memory.data;
    let (copy_slices, copy_groups) = build_copy_groups(&raw_moves, 256 << 10);
    let copy_count = copy_groups.len();
    let total_jobs = copy_count + scatter_work.len();
    let run_job = |i: usize| {
        if i < copy_count {
            for &(src, rel_dst, len) in &copy_slices[copy_groups[i].clone()] {
                // SAFETY: non-overlapping slices inside source mapping and freshly allocated `memory`.
                unsafe { std::ptr::copy_nonoverlapping(src as *const u8, (mem_data + rel_dst as usize) as *mut u8, len) };
            }
            return;
        }
        let (from, ranges, pair_rel) = scatter_work[i - copy_count];
        let mut to = (mem_data + pair_rel as usize) as *mut [u32; 2];
        for &(a, b) in ranges.iter().filter(|r| is_short(r)) {
            for offset in (a..b).step_by(4) {
                // SAFETY: inside the copy's mapping (its CPU side, which
                // the thread that asked, and waits, alone writes in these
                // ranges) and inside the memory, which has room for every
                // pair, a piece of work's behind the last one's.
                unsafe {
                    let word = ((from + offset) as *const u32).read_unaligned();
                    to.write([offset as u32, word]);
                    to = to.add(1);
                }
            }
        }
    };
    let total_bytes = total_move_bytes + total_words * 8;
    if total_jobs >= 2 && (total_bytes >= (64 << 10) || total_words >= 4096) {
        crate::land::each_under(total_jobs, &run_job);
    } else {
        (0..total_jobs).for_each(run_job);
    }
    STATS[0].fetch_add(total_move_bytes as u64, Ordering::Relaxed);
    STATS[1].fetch_add(raw_moves.len() as u64, Ordering::Relaxed);
    STATS[2].fetch_add(total_words as u64, Ordering::Relaxed);
    STATS[3].fetch_add(began.elapsed().as_micros() as u64, Ordering::Relaxed);
    Some(Own { memory, copies })
}

impl Own {
    /// Records the sends into `list`; the copies that `looked` says are in
    /// `COPY_DEST`, the others in the common state. Those with a "before" are
    /// left readable by compute shaders.
    ///
    /// # Safety
    /// An open list of ours, whose compute state is set anew behind this.
    pub(super) unsafe fn record(
        &self,
        host: &Host,
        list: &ID3D12GraphicsCommandList,
        dirty: &[(std::sync::Arc<Shadow>, std::sync::Arc<Vec<(usize, usize)>>)],
        looked: impl Fn(&Shadow) -> bool,
    ) {
        let read = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
        let write = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
        let dest = D3D12_RESOURCE_STATE_COPY_DEST;
        let (mut first, mut last) = (Vec::new(), Vec::new());
        for ((shadow, _), copy) in dirty.iter().zip(&self.copies) {
            for &(a, from, n) in &copy.runs {
                // SAFETY: the caller's; live buffers, ranges inside them.
                unsafe { list.CopyBufferRegion(&shadow.buffer, a as u64, &self.memory.buffer, from, n as u64) };
            }
            let mut state = (looked(shadow) || !copy.runs.is_empty()).then_some(dest);
            if copy.pairs.1 > 0 {
                if let Some(was) = state {
                    first.push(transition(&shadow.buffer, was, write));
                }
                state = Some(write);
            }
            let hashed = shadow.card_sides().is_some_and(|(_, before, _)| before.is_some());
            if let Some(was) = state
                && hashed
                && (!copy.runs.is_empty() || copy.pairs.1 > 0)
            {
                last.push(transition(&shadow.buffer, was, read));
            }
        }
        // SAFETY: the caller's; live objects.
        unsafe {
            if !first.is_empty() {
                list.ResourceBarrier(&first);
            }
            if let Some(pass) = pass(host)
                && self.copies.iter().any(|c| c.pairs.1 > 0)
            {
                list.SetComputeRootSignature(&pass.root);
                list.SetPipelineState(&pass.pso);
                list.SetComputeRootShaderResourceView(1, self.memory.buffer.GetGPUVirtualAddress());
                for ((shadow, _), copy) in dirty.iter().zip(&self.copies).filter(|(_, c)| c.pairs.1 > 0) {
                    list.SetComputeRootUnorderedAccessView(2, shadow.buffer.GetGPUVirtualAddress());
                    const MOST: u32 = 64 * D3D12_CS_DISPATCH_MAX_THREAD_GROUPS_PER_DIMENSION;
                    let (at, words) = copy.pairs;
                    let mut done = 0;
                    while done < words {
                        let n = (words - done).min(MOST);
                        let constants = [at as u32 + done * 8, n];
                        list.SetComputeRoot32BitConstants(0, 2, constants.as_ptr().cast(), 0);
                        list.Dispatch(n.div_ceil(64), 1, 1);
                        done += n;
                    }
                }
            }
            if !last.is_empty() {
                list.ResourceBarrier(&last);
            }
        }
    }

    /// The memory, to be kept until the submission has run.
    pub(super) fn memory(self) -> Memory {
        self.memory
    }
}

#[cfg(test)]
mod tests {
    use super::{build_copy_groups, trim_spare_vec};

    #[test]
    fn copy_groups_split_large_and_pack_small_runs() {
        let moves = [
            (0x1000, 0, 100),
            (0x2000, 200, 200),
            (0x3000, 1000, 700),
            (0x4000, 2000, 0),
        ];
        let (slices, groups) = build_copy_groups(&moves, 256);
        assert!(groups.len() >= 4);
        let mut unpacked = Vec::new();
        for g in &groups {
            let group_bytes: usize = slices[g.clone()].iter().map(|s| s.2).sum();
            assert!(group_bytes <= 256);
            for &(src, dst, len) in &slices[g.clone()] {
                for off in 0..len {
                    unpacked.push((src + off, dst + off as u64));
                }
            }
        }
        let mut expected = Vec::new();
        for &(src, dst, len) in &moves {
            for off in 0..len {
                expected.push((src + off, dst + off as u64));
            }
        }
        assert_eq!(unpacked, expected);
    }

    #[test]
    fn trim_spare_evicts_oversized_spikes_first_then_smallest() {
        let mut items = vec![4u64 << 20, 16 << 20, 8 << 20, 64 << 20, 16 << 20];
        // Total = 108 MiB, limit = 40 MiB:
        // First evicts 64 MiB spike (>32 MiB) -> remaining 44 MiB (16, 16, 8, 4),
        // then evicts smallest (4 MiB) -> remaining 40 MiB (16, 16, 8).
        trim_spare_vec(&mut items, |&s| s, 40 << 20);
        assert_eq!(items, vec![16 << 20, 16 << 20, 8 << 20]);

        trim_spare_vec(&mut items, |&s| s, 0);
        assert!(items.is_empty());
    }
}
