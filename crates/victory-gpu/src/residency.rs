//! Host-memory admission for speculative texture work and reusable upload storage.
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;
static PRESSURED: AtomicBool = AtomicBool::new(false);
static SHARED_PRESSURED: AtomicBool = AtomicBool::new(false);
static SHARED_CRITICAL: AtomicBool = AtomicBool::new(false);
static HEADROOM: AtomicU64 = AtomicU64::new(u64::MAX);

fn memory_status() -> Option<MEMORYSTATUSEX> {
    // SAFETY: the API initializes the fields of a correctly sized structure.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status)
}

// The texture sweep may run for seconds inside a single submitted frame.
static NEXT_COMMIT_SAMPLE: AtomicU64 = AtomicU64::new(0);
static AVAILABLE_COMMIT: AtomicU64 = AtomicU64::new(u64::MAX);
static RETRY_UPLOAD_AT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn allocation_error(error: &windows::core::Error) {
    if error.code() == windows::Win32::Foundation::E_OUTOFMEMORY {
        RETRY_UPLOAD_AT.fetch_max(crate::host::coarse_ms().saturating_add(500), Ordering::Relaxed);
    }
}

thread_local! {
    static STAGING_DEFERRED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn note_staging_deferred() {
    STAGING_DEFERRED.set(true);
}

pub(crate) fn take_staging_deferred() -> bool {
    STAGING_DEFERRED.replace(false)
}

pub(crate) fn upload_refused() -> bool {
    crate::host::coarse_ms() < RETRY_UPLOAD_AT.load(Ordering::Relaxed)
}

fn sample_commit(now: u64) -> u64 {
    let next = NEXT_COMMIT_SAMPLE.load(Ordering::Relaxed);
    if now >= next
        && NEXT_COMMIT_SAMPLE
            .compare_exchange(next, now.saturating_add(32), Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        && let Some(status) = memory_status()
    {
        AVAILABLE_COMMIT.store(status.ullAvailPageFile, Ordering::Relaxed);
    }
    AVAILABLE_COMMIT.load(Ordering::Relaxed)
}

fn has_upload_reserve(commit: u64, bake: bool) -> bool {
    // Keep a protective floor against Rust/game OOM aborts without starving
    // bindless world textures or one-shot ped decoration bakes when steady-state
    // commit headroom sits in the 1-4 GiB range on 32 GiB hosts.
    commit >= if bake { 256 * MIB } else { 512 * MIB }
}

fn has_growth_reserve(commit: u64, bake: bool) -> bool {
    // Growing a fresh 64 MiB upload block for speculative bindless prefetch
    // requires more headroom than reusing slices already committed inside the
    // staging pool.
    commit >= if bake { 256 * MIB } else { 1024 * MIB }
}

/// Leave commit for game allocations and submissions already in flight while
/// still letting bindless textures and one-shot material bakes upload.
pub(crate) fn admit_texture_pass(bake: bool) -> bool {
    let now = crate::host::coarse_ms();
    if now < RETRY_UPLOAD_AT.load(Ordering::Relaxed) {
        return false;
    }
    if !bake && SHARED_CRITICAL.load(Ordering::Relaxed) {
        return false;
    }
    has_upload_reserve(sample_commit(now), bake)
}

/// Whether the staging allocator may commit a brand-new OS upload buffer
/// (reusing existing free ranges in already-committed pool blocks stays open
/// whenever `admit_texture_pass` is true).
pub(crate) fn allow_staging_growth(bake: bool) -> bool {
    let now = crate::host::coarse_ms();
    if now < RETRY_UPLOAD_AT.load(Ordering::Relaxed) {
        return false;
    }
    if !bake
        && (SHARED_CRITICAL.load(Ordering::Relaxed)
            || (compact_host_ram() && SHARED_PRESSURED.load(Ordering::Relaxed)))
    {
        return false;
    }
    has_growth_reserve(sample_commit(now), bake)
}

pub(crate) fn limited_host_ram() -> bool {
    static LIMITED: OnceLock<bool> = OnceLock::new();
    *LIMITED.get_or_init(|| memory_status().is_some_and(|s| s.ullTotalPhys <= 32 * GIB))
}

/// Hosts with <= 16 GiB physical RAM (whose WDDM Shared GPU Memory cap is
/// ~8 GiB regardless of pagefile size).
pub(crate) fn compact_host_ram() -> bool {
    static COMPACT: OnceLock<bool> = OnceLock::new();
    *COMPACT.get_or_init(|| memory_status().is_some_and(|s| s.ullTotalPhys <= 18 * GIB))
}

fn next_pressure(was: bool, physical: u64, commit: u64) -> bool {
    if was {
        physical < 5 * GIB || commit < 6 * GIB
    } else {
        physical <= 3 * GIB || commit <= 4 * GIB
    }
}

/// Evaluates WDDM Shared GPU Memory (`DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL`)
/// pressure and critical state with hysteresis.
fn next_shared_pressure(was: bool, budget: u64, used: u64) -> (bool, bool) {
    if budget == 0 {
        return (false, false);
    }
    let remaining = budget.saturating_sub(used);
    let pct_ge = |p: u128| u128::from(used) * 100 >= u128::from(budget) * p;
    let critical = pct_ge(85) || (budget >= 4 * GIB && remaining < GIB);
    let pressured = if critical {
        true
    } else if was {
        pct_ge(60) || (budget >= 4 * GIB && remaining < 2560 * MIB)
    } else {
        pct_ge(70) || (budget >= 4 * GIB && remaining < 2 * GIB)
    };
    (pressured, critical)
}

/// Called by the once-per-second submission sweep. A failed sample keeps the last good state.
pub(crate) fn refresh(video_memory: Option<[u64; 4]>) {
    let Some(status) = memory_status() else { return };
    let (physical, commit) = (status.ullAvailPhys, status.ullAvailPageFile);
    let shared_avail = video_memory.and_then(|[_, _, sb, su]| (sb > 0).then(|| sb.saturating_sub(su)));
    HEADROOM.store(physical.min(commit).min(shared_avail.unwrap_or(u64::MAX)), Ordering::Relaxed);

    let was_ram = PRESSURED.load(Ordering::Relaxed);
    let was_shared = SHARED_PRESSURED.load(Ordering::Relaxed);
    let was = was_ram || was_shared;

    let ram_pressure = next_pressure(was_ram, physical, commit);
    let (shared_pressure, shared_critical) = match video_memory {
        Some([_, _, sb, su]) => next_shared_pressure(was_shared, sb, su),
        None => (false, false),
    };
    PRESSURED.store(ram_pressure, Ordering::Relaxed);
    SHARED_PRESSURED.store(shared_pressure, Ordering::Relaxed);
    SHARED_CRITICAL.store(shared_critical, Ordering::Relaxed);

    let now_pressured = ram_pressure || shared_pressure;
    if now_pressured != was {
        let [vram_budget_mib, vram_used_mib, shared_gpu_budget_mib, shared_gpu_used_mib] =
            video_memory.unwrap_or_default().map(|b| b >> 20);
        if now_pressured {
            tracing::warn!(
                pid = std::process::id(),
                frame = crate::frame::current(),
                physical_free_mib = physical >> 20,
                available_commit_mib = commit >> 20,
                vram_used_mib,
                vram_budget_mib,
                shared_gpu_used_mib,
                shared_gpu_budget_mib,
                shared_critical,
                buffer_mirror_span_mib = crate::shadow::live_span_bytes() >> 20,
                texture_staging_mib = crate::texture::live_staging_bytes() >> 20,
                spare_scratch_mib = crate::host::ALIVE[4].load(Ordering::Relaxed) >> 20,
                game_filled_gpu_texture_mib = crate::resource::KINDS[1].load(Ordering::Relaxed) >> 20,
                "host memory pressure: limiting texture prefetch and idle upload storage"
            );
        } else {
            tracing::info!(
                physical_free_mib = physical >> 20,
                available_commit_mib = commit >> 20,
                vram_used_mib,
                vram_budget_mib,
                shared_gpu_used_mib,
                shared_gpu_budget_mib,
                "host memory pressure cleared"
            );
        }
    }
}

pub(crate) fn headroom() -> Option<u64> {
    let bytes = HEADROOM.load(Ordering::Relaxed);
    (bytes != u64::MAX).then_some(bytes)
}

pub(crate) fn pressured() -> bool {
    PRESSURED.load(Ordering::Relaxed) || SHARED_PRESSURED.load(Ordering::Relaxed)
}

/// True when OS commit headroom is critically low (< 1.5 GiB), WDDM Shared GPU
/// Memory is critical, or an allocation just failed with `E_OUTOFMEMORY`,
/// requiring emergency throttling.
pub(crate) fn critical_commit() -> bool {
    let now = crate::host::coarse_ms();
    now < RETRY_UPLOAD_AT.load(Ordering::Relaxed)
        || sample_commit(now) < 1536 * MIB
        || SHARED_CRITICAL.load(Ordering::Relaxed)
}

/// A soft per-frame admission limit. A single texture can cross it. The
/// queue retains unserved slots so they can be retried on later frames.
pub(crate) fn prefetch_limit(bytes: u64, pressure: bool, critical: bool, limited: bool, staging: u64) -> u64 {
    if critical {
        bytes.min(16 << 20)
    } else if pressure || (limited && staging >= GIB) {
        bytes.min(64 << 20)
    } else {
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_and_lookahead_leave_room_for_live_game_allocations() {
        for (commit, bake, lookahead, growth) in [
            (128 * MIB, false, false, false),
            (256 * MIB, true, false, false),
            (512 * MIB, true, true, false),
            (768 * MIB, true, true, false),
            (1024 * MIB, true, true, true),
        ] {
            assert_eq!(has_upload_reserve(commit, true), bake);
            assert_eq!(has_upload_reserve(commit, false), lookahead);
            assert_eq!(has_growth_reserve(commit, false), growth);
        }
    }

    #[test]
    fn either_constraint_enters_pressure_and_both_must_recover() {
        assert!(next_pressure(false, 12 * GIB, 4 * GIB));
        assert!(next_pressure(false, 3 * GIB, 16 * GIB));
        assert!(next_pressure(true, 4 * GIB, 7 * GIB));
        assert!(next_pressure(true, 6 * GIB, 5 * GIB));
        assert!(!next_pressure(true, 5 * GIB, 6 * GIB));
        assert!(!next_pressure(false, 4 * GIB, 5 * GIB));
    }

    #[test]
    fn shared_gpu_pressure_tracks_8_gib_budget_with_hysteresis() {
        let budget = 8 * GIB;
        assert_eq!(next_shared_pressure(false, 0, 100), (false, false));
        // Below 70% (< 5.6 GiB) and >= 2 GiB free: normal
        assert_eq!(next_shared_pressure(false, budget, 5 * GIB), (false, false));
        // 70% of 8 GiB = 5735 MiB: enters pressure, not critical
        assert_eq!(next_shared_pressure(false, budget, 5800 * MIB), (true, false));
        // Hysteresis keeps pressure at 62% / 3 GiB remaining
        assert_eq!(next_shared_pressure(true, budget, 5 * GIB), (true, false));
        // Recovers below 60% (< 4.8 GiB) and >= 2560 MiB remaining
        assert_eq!(next_shared_pressure(true, budget, 4500 * MIB), (false, false));
        // Critical at >= 85% (6964 MiB) or < 1 GiB remaining
        assert_eq!(next_shared_pressure(false, budget, 7 * GIB), (true, true));
        assert_eq!(next_shared_pressure(true, budget, 7000 * MIB), (true, true));
    }

    #[test]
    fn prefetch_respects_existing_budgets_and_resumes_after_recovery() {
        assert_eq!(prefetch_limit(512 << 20, false, false, true, GIB), 64 << 20);
        assert_eq!(prefetch_limit(512 << 20, false, false, true, GIB - 1), 512 << 20);
        assert_eq!(prefetch_limit(512 << 20, false, false, false, 2 * GIB), 512 << 20);
        assert_eq!(prefetch_limit(0, true, true, false, 0), 0);
        assert_eq!(prefetch_limit(8 << 20, true, true, false, 0), 8 << 20);
        assert_eq!(prefetch_limit(512 << 20, true, false, false, 0), 64 << 20);
        assert_eq!(prefetch_limit(512 << 20, true, true, false, 0), 16 << 20);
        assert_eq!(prefetch_limit(192 << 20, false, false, false, 0), 192 << 20);
    }
}
