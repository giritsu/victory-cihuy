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
| Unbounded `thin::BACKS` pool (upstream `VICTORY_THIN=1`) | **Leak (~1.5–2 GB)** | Grew to **1,183 `host lists alive`** while `list::trim_idle()` thrashed `IDLE` (52,929 lists / 9,137 MB churn). Capped in Pass 3 (`64/32/16` + `thin::trim_idle()`). |
| Compact-RAM `window_kb=256` + `VICTORY_WINDOW_TAILS=1` | **Regression** | Created **6,408 overlapping windows** (`3,404 MB` covering `1,934 MB` = +1,470 MB pure duplication) and `34,682` window reallocations (`28,680 MB` churn). Reverted to `4 MiB` grid / tails off in Pass 3. |

### Next steps on this PC

1. **Re-test Pass 3 live in `diag`** (`.\scripts\play-dev.ps1 -GameDir '<path-to-game-dump>' -Panel`) to confirm `host lists alive`, `windows outside buffers`, `shared_gpu_used_mib`, and sustained gameplay FPS without `0x8007000E`.
2. **Pre-warm driver pipelines** (`target\diag\victory.exe warm --used` after gameplay sessions) to eliminate shader/pipeline compilation stutters (`dropped without a host pipeline: being made`).
3. **Optional BIOS tuning:** enabling **Above 4G Decoding + Re-Size BAR / Smart Access Memory** on Ryzen 5 5600 / RX 7600 XT can unlock `D3D12_HEAP_TYPE_GPU_UPLOAD` (`mapped_card_memory=true`), letting `shadow.rs` offload up to `1,536 MiB` of loose windows from Shared GPU Memory into 16 GB VRAM.
