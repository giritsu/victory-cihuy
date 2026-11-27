# VIctory on an RX 7900 XTX

## Run the codec_d Windows build

Download and extract the Windows x64 ZIP from the [private releases](https://github.com/codec-d/cool_fox/releases/latest). Open PowerShell in the extracted folder and point the launcher at your own complete Xbox dump.

```powershell
$env:VICTORY_GAME_DIR = 'D:\Games\MyDump'
.\Play-VIctory.cmd pareto-residency
```

The launcher selects Series S and the RTX-tested residency settings. It builds its shader cache locally under `%LOCALAPPDATA%\VIctory\cache-pareto`. Set `VICTORY_CACHE_DIR` to reuse an existing cache outside the checkout. Windows x64, an AVX2 CPU, and a Direct3D 12 GPU with shader model 6.6 and mesh shader support are required.

To build from this checkout, install the pinned Rust toolchain and the MSVC C++ build tools with a Windows SDK, then run `cargo build --release --locked -p victory`. The same launcher finds `target\release\victory.exe`. The sections below describe the upstream AMD fork and its separate launcher.

A fork of [VIctory](https://github.com/Marvisak/VIctory), a compatibility layer
that runs an Xbox Series title natively on Windows from files the user supplies.
This one is about **speed on AMD RDNA 3**: the world at 30 frames a second on
one PC, without giving up the picture. It is not there yet.

**This repository contains no game files, code or assets.** You need your own
copy of the game. It is not affiliated with or endorsed by the game's developer
or publisher.

## Where it stands (2026-11-25)

Tested on one machine only: Windows 11, Intel i9-12900K, 64 GB RAM, Radeon
RX 7900 XTX, 1440p, the Series X profile.

| | |
|---|---|
| Menus, videos, loading screens | 30 fps |
| Prologue, indoors | 50-75 ms a frame (13-20 fps) |
| First mission with the boat | 46-53 ms (19-22 fps) |
| Street chase with many cars and people | 100-165 ms (6-10 fps), single frames far longer |
| Outdoors on foot, the camp | 60-90 ms |
| Flying | 42-57 ms |
| Longest session so far | 41 minutes, no crash |
| Memory | 40-46 GB of private memory: **64 GB of RAM recommended** |

It started at 280 ms a frame in the same scenes two days earlier.

What is known to be wrong:

- Trees, plants and far buildings flicker outdoors in some places.
- Rooms with little direct light are too dark without `-RayTracing`.
- Textures of what just came into view appear late while the title streams.
- The first run with a new driver or a new build stutters while the driver makes
  pipelines (`victory warm` makes them ahead).
- Recording a frame in the world makes the title's own watchdog end it.

## Local PC Profile & Progress (2026-11-26)

Target workstation profile added in this checkout:

- **OS & Display:** Windows 10 Pro (22H2, Build 19045), 1920x1080, Xbox Series S console profile (`--console series-s`).
- **CPU:** AMD Ryzen 5 5600 (6 Cores / 12 Threads, Zen 3 with AVX2).
- **RAM / Pagefile:** 16 GB physical RAM + 32 GB system pagefile (**note:** Windows WDDM hard-caps Shared GPU Memory `NON_LOCAL` at 50% of physical RAM, ~7,490 MiB).
- **GPU:** AMD Radeon RX 7600 XT 16 GB VRAM (RDNA 3; native Wave32/Wave64, `views=2097152`, `raw_twins=true`, `mapped_card_memory=false` when BIOS ReBAR/SAM is disabled).
- **Windows 10 DirectX 12 Agility SDK:** `apps/victory` exports `D3D12SDKVersion=614` and `D3D12SDKPath=".\\D3D12\\"`; copy `D3D12Core.dll` and `d3d12SDKLayers.dll` (`1.614.x`) into `target\diag\D3D12\` and `target\release\D3D12\` on Windows 10 hosts so SM 6.6 `0xC00` root signatures initialize cleanly.

| Milestone | Status | Measured Result / Notes |
|---|---|---|
| Win10 Agility SDK 1.614.1 (`apps/victory`) | Done | Fixed Win10 `0x80070057` (`c00/c01`) and startup `0xc0000005`; 29/29 upscaler compute pipelines created. |
| Shader cache (`cache-pareto`) | Ready | 25,782 shaders pre-compiled (`victory shaders`) into `%LOCALAPPDATA%\VIctory\cache-pareto`. |
| Pass 1: 16 GB RAM & 6C/12T scaling | Done | Added WDDM `NON_LOCAL` pressure tracking, compact `STAGING_SPARES`, `bury_lag=6`, 32 MiB spare scratch cap, and 6C/12T worker counts (`thin=4`, `under=4`, `pipeline=4`, `dstorage=6`). |
| Pass 2: Staging bloat & 16 MiB staging pool | Done | Disabled `VICTORY_LEVELS_ON_DEMAND` default (`held_only()` active), added 16 MiB staging blocks, 100 ms `NON_LOCAL` sampling, and fast idle eviction. Runtime rose from ~1 min to **~7 min**; live texture staging dropped **2,104 MB -> 54 MB**; spurious texture re-uploads dropped **4,001+ -> 142**. |
| Pass 3: `thin::BACKS` cap & 4 MiB loose windows | Done | Bounded `thin::BACKS` (`64/32/16`) and restored `4 MiB` loose windows. Live `diag` test reached **8m22s (frame 4,757)** at **8.5–10.7 FPS**, holding Shared GPU steady around **6.1–6.4 GB** for 6 minutes with 0 `no upload buffer` stalls until a late area-load spike. |
| Pass 4: `detach_take` + `CARD_BUFFERS_MIN_KB=256` | Diagnosed | Live `diag` test ran **7m41s (frame 4,662)** before `0x8007000E` (`shared_gpu = 7,244 / 7,490 MB`, **4.85 GB VRAM free**). Telemetry revealed why `HostList` + card-buffer counts stayed high: `640 host lists alive` at second 6 came from empty startup `[RESET, CLOSE]` recordings (`3,057 MB` private bytes), while `153 host lists made/frame` (`339,267` total), `243,192` card buffers, and `206.6 s` of `MakeResident`/`mapped()` stalls came from `list::boxed` bypassing `list::IDLE` whenever `BACKS` missed. |
| Pass 5: Lazy `Front::recording`, `boxed -> fresh_list`, staging & OOM retry | Verified live | Ran **8m01s / frame 6,584** (**+41% more frames**, **10.3–18.0 FPS** gameplay in `diag`). Startup `HostList` dropped **640 → 23**, total created **339k → 27.5k (-92%)**, `card buffers` **243k → 54.5k (-78%)**, `MakeResident`+`mapped()` **206.6 s → 89.6 s (-57%)**, and staging pool blocks **352 → 56 MB (-84%)**. Failed at `shared_gpu = 7,745 / 7,490 MB` (while `5.01 GB` VRAM stayed free) due to `4,532 MB` loose windows + card-buffer `shared_buffer` mirrors in `NON_LOCAL`. |
| Pass 6: `CardStaging::Ram` + 2.5 GiB VRAM loose-window offload + `320` list pool | Verified (46 min) | Ran **46 minutes full main mission with 0 crashes** (`0` OOMs); WDDM Shared GPU held flat at **2,771–3,850 / 7,490 MB**, VRAM **12.3–12.6 / 14.95 GB**, `diag` **10.1–16.3 FPS** (min 4–31). Exposed `1.95M` upload-buffer / `858.7 s` `MakeResident` churn (`32 MiB` scratch cap), false `PRESSURED` lock at s42, `4.19M` placed-view `Shape` overflow at min 31, and `574k` texture evictions at 84% VRAM. |
| Pass 7: Compact streaming unlock, `192/256 MiB` pools, `Shape` filler strip, `1,792 MiB` VRAM balance | Verified (Series X) | Live 7-min Series X `diag` run (**0 crashes**): `3,327` pre-warmed pipelines loaded (`made=1`), `0/frame` non-RT pipeline drops, `0.00` empty indirects, `UPLOAD_AHEAD` active (`140,000` textures / `3,790 MB`, `1.1 ms` avg), `SHOW_AFTER` delay cut (`1.24–1.33 -> 0.60–0.69` frames), `distinct views kept=8,506`, Shared GPU `4,157–5,166 / 7,490 MB`, VRAM `13,018–13,191 / 14,859 MB`, `MakeResident` `40.9 s` (`~5.8 s/min`), median `8.3 FPS` (`diag`). Isolated remaining `629–855/frame` null raw twins in `ListViews` (`make_listed`), 14 false pressure toggles (`435–510 MiB` physical free), and `224 MB` scratch peak. |
| Pass 8: `ListViews` unaligned raw twins fix + `256 MiB` compact pressure/scratch tuning | Verified (Series X) | Live Series X `diag` run (**0 crashes**): **`0.0 null`** raw buffer views (down from `629–855/frame`), `0` texture evictions (`0 let go 0 MB`), `544` upload buffers (`113,083` spare reuses), `0` gameplay pressure toggles (1 brief `6.7 s` load blip), Shared GPU **`3,983 / 7,490 MB`**, VRAM **`11,512 / 14,781 MB`**, `diag` median **`10.5 FPS`** (`8.2–12.3`) with GPU only **`15.8 ms`**. |
| Pass 9: CPU hot-path optimization (`card_steps`, lock-free `watch`/`pending`, `wide_each_chunked`, `play-dev.ps1` tuning) | Verified (Series X) | Live Series X `diag` (`4m50s`, **0 crashes**, Shared GPU **`3,444 / 7,490 MB`**): cut `BEHIND_PLACED` settles **`15.8 -> 0.0 ms`** (`0.00` refused), `card_steps` **`9.8 -> 5.6–6.9 ms`**, and guard waits **`-70%`** (`104.6 -> 31.3 ms`). Isolated two regressions: `MIN_KB=64` evicted `144.7 MB/frame` back to CPU snapshots (`7.5 FPS` median `diag`), and `STORE_BESIDE` + 6-thread contention doubled kernel CPU (`51 -> 109 ms`). |
| Pass 10: Fix `MIN_KB=64` spill (`1,536 MiB` cap), balance 6C/12T pools, & wire `VICTORY_WRITE_SITES` | Verified (`Series X`) | Eliminated Pass 9's `144.7 MB/frame` snapshot spill & `109 ms` kernel contention (`snapshots 0.9–1.0 ms`, `write-sites.txt` `places=24`, `0.0 null` raw twins, `0` evictions, `refreshes 14.4–16.2 ms`, steady ~`10 FPS`). |
| Pass 11: Static `NullBank`, `copied_before` filter, early `sync_heaps`, async `land()` offload, `release` 1s `victory_gpu::fps` log | Verified (`Series S` & `X`) | `Series S release`: median **`11.5 FPS`** (steady `12.4–13.2`, max `14.1`, `0` error). `Series X diag`: `8.8 FPS` median / `13.8 max` (`0` error), isolating `card_steps::MOST`, `same-list BEHIND_PLACED`, `100–135 ms` texture passes, and `note_buffer` lock contention. |
| Pass 12: Raise `card_steps::MOST`/`BETWEEN`, inline placed CBV copies, `TEXTURE_STAGE_MS=12`, lock-free `Slots` (`in_textures`/`in_placed`) | Verified (`Series X`) | `Series X diag` median **`8.8 -> 12.6 FPS`** (+43%, max **`17.0 FPS`**), `release` peak **`16.8 FPS`** (`p10 8.6`), `too many steps=0`, `same-list BEHIND_PLACED=0.00`, `waiting for flights 0.0 ms`, `sync_heaps wait 6.2 ms`, `heap_views textures 14.3 ms`, RX 7600 XT **`82–95%` busy**, `0` errors. |
| Pass 13 / 13b: View write-watch skip, `Blocks` bit-ops, `Journal::ahead` binary search, `CopiedState`, `FrontSpans` + chunk-8, `O(W log W)` `WriterIndex`, `keep_ref`, cumulative `TEXTURE_STAGE_MS` | Verified (`Series X`) | `Series X release` hit **new record median `11.8 FPS`** (avg **`12.4`**, `p10 8.8`, `p90 17.0`, peak **`18.3 FPS`**, CPU med `49.5 ms`, `0` errors); `diag` cut refused write-watch **`324–355 -> 4.0`**, `card_steps plan` max **`242.3 -> 16.1 ms`**, `textures` max **`157.4 -> 40.7 ms`**, worst frame **`1,515 -> 281 ms`**, RX 7600 XT up to **`97%` busy**. |
| Pass 14: Consolidate `GUARDS` locks & piece-scans + batch `placeholder_end`, `send::take` eviction/parallel dispatch, cache `placed_bytes_from`, pace `copied_before`, smooth `heap_buffer_pass` | Verified (`Series X`) | **0 error (`9k+` heavy open-world):** `send::take` **`6.6 -> 5.3 ms`** med (`p90 9.5 ms`), `card_before` **`10.2 -> 9.1 ms`**, `release 7k-9k` CPU **`61.5 -> 54.8 ms`**, worst frame **`583.9 -> 330.3 ms`**, Shared GPU **`~650 MiB` leaner**, `0.0 null` raw twins; isolated **`VICTORY_THIN` (`373–466 ms` CPU)** as #1 bottleneck. |
| Pass 15: Sharded `RETIRED_SHARDS` + batch retire, `CopyDescriptors` batching, `(frame, epoch)` `ListViews` w/ inline CBV bypass, cached COM metadata (`Shadow`/`HostTexture`/`Chunk`/`HostHeap`), precomputed `HostPipeline` flags, zero-alloc `set_targets`/`prepare_for`, `title_fills` filter & `mirror_sync` | Verified (`Series X`) | **0 error (`130s` `release` / `88s` `diag`, `avg 9,278`–`10,030` draws):** `VICTORY_THIN` **`338.2 ms` med** (`-15–25%`), `sync_heaps` **`38.0 ms`** (`making 14.7 ms`), `heap_views textures` **`24.2 ms`**, `release 9k+` **`9.4 -> 10.2 FPS` med** (`p90 11.2`), Shared GPU **`3,920–3,936 MiB`**, `0.0 null` raw twins, `0` evictions. Isolated `GUARDS` `95.2 ms`, `ListViews` over-invalidation (`31.6k` raw twins), `lists reset` `18.7 ms`, & `flush_all` `26.3 ms`. |
| Pass 16: `GUARDS` `RwLock` + read-only `guard()` fast-path + region-batched `protect_here`/coalesced `unguard`; split `BUFFER_VIEWS_EPOCH`/`TEXTURE_VIEWS_EPOCH` `ListViews` + drop false `buffers()` invalidation; batched `release_dynamic_batch`; `Deferred::clear` + parallel `recycle_many`; `AHEAD_ACTIVE` + `JournalRead::ranged_nothing_new` `CHUNK=16` in `flush_all` | Verified (`Series X`) | **0 error (`155s` `release` / `146s` `diag`, up to `13.5k` draws):** `1k–4k` **`23.3–24.6 FPS`**, `4k–7k` `release` **`9.9 -> 11.2 FPS` (+13%)**, `7k–9k` `diag` **`8.5 -> 10.0 FPS` (+17.6%)**, `maxFrame` **`3,349 -> 514 ms`**, Shared GPU **`3,660 MiB`**, `0.0 null` raw twins; `guards` `-27%/call`, `flush_all` `-14.4%`, `sync_heaps making` `-17%`. Isolated `let_go_idle` **`607.2 ms`**, `30.2 s` residency stalls, and `GUARDS` write-lock holds. |
| Pass 17: Budgeted `let_go_idle` (`512/3ms`, urgent `2048/8ms`) + `select_nth_unstable` + `host_written_snapshot` + single batch `retire`; private non-descriptor-heap buffers, `SPARE_SHARED`/`upload_cpu` + reusable `place_between` + `idle_most`/`idle_trim_keep`; chunked `PROTECT_CHUNK=64` `guard()` + batched `cover_run` | Verified (`Series X`) | **0 error (`83s` `release` & `diag`):** Eliminated `607.2 ms` sweep freeze (`0.28 ms`), cut `mapped()` waits `-83.1%`, `MakeResident` `-62.4%`, `host lists made` `-81.2%`, and `GUARDS` wait `-21.8%`; pushed `release ALL` `9.0 -> 9.8 FPS` (`p10 +25%`, `maxFrame 514 -> 236.9 ms`) and `9k+ diag` **`8.0 -> 9.4 FPS`** (`+17.5%`, `maxFrame 446.5 -> 152.8 ms`). |
| Pass 18: 64-way `GUARDS` sharding (`4 MiB` stripes, sorted `ShardLocks`), lock-free `Pool::each`, 128B AVX2 fast-skip in `victory_safe_merge` & `changed_records`, single-pass unguarded `land::piece`, single `Notes::flush()` + `Shadow::landed_many` + `remapped_batch`, and `sync_heaps` scratch/`quick_matches` | Verified (`Series X`) | **0 error / 0 crash:** `release` (`avg 9,147` draws) `7k–9k` **`11.4 FPS` (+17.5%)**, `1k–4k` `15.9` (`max 17.0`), `4k–7k` `11.9`, `9k+` `9.0` (`cpu 59.8 ms`), `ALL` med `9.9` (`p10 7.4`, `p90 12.4`); `diag` (`129.9s`) `GUARDS` lock wait **`-71.5%`** (`61.5 -> 17.50 ms`), `JOURNAL` note wait `-68.7%`, `land` notes `-78.6%`, `sync_heaps` read/compare `-58%` to `-71%` (`4.28 ms`), user CPU/frame `-20.6%` (`768 -> 610 ms`). |
| Pass 19: HLE `SetThreadGroupAffinity`/`SetThreadAffinityMask` unpinning, 64-way `64 KiB` `Shadow::merging`, `land::JOB=128 KiB` + `victory-reset` + LPT `relay_card_back`, in-place dynamic CBV/raw twin + batched retire, `ranged_check` + `REFRESH_BATCH_CHUNK=32`, and parallel disjoint `texback::land` | Verified (`Series X`) | **0 error / 0 crash / 0.00 null / 0 evictions:** `Shadow::merging` wait **`24.82 -> 2.66 ms` med** (`0.04–0.74 ms` steady), `read-backs` **`9.87–10.04 -> 6.79 ms`**, `card_steps` **`13.3–17.6 -> 9.85 ms`**; exposed CPU oversubscription (`user 610 -> 860 ms`, `kernel -> 109.5 ms`), `refreshes` (`29.6 ms`), & sync `bury(last)` (`8.1–41.8 ms`) in `victory-relay`. |
| Pass 20: Pass-through default affinity (`VICTORY_TITLE_AFFINITY=1\|unpin` opt-in), zero-double-lock `RangedCheck` (`MustUpdate`/`Exact`) + `REFRESH_BATCH_CHUNK=16`, async `bury()` offload to `victory-late-wait`, and trimmed `victory-reset` (`n/6` clamped `2..3`) | Verified (`Series X` & `Series S`) | **0 error / 0 crash / 0 pressure / 0.00 null / 0 evictions:** Series X `release` `ALL` **`10.1` med** (`p10 7.6`, `p90 13.3`, `max 18.4 FPS`), `7k–9k` `10.3`, `9k+` `8.9` (`maxMs 293.6 ms`); Series S `release` `ALL` **`11.4`**, `9k+` **`10.9 FPS`** (`maxMs 113.9 ms`); `diag` `bury` **`8.09 -> 0.04 ms`**, GPU busy **`70% -> 83%`**, `refreshes` **`29.6 -> 24.0 ms`**. |
| Pass 21: 256-way per-stripe `Shadow::merging` locking inside `merge_under_writes` + 6 `diagnostics` telemetry breakdowns (`under_writes`, `VICTORY_THIN` ops, `draw::tables`, `refreshes`/`card_before`/`card_steps`, `GUARDS` op waits, & `worker CPU` via `GetThreadTimes`) | Verified (`Series X diag`) | **0 error / 0 crash / 0 pressure / 0.00 null (`n=62`, avg `8,073` draws):** `ALL` **`8.9 -> 9.9 FPS` med** (`p10 8.0`, `p90 10.8`), `4k–7k` `10.8`, `7k–9k` `9.9` (`cpu 66.0 ms`), `9k+` `8.9`, `Shadow::merging` wait **`15.23 -> 0.58 ms` med** (`1.01 shards/lock`), GPU busy **`83% -> 86%`**. Exposed `VICTORY_THIN` `indirect` (`185.1 ms`), `draw::tables` (`146.0 ms`), `dirty_runs` (`15.9 ms`), & `GUARDS` (`27.2 ms`). |
| Pass 22: 256-way sharded `Card::dirty` + `RwLock<SentRuns>`, `HostList::refresh_epoch` + same-4KiB `Step::Refresh` coalescing, `shadow::find_any_bounds` MRU cache, lock-free `&'static` `host_signature` TLS + `last_indirect_args`, single-lookup `HostHeap::apply_listed_batched` + `CachedReadable`, & `watch` `fault` vs `land` guard wait split | Verified (`Series X diag`) | **0 error / 0 crash / 0.00 null (`n=184`, avg `8,637` draws, max `14.6k`):** `dirty_runs` **`15.90 -> 2.23 ms`** (-86.0%), `each_under` **`9.17 -> 5.75 ms`**, `under flights` **`13.45 -> 10.00 ms`**, `land merging` **`22.64 -> 18.03 ms`** (`all-thr 54.98 -> 42.56 ms`), `9k+ p10` **`4.9 -> 7.3 FPS` (+49%)**, `kernel` **`81.0 ms`**, `thin` **`217.0 ms`**. Isolated **`guards fault = 18.29 ms`** (`land = 0.00 ms`). |
| Pass 23: `GUARDS` `256` shard × `512 KiB` stripe, `FreshTable` (`has_texture` + `refresh_epoch` + `with_cached_readable` & initial seeding), & `draw::tables` / `indirect` micro-breakdowns | Verified (`Series X diag`) | **0 error / 0 crash / 0.00 null (`n=153`, avg `9,051` draws):** `ALL` **`9.5 -> 10.1 FPS` med** (`p10 8.5`, `cpu 65.8 -> 53.2 ms` `-12.6 ms`), `4k–7k` `11.8`, `7k–9k` `10.5`, `9k+` `9.3` (`p10 8.2`), `readable` `-44.2%` (`11.35 ms`), `guards fault` `18.29 -> 14.35 ms`, `kernel` `68.0 ms`. Pinpointed **`listed_slot` (`84.2 ms`: `22.3 ms` `keep_ref` + `25–28k` `ListViews` misses/frame)** as #1 bottleneck. |
| Pass 24: Cross-frame TTL `ListViews` (`BUF_SLOTS=16384`, `buf_made`-first, combined `make_twinned_with_readable`, idle-safe invalidation), `DynBlock=512` (`NullBlock` + `CopyBatch::kept_seen`), per-list `alloc_dynamic` slab `512`, `slot_tabled_range`, & `list_views` stats | Verified (`Series X diag`) | **0 error / 0.00 null (`n=100`, avg `10,667` draws):** `dynamic slots asked` **`-98.5%` (`326`, `0.40 ms`)**, `tables: dynamic` **`15.35 -> 11.80 ms`**, `buf_clr_epoch = 0.00`. Exposed `buf_clr_cap = 3.40/frame` full wipes (`listed_slot 101 ms`, `35.5k` raw twins), duplicate `CreateSRV`/`UAV` on already-raw buffers, `diag` atomic false-sharing, & `512`-slot block wipe cost. |
| Pass 25: Second-chance CLOCK eviction in `ListViews` (`BUF_CAP=8192`), primary-raw descriptor reuse (`twin_same`), batch-local `CopyBatch` diag + `u64` `last_kept_key`, & `DYN_BLOCK=256` (`used_hi`/`had_twin` + `write_bytes` + slab `256`) | Verified (`Series X diag`) | **0 error / 0.00 null (`n=94`, avg `10,526` draws):** **`keep_ref` `22.30 -> 2.10 ms` (-90.6%)**, `tables: dynamic` **`15.35 -> 9.60 ms`**, `inline_cbv` **`15.75 -> 12.90 ms`**, `listed_slot` **`101.0 -> 92.7 ms`**, `thin` **`401.3 -> 379.2 ms`**, `9k+` `8.3 -> 8.7 FPS` (`cpu 65.9 ms`), `ALL` `8.3 -> 8.6 FPS`, GPU busy `73% -> 80%`, `buf_clr_epoch 0.00`. Isolated `25.6k buf_miss` / `22.9k` CLOCK evictions + `VIEWS.read()` contention + per-slot overhead. |
| Pass 26: Lock-free `SHAPES_TABLE` slab + `SHAPE_INDEX` HashMap, per-table `TableViews`/`ReadableViews` scope, zero-hash `BufClockSlot` (`BUF_CAP=16384`), contiguous `write_strides_slice` + `note_dyn_range`, & low-QPC `listed_slot (miss / keep_ref)` diag | Verified (`Series X diag`) | **0 error / 0.00 null (`n=104`):** `draw::tables` **`170.6 -> 150.0 ms`**, `listed_slot` **`92.7 -> 84.8 ms`** (`miss 32.3 ms`, `keep_ref 2.1 ms`, HIT `50.4 ms`), `buf_evict -39.5%`, `dynamic 9.6 -> 7.9 ms`, `lists reset 20.3 -> 16.2 ms`; `8.60 FPS` clean (`12.8k` draws), `6.75 FPS` paged (`28.8 GB` private); pinpointed `submit 103.7 ms` wall & `title+other 373–632 ms`. |
| Pass 27: Deep critical-path telemetry (`#[cfg(feature = "diagnostics")]`): default `VICTORY_CARD_SPANS` ON in `diag`, `title hooks` (`descriptor`/`thin_front`/`veh_fault`), `sweep`/`make_waiting`, `bind/prepare`, & `listed_slot` `mirror`/`stride_dyn` | Verified (`Series S` & `X` `diag`) | **0 error / 0 crash / 0.00 null:** GPU `spans` **`77.4–81.7 ms`** Series X (cap ~12.9 FPS) vs **`44.3 ms` med (`p10 35.2 ms`)** Series S (`60%` GPU busy, **48.5 ms idle menunggu CPU**); membongkar `ListViews` pressure-TTL thrashing, `descriptor::mirror` / `copy_simple`, double `heap_textures` budget, & `on_write` VEH loop. |
| Pass 28: CPU Big-Hitters (`LIST_VIEWS_TTL_PRESSURED_FRAMES=8`, `mirror` lock-free + batched `mirror_range`, `HostHeap::null_state` + fast `mirror_after`, `early_textures`/`heap_textures` budget dedup + `bake_targets` cache, `on_write` VEH fast-exit, `-BarrierTest` switch) | Verified (`Series S diag`) | **0 error / 0.00 null (`n=46`, avg `8,778` draws):** `ALL` **`11.00 FPS` med** (`avg 11.46`, `max 25.4`), `submit` `78.3 ms`, `present wait` **`-49%`**, `list_views` miss **`-38%`**, `copy_desc`/`mirror` **`-63%`/`-56%`**, `sync_heaps making` **`-39%`**, `GUARDS` **`-49%`**; `-BarrierTest both` dikonfirmasi merusak gambar (*white screen*) tanpa hemat GPU berarti. |
| Pass 29: Sound global UAV barrier elision (`stores_any_uav` + `State::uav_clean`), per-phase GPU `spans` (`card_sync`/`card_steps`/`title`/`card_check`), skip redundant in-world `early_textures` + pre-loop budget, adaptive hot-block guard rest & complete `VEH_DIAG` | Verified (`Series X diag`) | **0 error / 0.00 null (`n=309`, avg `9,694` draws):** `early_tex` **`-98%`** (`0.50 ms`), `VirtualProtect` **`-69%`** (`12.3 ms`), `GUARDS` **`-72%`** (`2.6 ms`), `title+other` kernel **`-82%`** (`20 ms`, `-301 ms` total), `copy_desc`/`mirror` **`-80%`**, UAV elided `129.5`/frame, GPU `spans` `81.1 ms` (`title 67.25`, `card_sync 7.3`, `card_check 5.4`); isolated `SPARE` movie-chunk poisoning (`inline_cbv 77.8–95 ms`, `177k` alloc) & `75 ms` rest `refresh MB` inflation. |
| Pass 30: Isolate `SPARE_PLANES` + LRU `Spare::give` & `ScratchSpare` (`256 KiB` vs `4 MiB`) buckets, recalibrate adaptive guard rest (`3 -> 6 -> 12 -> 18 ms`, cap `24 ms`, window `150 ms`), & fix `LOGGED_PRESSURED` logging | Verified (`Series X`) | **0 error / 0 crash / 0.00 null (`release` `n=94`, `diag` `n=48`):** New **`Series X release` record `14.65 FPS` med** (`4k–7k 15.30`, `7k–9k 11.70`, `maxMs 196.0 ms`); `diag` cut `inline_cbv` **`-85%`** (`11.55 ms`), upload buffers `177k -> 545`, Shared GPU to `3,425–3,842 MiB`, `refresh copied 71.7 MB` (-46%), `card_before 10.4 ms` (-46%), and GPU `card_sync 4.8 ms`. |
| Pass 31: Word-chunked `AddressHash::write` (8B/4B), `ListViews::BUF_CAP=32768`, lock-free `twins_written` / `reset_id`, & cached `root_view_cached` / `heap_buffers` / `heap_strides` / `RootSlot` | Verified (`Series X`) | **0 error / 0 crash / 0.00 null (`diag` `n=62`, `release` `n=69`):** `diag` `ALL` `10.50 FPS`, `4k–7k 12.30 FPS`, `9k+ p90 10.50 FPS`, `list_views evict 5,913`, `inline_cbv 9.50 ms`, `refresh copied 65.0 MB`, `pure root_views 10.70 ms`; `release` `9k+ 8.60 FPS` med (`p10 6.80`, `+31%`, `max 12.20 FPS`), `0` pressure. |
| Pass 32: Safe Ray Tracing activation (`MEMORY_CARD_BYTES` isolation, `2 GiB` / `92%` VRAM compact RT cap, Shared GPU fallback guard) & default `play-dev.ps1` to `series-x` + `VICTORY_BVH_BUILDER=1` | Verified (`Series X diag` + RT) | **0 error / 0 crash / 0 pressure / 0.00 null (`n=69`):** VRAM **`10.32 / 15.55 GB`**, Shared **`2.80 / 7.49 GB`**, `53/53` BVH builder pipelines active, RT arenas `684 MB`; isolated `501 ms/frame` pipeline-wait stall on cold `CS_RaytracingResolve` shaders (`2.80 FPS`, GPU `69%` idle vs `93–130 ms` GPU work). |
| Pass 33: Non-blocking RT/slow pipeline wait (`Pending::non_blocking` + `VICTORY_PIPELINE_WAIT_SLOW=1`), `REQUESTED` worker priority boost, & coalesced `memory::bring_up` + 64B-chunked `memory::merge` | Verified (`Series X` + RT) | **0 error / 0 crash / 0.00 null (`diag` `n=225`, `release` `n=72`, `-Warm`):** `waiting for pipelines` **`501.1 -> 0.00 ms`**, `dropped 0`, `GPU busy 85%`, `diag 4k–7k` **`6.50 FPS` (`+124%`)**; **`release` `ALL 5.90 FPS` med** (`avg 5.87`, `9k+ 5.40`, `max 12.70`, `cpu 84.1 / gpu 72.7 / back 49.4 ms`). |
| Pass 34: Skip builder scratch `gpu_writes` (`> 256 KiB`), static RT arena/directory refresh skip, zero-upgrade `card_dirty()`, HashSet `relay_card_back`, & parallel tree-reduced `CARD_COMPARE`/`torn::finish` | Built (`diag` & `release`) | Cuts RT `card_check` scratch overhead, redundant `memory.rs` full-arena refreshes, `card_dirty()` atomic churn, `relay_card_back` sorting/search, and serial GPU compare reduction; ready for live Series X RT test. |

To run locally from a normal (non-Administrator) PowerShell window with your own dump path and external cache:

```powershell
cargo build --locked --profile diag -p victory --features diagnostics
.\scripts\play-dev.ps1 -GameDir '<path-to-game-dump>' -Panel

# Faster release profile once verified:
cargo build --release --locked -p victory
.\scripts\play-dev.ps1 -GameDir '<path-to-game-dump>' -Build release -Panel
```

Detailed chronological measurements, progress percentages, input mapping, and hardware findings are recorded in [docs/progress.md](docs/progress.md), [docs/bug-journal.md](docs/bug-journal.md), and [docs/findings.md](docs/findings.md).

## How to run it

You need Windows 10/11 x64, a CPU with AVX2, a Direct3D 12 GPU with shader
model 6.6 and mesh shaders, Rust (the toolchain is pinned), the MSVC build
tools, and your own dump of the title.

```powershell
git clone https://github.com/avatarmanz/VIctory-7900XTX
cd VIctory-7900XTX
copy victory.example.toml victory.toml      # then set your paths, outside this folder
cargo build --locked --profile diag -p victory --features diagnostics
scripts\play.ps1 -Exe <path of your copy's executable>
```

`scripts\play.ps1 -Safe` leaves out the newest, faster ways; try it first when
something is wrong. `-RayTracing` turns the title's ray-traced indirect light on.

Before the first run, ahead of time (both can be stopped and go on later):

```powershell
target\diag\victory.exe shaders <the shader archives>   # every shader translated, minutes
target\diag\victory.exe warm --used                     # pipelines earlier runs drew with, for a new driver or build
```

Commands, debugging switches and upstream's own description:
[docs/README-fxceless.md](docs/README-fxceless.md). Every `VICTORY_...` switch is
described in a comment where it is read.

## What is different here

In short; the reasons and the measurements are in [docs/findings.md](docs/findings.md).

- **List calls are written down, not carried out, on the title's threads**
  (`VICTORY_THIN`): eight threads of ours translate them.
- **Submissions are prepared by a thread of ours and joined**
  (`VICTORY_ASYNC_SUBMIT`, `VICTORY_JOIN_PARTS`), with what can be spread over
  threads spread: refreshes, compares (AVX2), descriptor records, landings.
- **What shaders wrote comes back beside the write guards**, by the dwords that
  changed, on a thread of its own (`VICTORY_RELAY`, `VICTORY_LAND`).
- **Buffer fills that need bytes still on the card are done on the card**
  (`VICTORY_CARD_STEPS`) instead of waiting for it.
- **The title's writes are caught more cheaply**: one view opened at a fault,
  blocks that rest, places in the title's code that look before they write.
- **Textures go up on a copy queue of their own** and a slot shows one only after
  its upload ran, which also ended a device loss that came every 15 minutes or so.
- **Boats float**: textures that shaders write and the title's CPU reads go back
  to title memory (from fxceless and Geckomacabre, on by default here).
- **Frame record and replay** with per-pass times and shader variants compared
  turn by turn ([docs/replay.md](docs/replay.md)).

## Credits

This is a tree of other people's work with more on top. The history is kept, and
commits taken from elsewhere name the commit and its author.

- **[VIctory](https://github.com/Marvisak/VIctory)** by Marvisak and its
  contributors: the loader, the runtime services, the Direct3D 12 back end and
  the shader recompiler.
- **[fxceless/victory-fork](https://github.com/fxceless/victory-fork)**: the fork
  this one started from, and the people whose pull requests it carries.
- **[DexNom/VIctory](https://github.com/DexNom/VIctory)**: ray tracing fixes,
  cube gradients, sparse textures.
- **[Belobug/VIctory-Belobug-AMD](https://github.com/Belobug/VIctory-Belobug-AMD)**:
  the ambient light idea, pipeline workers, and work by romainhedouin.
- **[FQY55kai/ZJOU-VICTORY](https://github.com/FQY55kai/ZJOU-VICTORY)**.
- **[myokex/victory-amd](https://github.com/myokex/victory-amd)**: the guard over
  every view of a page (with Spexivus, joroyt and tightfall), and Joshua Odom's
  reads into title memory.
- **Geckomacabre** and **cxy0072019-sketch**: the texture read-back.
- **[XCL](https://github.com/ivall/xcl)** (GPL-2.0-only) was read for how the same
  title behaves under another layer; no code of it is here.
- This fork's changes were made with [Claude Code](https://claude.com/claude-code).

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE); the one file that is not
covered is named in [docs/asset-policy.md](docs/asset-policy.md).
