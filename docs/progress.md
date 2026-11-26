# Local PC Progress & Input Reference (2026-11-26)

Workstation-specific progress tracker for running VIctory cleanly and playably on Windows 10 + 16 GB RAM + AMD Radeon RX 7600 XT 16 GB.
Chronological logs and deep technical notes live in [bug-journal.md](bug-journal.md) and [findings.md](findings.md).

## Target PC Specifications

- **OS & Display:** Windows 10 Pro (22H2, Build 19045), 1920x1080, `--console series-s`
- **CPU:** AMD Ryzen 5 5600 (6 Cores / 12 Threads, Zen 3, AVX2)
- **RAM / Pagefile:** 16 GB physical RAM + 32 GB pagefile (Windows WDDM `NON_LOCAL` Shared GPU Memory hard-capped at 50% physical RAM ≈ 7,490 MiB)
- **GPU:** AMD Radeon RX 7600 XT (16 GB VRAM, RDNA 3, `views=2097152`, `raw_twins=true`, `mapped_card_memory=false` while BIOS ReBAR/SAM is off)
- **Runtime & Cache:** DirectX 12 Agility SDK `1.614.1` (`D3D12SDKVersion=614`), pre-compiled shader cache (`25,782` shaders in `%LOCALAPPDATA%\VIctory\cache-pareto`)

## Overall & Per-Subsystem Progress

| Area | Progress | Current State |
|---|---|---|
| **Boot, Loader, Win10 Agility SDK & Shader Cache** | **95%** | Boots cleanly on Windows 10 (`0xC00` root signatures fixed, 29/29 upscaler pipelines created, `25,805` shader files / `914.3 MB` in `cache-pareto`). |
| **Visual & Rendering Core (RDNA 3)** | **85%** | World, characters, textures, and lighting render cleanly across 46-min Series S campaign and Series X `diag`: Pass 8 verified **`0.0 null`** raw buffer views (eliminating distant object flicker), `0` texture evictions (`0 let go 0 MB`), `SHOW_AFTER 0.57` frames, and `UPLOAD_AHEAD 45k` (`1.4 ms` avg). |
| **Input & Controls (XInput + Keyboard Stand-in)** | **85%** | XInput pads (slots 0–3, low/high rumble), merged `VICTORY_ONE_PAD=1` mode, full keyboard-to-pad mapping, and `F3` performance panel toggle; trigger rumble and mouse-to-camera mapping not yet implemented. |
| **16 GB RAM / WDDM Shared GPU Stability** | **95%** | **Pass 8 verified live on Series X (`diag`, 0 crashes):** Shared GPU **`3,983 / 7,490 MiB`** (~3.5 GiB headroom), VRAM **`11,512 / 14,781 MiB`** (~3.27 GiB headroom), `544` upload buffers (`113,083` spare reuses), and `0` gameplay pressure toggles (only `1` brief `6.7 s` loading blip at `physical_free_mib=95`). |
| **FPS Performance vs. 30 FPS Target (6C/12T CPU)** | **75–80%** | **Pass 12 (`Series X` `diag` & `release`, `0` error):** `Series X diag` median melonjak `8.8 -> 12.6 FPS` (**+43%**, max **`17.0 FPS`**), `release` peak **`16.8 FPS`** (`p10 8.6 FPS`, `9k+` draws avg `9.7 FPS`), RX 7600 XT sibuk **`82%` median / `91–95%` berat**, `too many steps` **`0`**, `same-list BEHIND_PLACED` **`0.00`**, `waiting for flights` **`0.0 ms`** median. **Pass 13** memangkas `refused` write-watch syscalls, kontensi lock `GUARDS`, overhead `flush_all`/`refresh_front`, serta `card_steps::plan` & `holder`. |

- **Overall "Playable & Stabil di PC ini": ~88–90%**
- **Overall "100% Mulus 30 FPS Konstan di Semua Scene": ~70–75%**

## Optimization Passes Summary (Pass 1–8)

| Pass | Focus | Key Result / Status |
|---|---|---|
| **Agility SDK (`1.614.1`)** | Export `D3D12SDKVersion=614` + `.\D3D12\` DLLs in `apps/victory` | Eliminated Win10 `0x80070057` (`c00/c01`) and startup `0xc0000005`. |
| **Pass 1** | `compact_host_ram()`, WDDM `NON_LOCAL` pressure hysteresis, 6C/12T worker scaling | Reached world rendering; uncovered `VICTORY_LEVELS_ON_DEMAND=1` staging bloat. |
| **Pass 2** | Default `held_only()`, 16 MiB staging pool, 100 ms `NON_LOCAL` polling, fast idle trim | Runtime ~1 min → **~7 min**; `texture_staging_mib` **2,104 → 54 MB**; `changed=` re-uploads **4,001+ → 142**. |
| **Pass 3** | Bounded `thin::BACKS` + revert `256 KiB` / `tails=1` loose window fragmentation | Reached **8m22s (frame 4,757)** at **8.5–10.7 FPS** (`diag`), holding Shared GPU flat at **6.1–6.4 GB** for 6 min. |
| **Pass 4** | `detach_take` + `finish_back()` + `VICTORY_CARD_BUFFERS_MIN_KB=256` | Ran **7m41s (frame 4,662)** and isolated remaining 640→849 `HostList` + `243k` card-buffer churn root cause. |
| **Pass 5** | Lazy `Front::recording`, `boxed -> fresh_list()`, 1-frame pool size, `8 MiB`/`500 ms` staging, `make_prelude` OOM retry | Ran **8m01s / frame 6,584** (**10.3–18.0 FPS** `diag`); startup `HostList` **640 → 23**, total lists **-92%**, staging **352 → 56 MB**. |
| **Pass 6** | `CardStaging::Ram` demand-zero RAM mirrors, VRAM loose-window offload, `send::SPARE` cap, `320` `IDLE` pool | **Verified live (46 min full main mission, 0 crash):** Shared GPU flat at **2.8–3.8 / 7.49 GB**, VRAM **12.3–12.6 / 14.95 GB**, `diag` **10.1–16.3 FPS** (min 4–31). |
| **Pass 7** | Compact RAM pressure threshold (`512 MiB`/commit), `192 MiB` scratch / `256 MiB` send caps, strip `Shape` filler words, `1,792 MiB` (`80%`) VRAM window balance, `left_out` diagnostic fix, `-NoKeepShown`/`-Warm` flags | **Verified live (Series X `diag`, 7 min, 0 crash):** `3,327` pre-warmed pipelines (`made=1`), `0/frame` non-RT drops, `0.00` empty indirects, `UPLOAD_AHEAD` (`3,790 MB`, `1.1 ms`), `SHOW_AFTER` (`0.60–0.69` frames), `distinct views kept=8,506`, Shared GPU `4,157–5,166 / 7,490 MiB`, `MakeResident` `40.9 s` (`~5.8 s/min`). Exposed `629–855/frame` null raw twins in `ListViews`, 14 false pressure toggles (`435–510 MiB` phys), and `224 MB` scratch peak. |
| **Pass 8** | Fix `ListViews` (`make_listed`) unaligned raw buffer twins via `(handle, staged_twin)` tuple, lower compact RAM physical floor (`256 / 512 MiB`), raise `scratch_spare_limit` (`256 MiB`) | **Verified live (Series X `diag`, 0 crash):** **`0.0 null`** raw twins (down from `629–855`), `0` evictions, `544` upload buffers (`113,083` reuses), Shared GPU **`3,983 / 7,490 MiB`**, VRAM **`11,512 / 14,781 MiB`**, `diag` median **`10.5 FPS`** (`8.2–12.3`), GPU **`15.8 ms`**. |
| **Pass 9** | CPU FPS pass: `card_steps` per-list `FillTo` + `plan()` zero-alloc + batched `descriptor::constant_slots`, lock-free `watch::ask_ahead_needed()` & `Shadow::pending_any`, chunked `wide_each_chunked`, `VICTORY_PLACED_ORDER=1`, `COPY_BESIDE=1`/`STORE_BESIDE=1`, `CARD_BUFFERS_MIN_KB=64`, 6C/12T pools | **Verified live (Series X `diag`, 4m50s, 0 crash):** `waiting for flights` **`15.8 -> 0.0 ms`**, `card_steps` **`9.8 -> 5.6–6.9 ms`**, guard waits **`-70%`** (`104.6 -> 31.3 ms`), Shared GPU **`3,444 / 7,490 MiB`**. Exposed `MIN_KB=64` overflowing `1,024 MiB` (`+144.7 MB/frame` CPU snapshots) and `STORE_BESIDE` + thread oversubscription (`109 ms` kernel, `7.5 FPS` `diag`). |
| **Pass 10** | Raise compact `card_buffers` cap (`1,536 MiB`), restore `MIN_KB=256`, balance pools (`4` threads, `WIDE_SPIN` off), wire `VICTORY_WRITE_SITES` + `-LearnWriteSites` | **Verified live (Series X `diag` & `release`, 0 crash):** Eliminated Pass 9's `144.7 MB/frame` snapshot spill and `109 ms` kernel contention (`snapshots 0.9–1.0 ms`, `write-sites.txt` `places=24`, `0.0 null` raw twins, `0` evictions, `refreshes 14.4–16.2 ms`, `diag 9.7–11.0 FPS`, `release` steady ~`10 FPS`). Isolated remaining `victory-submit` (`97.1 ms`) bottlenecks: `18.7 ms` `sync_heaps` wait, `16.8 ms` sync `land()` resets, `copied_before` sort/dedup waste, and `18.9 ms` (`0.27/frame`) `BEHIND_PLACED` stalls. |
| **Pass 11** | Static CPU `NullBank`, `copied_before` pre-filter, early `begin_sync_heaps()` overlap, async `victory-late-wait` `land()` offload, `cut_for` `BEHIND_PLACED` diagnostics, per-profile `play-dev` logs, 1s `victory_gpu::fps` log | **Verified live (`Series S` `release` & `Series X` `diag`, 0 crash):** `Series S release` median **`11.5 FPS`** (steady `12.4–13.2`, max **`14.1`**, `gpu=0.0 ms`), `Series X diag` **`13.8 FPS`** di scene ringan / **`8.8 FPS`** median rata-rata `8,898` draw. Mengisolasi 4 akar drop/stall `Series X`: `card_steps::MOST=12288` (`23–34 ms`), `same-list BEHIND_PLACED` (`0.12–0.43/frame`, `21–23 ms`), `heap_textures` sinkron `100–135 ms`, & antrean mutex `note_buffer`. |
| **Pass 12** | Raise `card_steps::MOST` (`32768`) & `BETWEEN` (`8 MiB`), inline GPU CBV copy via `HostList::constant_from_placed` for `same-list BEHIND_PLACED`, `VICTORY_TEXTURE_STAGE_MS=12`, lock-free `Slots` (`in_textures` & `in_placed`) on `HostHeap` | **Verified live (`Series X` `diag` & `release`, 0 crash):** `diag` median **`8.8 -> 12.6 FPS`** (**+43%**, max `17.0`), `release` peak **`16.8 FPS`** (`p10 8.6`, `9k+` draws avg `9.7 FPS`), `too many steps=0`, `same-list BEHIND_PLACED=0.00`, `waiting for flights` **`21–34 -> 0.0 ms`** median, `sync_heaps wait` **`13–22 -> 6.2 ms`**, `heap_views textures` **`100–135 -> 14.3 ms`**, RX 7600 XT **`82–95%` busy**, `0.0 null` raw twins, `0` eviksi. |
| **Pass 13 / 13b** | Eliminate refused `GetWriteWatch` calls (`writes_with_hint`, `PROTECTED` filter), O(1) `Blocks` count/bitwise `all`/`diff_range`, `PROTECTED` fast-path on `title_memory_written`, sorted `Journal::ahead`, merged `Shadow::CopiedState`, `FrontSpans` & chunk-8 `flush_all`, `card_steps::plan` fast-path + `O(W log W)` sweep-line `WriterIndex` + `MappedCache`, `HostHeap::keep_ref`, cumulative per-frame `VICTORY_TEXTURE_STAGE_MS` | **Pass 13 live-tested (`0` error) -> 13b hotfix built & tested:** Pass 13 memangkas `kernel askings refused` **`324–355 -> 4.0`** (-98.8%), lock wait `guards` **`89–154 -> 54.0 ms`**, `journal` **`5.1 -> 3.6 ms`**, dan mencapai peak `24.0 FPS` (`diag`); sekaligus mengungkap spike `O(W²)` `242.3 ms` di `WriterIndex`, chunk `32` refresh, dan `heap_textures 157 ms` yang langsung diperbaiki di **Pass 13b**. |

Details: [bug-journal.md](bug-journal.md) | Hardware findings: [findings.md](findings.md).

## Input & Hotkeys Reference

- **XInput Gamepad (Controller Xbox):** Didukung otomatis di slot 0–3 termasuk motor getar (*low/high frequency rumble*). Secara default `VICTORY_ONE_PAD=1` menggabungkan controller + keyboard menjadi 1 gamepad agar tidak muncul layar *controller disconnected* (`VICTORY_ONE_PAD=0` memisahkan tiap slot).
- **Keyboard Stand-in Gamepad** (aktif saat jendela game fokus, sumber: `crates/victory-hle/src/modules/gameinput.rs`):

| Keyboard | Fungsi Pad |
|---|---|
| `W` / `A` / `S` / `D` | Left Stick (atas / kiri / bawah / kanan) |
| `I` / `J` / `K` / `L` | Right Stick / Kamera |
| `Arrow Keys` (Panah) | D-Pad |
| `Enter` / `Backspace` / `X` / `Y` | Tombol `A` / `B` / `X` / `Y` |
| `Q` / `E` | `LB` / `RB` (*Left / Right Shoulder*) |
| `Z` / `C` | `LT` / `RT` (*Left / Right Trigger*) |
| `F` / `H` | `L3` / `R3` (*Left / Right Stick Click*) |
| `Esc` / `Tab` | `Menu` / `View` |

- **Keyboard Native & Mouse:** Keyboard juga diekspos sebagai device keyboard native ke `GameInput`, sedangkan **mouse belum di-mapping ke kamera/pad**.
- **Hotkey VIctory:** `F3` untuk menampilkan/menyembunyikan panel performa (*overlay* FPS/memori) saat dijalankan dengan `-Panel` (`VICTORY_PANEL=1`).

## Next Milestones

- [x] Uji live **Pass 6** (`46 menit` penuh tamat misi utama, **0 crash**, Shared GPU flat `2.8–3.8 / 7.49 GB`).
- [x] Uji live **Pass 7** di Series X `diag` (`7 menit`, **0 crash**, `3,327` pre-warmed pipelines `made=1`, `0.00` empty indirects, `UPLOAD_AHEAD` aktif `3,790 MB`, `distinct views kept = 8,506`, Shared GPU `4,157–5,166 / 7,490 MiB`).
- [x] Uji live **Pass 8** di Series X `diag` (**0 crash**, **`0.0 null`** raw buffer views, `0` eviksi VRAM, `544` upload buffers, Shared GPU `3,983 / 7,490 MiB`, median `10.5 FPS` di `diag` dengan GPU hanya `15.8 ms`).
- [x] Uji live **Pass 9** di Series X `diag` (`4m50s`, **0 crash**, `waiting for flights` **`15.8 -> 0.0 ms`**, guard waits **`-70%`**, `card_steps` `5.6–6.9 ms`; menemukan regresi `MIN_KB=64` & `STORE_BESIDE` fault/thread oversubscription).
- [x] Uji live **Pass 10** di `Series X` `release` & `diag` (**0 crash**, `write-sites.txt` aktif `places=24`, `0.0 null` raw twins, `refreshes 14.4–16.2 ms`, stabil di ~10 FPS; teridentifikasi bottleneck sisa di `sync_heaps` wait `18.7 ms`, sync `land()` `16.8 ms`, `copied_before`, dan `BEHIND_PLACED` `0.27/frame`).
- [x] Uji live **Pass 11** di `Series S` `release` (`11.5 FPS` median / `14.1` max, `0` error) & `Series X` `diag` (`13.8 FPS` max / `8.8` median, `0` error; mengisolasi 4 bottleneck `Series X`).
- [x] Uji live **Pass 12** di `Series X` `release` & `diag` (**0 crash / 0 error**, `diag` median **`8.8 -> 12.6 FPS`**, peak **`17.0 FPS`**, `too many steps = 0`, `same-list BEHIND_PLACED = 0.00`, `waiting for flights = 0.0 ms`, `RX 7600 XT` `82–95%` busy).
- [x] Uji live **Pass 13** di `Series X` `release` & `diag` (**0 error / 0 crash**, `askings refused` **`-98.8%`**, `guards` lock wait **`54 ms`**, terisolasi bug `O(W²)` `WriterIndex` + spike `heap_textures 157 ms`).
- [ ] Uji live **Pass 13b** di `Series X` `release` (`.\scripts\play-dev.ps1 -GameDir '<path-to-game-dump>' -Build release -Console series-x -Panel`) dan opsional `diag` (`dev-run-series-x-release.log` / `dev-run-series-x-diag.log`).
- [ ] *(Opsional BIOS)* Aktifkan **Above 4G Decoding + Re-Size BAR / AMD SAM** di BIOS untuk membuka `mapped_card_memory=true`.
