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
| Pass 17: Budgeted `let_go_idle` (`512/3ms`, urgent `2048/8ms`) + `select_nth_unstable` + `host_written_snapshot` + single batch `retire`; private non-descriptor-heap buffers, `SPARE_SHARED`/`upload_cpu` + reusable `place_between` + `idle_most`/`idle_trim_keep`; chunked `PROTECT_CHUNK=64` `guard()` + batched `cover_run` | Built & tested | Eliminates `607.2 ms` texture-sweep stalls, cuts unnecessary `mapped()` fences & ephemeral buffer churn (`30.2 s` residency waits), and splits `GUARDS` write-lock holds into 64-block chunks. |

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
