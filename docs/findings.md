# What was learned on the RX 7900 XTX (2026-11-23 to 2026-11-25)

One PC: i9-12900K (8 fast cores, 8 slow ones, 24 threads), 64 GB, Radeon RX 7900 XTX
24 GB, Windows 11, 1440p, Series X profile. Figures are of diagnostics builds with a
screenshot a second. Scenes are not the same from run to run, so read differences
under about 5 % as nothing. What a switch does is in the comment where it is read.

## Where a frame goes

Median of whole runs, same part of the prologue: 280 ms, then 124, 103, 75, now 46-65.
On the water (first boat mission) 46-53 ms.

The title needs 33 ms. Of a 55 ms frame on the water:

- **The thread that prepares submissions: 48 ms.** It is the wall. Steps of the lists
  18 (refreshes of host copies 10), steps done on the card 7, waiting for descriptor
  records 6, asking the write watch 5, copies in the card's memory 4, the title's bytes
  under shader writes 4.
- **The card: about 41 ms busy**, the next wall.
- **List calls: 36,000 a frame, 230 ms of work** spread over eight threads. In a street
  chase 67,000 calls and 430 ms.
- Descriptor records copied as plain bytes: 400,000 slots looked at, 40,000 mirrored a
  frame, most of them empty.
- Landing what shaders wrote: 16 MB a frame put as dwords, on a thread of its own.

## The one idea behind most of it

On the console the CPU and the graphics processor share memory. Here they do not, so
everything one writes and the other reads must be noticed and moved. Every large cost
is one of four: **noticing** writes of the title (write watch, guards), **noticing**
writes of shaders (hashes on the card, compares on the CPU), **moving** them, and
**waiting** until an order is safe. Gains came from making each of them narrower
(dwords, not pages), wider (threads, AVX2) or later (on the card, behind the frame),
in that order of size:

| What | Frame before | after |
|---|---|---|
| Descriptor records of draws' tables left to the draws | 280 ms | about 124 |
| Our hash passes over card copies, 64 threads a piece | card 79 ms | 66 |
| Preparing on a thread of its own, refreshes and records spread | 103 | 75 |
| List calls written down and translated by eight threads (`VICTORY_THIN`) | 75 | 68 |
| Submissions joined, card steps, textures sent ahead | 68 | about 63 |
| Title bytes under shader writes compared with AVX2 on six threads | 14 ms of it | 4-7 |
| Stores into guarded memory looked at first, fewer guards reset | 69 ms of threads' time | about 6 |

(Each figure is from the run in which it was measured; they overlap and do not add up.)

## Faults found, and what they were

- **Boats sank, the sea plane went under.** The ocean's height and displacement maps are
  textures that compute passes write and the title's physics sample on the CPU. They were
  never brought back to title memory. Fixed by Geckomacabre's read-back as merged in
  fxceless/victory-fork f5c03f3b; on by default here (`VICTORY_TEXTURE_BACK=0`: off).
  With it the boat stays up through a whole run; without it sank in every one.
- **A device loss about every 16 minutes**: a page fault inside a texture made in the same
  tenth of a second. A slot showed it before its upload had run.
  `VICTORY_SHOW_AFTER=1`; none in the sessions since (likely fixed, not proven).
- **A rare lost write** (6 of 200 runs of a stress test): a view was mapped between our
  look at its guards and our lock, and views were opened one after another. None in 603
  runs after.
- **Blocky patches**: indirect dispatches cut at 65,535 groups.
- **Square holes and dashed edges in shadows**: pixel shaders that read their quad's
  neighbours with wave reads; on AMD helper lanes take no part in them.
- **Cube reflections too blurred, wrong materials**: DXIL's `Log` is base 2, and
  normalized buffer codes were one step off.
- **Every ray missed**: the dispatch flag that says counts are threads, not groups.

## Measured and not worth it

- Unordered early pipelines for everything (AMD's compiler crashed).
- Series S profile, smaller textures, a second drawing queue, `ASYNC_SUBMIT` off.
- Mapping pages on sixteen threads: no more pages mapped, frames longer.
- Scalar buffer loads four dwords a call (`VICTORY_SHADER_OPT=127`): no gain on this card.
- Wave sizes, fused multiply-adds, fast reciprocals, raw picked buffers, leaving out
  barriers: nothing to 0.1 ms each. Exact RDNA float rules (`VICTORY_RDNA_FLOAT`): 0.8 ms.
- Running the console's shader code natively on the card: the Windows driver takes no raw
  code, the two generations differ, and a Vulkan host layer ends at the same compiler. What
  is left to gain from shader code was guessed at under 1 ms.
- An on-screen panel of numbers (`VICTORY_PANEL`): it helps finding where a frame goes but
  costs some, so it is off and F3 shows it.

## What is next

1. The preparing thread: split what one submission does over two threads in a row; ask
   for the next submission while this one is sent; view the card's own buffers where fills
   copy them now.
2. The card: small buffers' copies in its own memory (`VICTORY_CARD_BUFFERS_MIN_KB=64`,
   after fxceless c36971b2; not measured here yet), and the passes that run for nothing
   without rays.
3. Stalls: 270-660 s of a session were full of frames over 200 ms, almost all while new
   pipelines were made. `victory warm`, and a quarter of the workers for the slow ones.
4. Outdoor flicker: suspected to be one bad indirect record that uses up its list's
   budget, so that every later draw of it is empty. The log's frame line counts them;
   `VICTORY_INDIRECT_REST=0` is the experiment.
5. Memory, 40-46 GB private: lists' allocators 9 GB, windows 7, uploads 4
   (`VICTORY_MEMORY_MAP=1` lists it).

## The other repositories, compared

Read up to 2026-11-25: Marvisak/VIctory, fxceless/victory-fork, DexNom/VIctory,
Belobug/VIctory-Belobug-AMD, FQY55kai/ZJOU-VICTORY. What was taken is in the history,
each commit naming its source. In short:

- Marvisak's commits hardly apply any more; nearly all of what they fix is here, often
  another way. Taken: nearest normalized codes, d16 views, shader model flags.
- fxceless: the base. Newest taken: texture read-back, small buffers on the card, hashes
  over a buffer's own bytes.
- DexNom: thread counts of ray dispatches, cube gradients, joined sparse ranges.
- Belobug: pipeline workers for screen passes, the ambient stand-in's idea, pipeline and
  constant buffer bookkeeping (romainhedouin).
- ZJOU: render targets not set again; some shader corrections are being looked at.

## Windows 10 + 16 GB RAM + RX 7600 XT 16 GB (2026-11-26)

Local workstation telemetry (`Ryzen 5 5600` 6C/12T, 16 GB RAM + 32 GB pagefile, RX 7600 XT 16 GB VRAM, 1080p, `--console series-s`):

### Platform constraints measured

- **Windows 10 D3D12 inbox runtime lacks SM 6.6 dynamic resource flags (`0xC00`):** fixed by shipping Agility SDK `1.614.1` (`D3D12SDKVersion=614` + `.\D3D12\D3D12Core.dll` beside the binary).
- **WDDM Shared GPU Memory hard cap (~7,490 MiB on 16 GB RAM):** expanding `C:\pagefile.sys` to 32 GB raises process commit headroom (~15–24 GB free) and prevents Rust heap aborts, **but does not raise WDDM `NON_LOCAL` budget** (fixed at 50% of physical RAM). Exhausting `NON_LOCAL` triggers `0x8007000E` (`E_OUTOFMEMORY`) on upload/shared buffers and command list closes even while ~2.8 GB of local VRAM remains free.
- **`card memory the CPU maps supported=false` without BIOS ReBAR/SAM:** when `GPUUploadHeapSupported` is false, all loose windows remain in `NON_LOCAL` Shared GPU Memory rather than VRAM BAR.

### Measured wins vs. regressions (16 GB RAM tuning)

| Change tested | Outcome | Evidence (`dev-run.log`) |
|---|---|---|
| Disable `VICTORY_LEVELS_ON_DEMAND` default (`held_only()` active) + 16 MiB staging pool + 100 ms `NON_LOCAL` polling | **Major win** | Session runtime **~1 min -> ~7 min**; `texture_staging_mib` **2,104 MB -> 54 MB**; `changed=` texture re-uploads **4,001+ -> 142**. |
| Unbounded `thin::BACKS` + `Front::recording` + `boxed()` bypassing `IDLE` | **Major leak & churn (fixed Pass 3–6)** | Caused `1,183` -> `849` `host lists alive` (`~1.5 GB` Shared GPU) and `153 host lists made/frame` (`339,267` total), `243,192` card buffers, and `206.6 s` of `MakeResident`/`mapped()` waits. Pass 5 (lazy `Front::recording`, `boxed -> fresh_list`, `8 MiB`/`500 ms` compact staging) cut startup lists **640 -> 23**, total lists **-92%**, card buffers **-78%**, `MakeResident`+`mapped()` **206.6 -> 89.6 s**, and staging blocks **352 -> 56 MB**, raising Pass 5 runtime to **6,584 frames / 8m01s** (**10.3–18.0 FPS** `diag`). Pass 6 raises `IDLE`/`BACKS` to `320/256` to cover the 222-list gameplay peak. |
| Compact-RAM `window_kb=256` + `VICTORY_WINDOW_TAILS=1` | **Regression** | Created **6,408 overlapping windows** (`3,404 MB` covering `1,934 MB` = +1,470 MB pure duplication) and `34,682` window reallocations (`28,680 MB` churn). Reverting to `4 MiB` grid / tails off cut window count to **733–1,078**. |
| `Card::staging` (`shared_buffer`) + loose windows in `NON_LOCAL` (Pass 6) | **Verified win (46 min)** | Moving `Card::staging` to `CardStaging::Ram` and offloading loose windows into free VRAM completed the **46-minute full main mission with 0 crashes**: Shared GPU held flat at **2,771–3,850 / 7,490 MB** (`3.6 GB` headroom), VRAM at **12.3–12.6 / 14.95 GB**. |
| `32 MiB` scratch cap, false `3 GiB` RAM pressure, `Shape` filler words, & `2,560 MiB` VRAM crowding (Pass 7) | **Verified win (Series X)** | Live 7-min Series X `diag` run (**0 crashes**): `3,327` pre-warmed pipelines loaded (`made=1`), `0/frame` non-RT drops, `0.00` empty indirects, `UPLOAD_AHEAD` (`140,000` textures / `3,790 MB`, `1.1 ms`), `SHOW_AFTER` cut `1.24–1.33 -> 0.60–0.69` frames, `distinct views kept = 8,506`, Shared GPU **`4,157–5,166 / 7,490 MB`**, VRAM **`13,018–13,191 / 14,859 MB`**, and `MakeResident` cut to `40.9 s` (`~5.8 s/min` vs `~18.7 s/min` in Pass 6). Exposed `629.1–855.0/frame` null raw buffer views in `ListViews` (`make_listed`), 14 false pressure toggles (`435–510 MiB` phys), and `224 MB` Series X scratch peak. |
| `ListViews` raw twin check (`staged(false, 1)`) + `512 MiB` compact phys floor (Pass 8) | **Verified win (Series X)** | Live Series X `diag` confirmed **`0.0 null`** raw buffer views (eliminating distant object flicker), `0` texture evictions (`0 let go 0 MB`), `544` upload buffers (`113,083` spare reuses), `0` gameplay pressure toggles, Shared GPU **`3,983 / 7,490 MB`**, VRAM **`11,512 / 14,781 MB`**, `diag` median **`10.5 FPS`** (`8.2–12.3`), GPU busy **`15.8 ms`**. |
| `card_steps` per-slot overhead, `watch`/`pending` locks, `wide_each` overhead, & conservative 6C/12T pools (Pass 9) | **Implemented & tested (`116` passed)** | Batched `descriptor::constant_slots` + per-list `FillTo` + lock-free `ask_ahead_needed()` / `Shadow::pending_any` + `wide_each_chunked`, paired with `VICTORY_PLACED_ORDER=1`, `COPY_BESIDE=1`, `STORE_BESIDE=1`, `CARD_BUFFERS_MIN_KB=64`, `WIDE_SPIN=50`, and `5–6` worker threads per pool in `scripts/play-dev.ps1`. |
| Compact `8 MiB` / `500 ms` staging pool + mild-pressure scratch preservation | **Verified (Pass 5)** | Replaced `16 MiB` / `2,500 ms` staging pool (`352 MB` fragmented) and 250 ms scratch flushing: live Pass 5 telemetry confirmed staging pool blocks dropped **352 -> 56 MB**. |

### Next steps on this PC

1. **Benchmark Pass 9 live on Series X (`release` and `diag`)** (`.\scripts\play-dev.ps1 -GameDir '<path-to-game-dump>' -Build release -Console series-x -Panel`) to measure pure `release` FPS against the 30 FPS goal, and check `diag` settles (`BEHIND_PLACED`), `card_steps`, `refreshes`, and `guards`.
2. **Use `-Safe` fallback if needed:** `-Safe` keeps `CARD_BUFFERS_MIN_KB=256`, disables `UNDER_WIDE`/`LAND_ECHO`, and omits `PLACED_ORDER` / `COPY_BESIDE` / `STORE_BESIDE` to isolate regressions cleanly.
3. **Optional BIOS tuning:** enabling **Above 4G Decoding + Re-Size BAR / Smart Access Memory** on Ryzen 5 5600 / RX 7600 XT unlocks `D3D12_HEAP_TYPE_GPU_UPLOAD` (`mapped_card_memory=true`), letting `shadow.rs` map up to `1,536 MiB` of loose windows directly over PCIe BAR without a RAM mirror copy.
