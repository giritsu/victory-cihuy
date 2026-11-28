# Overnight Optimization Report (`wadaw-overnight`)

## Konfigurasi & Target
- **Branch:** `wadaw-overnight` (berangkat dari `340b16f`)
- **Hardware Uji:** AMD Ryzen 5 5600 (6C/12T), Radeon RX 7600 XT 16 GB VRAM, 16 GB System RAM, Windows 11 (`1920x1080`, `-Panel -NoRayTracing`)
- **Target Utama (`Series S release`):** Median **>= 20 FPS** (`draws >= 3000`), `p10 >= 15 FPS`, steady-state `maxMs <= 120 ms`, `0` VRAM eviction, Shared GPU `< 5.5 GB`, `0 ERROR`, visual 100% utuh.
- **Target Sekunder (`Series X release`):** Mendekati GPU-bound (`GPU busy >= 80%`) tanpa regresi.

### Pre-Pass Infrastructure (sebelum Pass 1)
- `a8a8297` — `chore(gpu): add opt-in VICTORY_SHOT_DIR full-res backbuffer+HUD PNG capture` ([shot.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/host/shot.rs), [host.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/host.rs)).
- `71bba27` — `fix(hle): add silent WASAPI/SpatialAudio fallback when no host audio endpoint is active` ([wasapi_null.rs](file:///d:/Codingan/victory-amd/crates/victory-hle/src/modules/audio/wasapi_null.rs)): mengatasi crash Frame-82 `0x80000003` (`victory-image-b99f876b74d4a77f.dll+0x4dcbc9d`) ketika layar/HDMI audio tidur dan Windows tidak memiliki endpoint render aktif.

---

## Pass 0 Baseline (`71bba27`)

Kedua run dijalankan via `scripts/play-dev.ps1 -Console series-s -Panel -NoRayTracing` selama >68 detik berturut-turut di in-game world (`draws >= 3000`) dengan input gerak/kamera horizontal (`W/A/D` + `J/L`).

| Run | STEADY>=3k Med FPS | ALL>=3k Med FPS | `p10` / `p90` (STEADY) | `ms_p99` / `maxMs` | HUD FPS / `1%` / `MAX` | GPU Busy / Working | `present wait` / `submit` | VRAM / Shared GPU | Error / Crash / Evict / Null Twins |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| **Series S `release`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-0/series-s-release/run.log)) | **13.50 FPS** (n=72, avg 7160 draws) | 13.20 FPS (n=82, avg 7221 draws) | 9.80 / 15.00 | 222.3 / 222.3 ms | 9.2–15.5 / `1%` 4–5 / `MAX` 77–162 | GPU 37–60% (37.5–45.3 ms) | `cpu` 55.8 ms / `back` 23.6 ms (HUD SUB 51.2–71.2 ms) | Local 7.7–8.2 GiB / Shared 3.3–3.4 GiB | `ERROR=0`, 0 crash, 0 evict |
| **Series S `diag`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-0/series-s-diag/run.log)) | **13.20 FPS** (n=73, avg 6622 draws) | 13.00 FPS (n=83, avg 6809 draws) | 10.70 / 15.80 | 204.0 / 204.0 ms | 14.5–16.4 / `1%` 6–13 / `MAX` 63–70 | **55%** (p90 60%) / 37.90 ms (`title` 29.50) | **23.00** (p90 40.50) / **67.30** (p90 81.80, max 112.80 ms) | 8759 MB / **3614–3677 MB** (HUD Local 7.8–8.5 / Shared 3.5–3.6 GiB) | `ERROR=0`, 0 crash, **0** evict, **0.00** null twins, `wasapi_null` aktif |

### Top Bottleneck CPU & Stutter dari Pass 0 `Series S diag`
1. **`submit worker` (`67.30 ms` med wall, `p90 81.80 ms`, `max 112.80 ms`, `present wait 23.00 ms` med / `p90 40.50 ms`):**
   - `submit refreshes wall`: `23.60 ms` (`p90 32.90 ms`) — **`refresh update all-thr 48.20 ms`**, menyalin **`110.10 MB`/frame**, dengan **`need-full = 772`/frame** (`mask 5.60 ms`).
   - `heap_pass wait`: `14.60 ms` (`p90 20.60 ms`) — `heap_pass slots 22.90 ms`, `heap_pass textures 9.30 ms` (`p90 24.40 ms`).
   - `sync_heaps total`: `27.36 ms` (`sync_heaps waited 11.46 ms`, `p90 15.18 ms`).
2. **`VICTORY_THIN` (`222.30 ms` all 4 threads = `~55.6 ms` wall, `p90 306.80 ms`, `max 469.60 ms`):**
   - `bind/prepare tables`: `120.30 ms` vs `draw::tables`: `99.30 ms` (**selisih `21.0 ms`** di luar `tables()`).
   - `listed_slot`: `47.90 ms` (`p90 63.60 ms`, `miss 20.10 ms`, HIT `~23 ms`, `mirror 4.80 ms`), dengan `list_views buf miss ~14–15k/frame` (`evict 4.9–14.1k`).
   - `direct_only`: `15.90 ms`, `inline_cbv`: `10.10 ms`, `readable`: `8.70 ms` (`p90 13.10 ms`), `root_views`: `20.00 ms` (`p90 38.70 ms`).
   - `thin: indirect`: `109.10 ms`, `dispatches`: `36.50 ms`, `draws`: `35.50 ms`, `other`: `25.80 ms` (`p90 43.00 ms`).
3. **`title hooks` (pada thread game):** `veh_fault 24.00 ms` (`guards lock 19.91 ms`, `p90 27.12 ms`), `mirror 13.50 ms` (`p90 25.80 ms`), `copy_desc 13.10 ms` (`p90 23.80 ms`), `placed 10.10 ms`.
4. **`relay landing` (`30.70 ms`):** `land merging wall 13.63 ms`.
5. **Memory:** Sistem 16 GB terpantau ketat (`physical_free_mib=236` sesaat pada frame 1918 memicu `host memory pressure` yang segera pulih di 515 MiB; Shared GPU aman `3.3–3.6 GiB`).

### Screenshot Pass 0 (Terverifikasi 3D World Normal)
- `Series S release`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-0/series-s-release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-0/series-s-release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-0/series-s-release/shot-3.png)
- `Series S diag`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-0/series-s-diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-0/series-s-diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-0/series-s-diag/shot-3.png)

---

## Pass 1 (`2793754`) — Zero-Clone `BufferSweep` Fast-Path, `mirror_for_pass`, & Incremental `CopySpans`

- **Target:** `heap_buffer_pass` ([draw.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/draw.rs), [descriptor/host.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/descriptor/host.rs), [descriptor.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/descriptor.rs)):
  1. Menambahkan `descriptor::view_alive(i, generation)` lock-free/zero-clone check tanpa `View::clone()` pada slot yang sudah ter-materialisasi.
  2. Menambahkan `HostHeap::mirror_for_pass` (`notify_changed = false`) agar `heap_buffer_pass` tidak menandai ulang slot baru ke `self.changed` untuk frame berikutnya.
  3. Mengganti `HeapRanges` + rebuild penuh `ByCopy` setiap pass dengan `SweepIndex` / `CopySpan` inkremental (40B per slot tanpa `Arc::clone` di slot) dan fast-path `prev.buf_epoch == cur_buf_epoch && prev.key == key && quick_matches && view_alive` tanpa `forget_part` atau `buffer_copy`.

### Hasil Pass 1 vs Pass 0 Baseline (`Series S`, `-Panel -NoRayTracing`)

| Metrik | Pass 0 `release` | Pass 1 `release` | Pass 0 `diag` | Pass 1 `diag` |
|---|---:|---:|---:|---:|
| **`STEADY>=3k` Median FPS** | 13.50 FPS (`n=72`) | **15.00 FPS (+11.1%)** (`n=74`) | 13.20 FPS (`n=73`) | **13.60 FPS (+3.0%)** (`n=74`) |
| **`STEADY>=3k` Avg / `p10` / `p90`** | 12.81 / 9.80 / 15.00 | **14.31 / 12.00 (+22.4%) / 16.00** | 12.80 / 10.70 / 15.80 | 13.21 / 10.00 / 16.00 |
| **`STEADY>=3k` `ms_p99` / `maxMs`** | 222.3 / 222.3 ms | 224.0 / 224.0 ms *(0 spike >=95ms setelah masuk)* | 204.0 / 204.0 ms | 204.9 / 204.9 ms |
| **`4k–7k` / `9k+` Med FPS** | 13.90 / 11.50 | **15.30 / 12.30** (`9k+ maxMs 94.0 ms`) | 13.90 / 11.60 | 13.90 / **12.50** |
| **HUD FPS / `1%` / `MAX`** | 9.2–15.5 / `1%` 4–5 / `MAX` 77–162 | **13.3–15.4 / `1%` 10 / `MAX` 68–95** | 14.5–16.4 / `1%` 6–13 / `MAX` 63–70 | 12.3–15.7 (`10032 draws` di `shot-2`) / `1%` 3–7 / `MAX` 65–93 |
| **GPU Busy / Working** | 37–60% (37.5–45.3 ms) | 57–67% (43.2–43.8 ms) | 55% / 37.90 ms | 53% / 36.90 ms |
| **`submit worker` / `present wait`** | `cpu` 55.8 / `back` 23.6 ms | `cpu` 54.3 / `back` 24.7 ms | 67.25 / 22.55 ms | **63.50 / 22.00 ms** |
| **`heap_pass slots` / `spans` / `wait_close`** | — | — | 22.85 / 4.50 / 14.55 ms | **21.60 / 1.40 (-68.9%) / 9.90 (-32.0%) ms** |
| **`VICTORY_THIN` / `other`** | — | — | 221.80 / 25.20 ms | **208.50 / 21.10 ms** |
| **VRAM / Shared GPU / RAM** | 7.7–8.2 / 3.3–3.4 GiB | 7.8–8.5 / 3.4–3.6 GiB (RAM 13.9–14.6 GiB) | 8759 / 3614–3677 MB | 8760 / **3546 MB** (19410 MB priv, 425 MB free) |
| **Stabilitas** | `ERROR=0`, 0 crash, 0 evict | **`ERROR=0`, 0 crash, 0 evict** | `ERROR=0`, 0 evict, `0.00` null twins | **`ERROR=0`, 0 evict, `0.00` null twins** |

### Log & Screenshot Pass 1 (Terverifikasi 3D World Normal)
- `Series S release` ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-s-release/run.log)): [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-s-release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-s-release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-s-release/shot-3.png)
- `Series S diag` ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-s-diag/run.log)): [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-s-diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-s-diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-s-diag/shot-3.png)

---

## Pass 2 (Reverted) — Defer Object-Buffer Sync + `Tracked` Journal Fast-Exit

- **Target:** `sync_heaps` (`descriptor.rs`) + `watch.rs` (`Tracked::take` + `Journal::last_note`).
  - Memindahkan sinkronisasi `View::Shader`/`View::Unordered` buffer object (`kind == 0`) dari mirror langsung ke antrean tunda `out.wait`, menggabungkan `before` + `make` ke satu panggilan `land::each_view(true, ...)`, dan menambahkan fast-exit `full_note` + single `journal()` lock di `Tracked::take`.
- **Hasil Live Test (vs Pass 1 `2793754`):**
  - **`Series S release`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-2/series-s-release/run.log)): `STEADY>=3k` (`n=71`, avg 6195 draws): med **14.90 FPS** (vs 15.00), avg **13.72** (vs 14.31), **`p10` 8.80 FPS** (**`-26.7%` regresi** vs 12.00 FPS), `p90` 16.30, `maxMs` **220.6 ms** (`ALL>=3k` `maxMs 309.1 ms`); HUD `1%` turun menjadi **2–4** (vs 10 di Pass 1), dengan hitch **2.7 FPS / `FRAME 375.8 ms` (`SUB 46.7 ms`, `MAX 719`, `CPU 32%`, `GPU 12%`)** tertangkap pada `shot-2.png`. `ERROR=0`, 0 crash, RAM `13.8–14.2/15.9 GiB`, `pressure_events=1`.
  - **`Series S diag`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-2/series-s-diag/run.log), `69.9s` in-world): `STEADY>=3k` (`n=73`, avg 6397 draws): med **14.20 FPS** (vs 13.60), `p10` 10.20 (vs 10.00), `maxMs` **273.2 ms** (vs 204.9 ms); `sync_heaps total` `25.99 -> 23.91 ms` (`waited 9.09 -> 7.20 ms`, `objects 6650 -> 1481`), `heap_pass slots 21.60 -> 18.70 ms`, `wait_close 9.90 -> 7.90 ms`; namun `waited_nd` ("waited and no draw came") melonjak **2923 -> 5292** dan `sync_heaps making` naik **7.76 -> 9.81 ms**, `refresh wall` naik `22.80 -> 25.30 ms`, `guards lock` `19.04 -> 20.03 ms`. `ERROR=0`, 0 crash, `0.00` null twins, `0` evict, RAM `13.9–14.9 GiB`, Shared `3.3–3.4 GiB`.
- **Keputusan (Aturan 4 — Auto-Revert):** **GAGAL / REVERTED** ke `2793754` karena `Series S release` mengalami regresi tajam pada `p10` (`12.00 -> 8.80 FPS`) dan `1%` low (`10 -> 2–4`) disertai frame stall `375.8 ms`, serta penundaan buffer objek menggandakan `waited_nd` dan menaikkan biaya `sync_heaps making`.
- **Screenshot Pass 2 (Visual World Normal):**
  - `Series S release`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-2/series-s-release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-2/series-s-release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-2/series-s-release/shot-3.png)
  - `Series S diag`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-2/series-s-diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-2/series-s-diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-2/series-s-diag/shot-3.png)

---

## Pass 3 (Reverted) — Single-Call `mirror_for_pass` (`PassSlot`), `put_with_twin`, & `merge_sorted_dedup`

- **Target:** `heap_buffer_pass` & `mirror` ([descriptor/host.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/descriptor/host.rs), [draw.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/draw.rs), [host/aside.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/host/aside.rs)).
  - Menghilangkan lookup `buffer_copy` ganda melalui `PassSlot` di `mirror_for_pass`, menggabung `put + put_twin` menjadi satu `CopyDescriptors(2)` via `put_with_twin`, memakai `keep_ref` + `Slots::put_if_absent`, `merge_sorted_dedup`, `AT_A_TIME = 256`, serta registrasi thread `victory-aside` di `copystat`.
- **Hasil Live Test (vs Pass 1 `2793754`):**
  - **`Series S release`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-3/series-s-release/run.log)): `STEADY>=3k` (`n=71`, avg 6059 draws): med **14.20 FPS** (`-5.3%` vs 15.00), avg **13.65** (vs 14.31), **`p10` 9.20 FPS** (`-23.3%` vs 12.00), `p90` 16.80, `maxMs` 222.8 ms (`9k+` `12.10 FPS`, `maxMs` 95.9 ms); HUD `1%` `2–9`, RAM memuncak **`15.5/15.9 GiB`** saat `shot-3.png` (`pressure_events=1`). `ERROR=0`, 0 crash.
  - **`Series S diag`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-3/series-s-diag/run.log), `69.8s` in-world): `STEADY>=3k` (`n=71`, avg 6992 draws): med **13.00 FPS** (vs 13.60), `p10` **7.00 FPS** (vs 10.00), `maxMs` 189.6 ms (`ALL>=3k` `271.9 ms`); `heap_pass slots` **21.20 ms** (vs 21.60 ms, `-1.8%`), `wait_close` **9.90 ms** (vs 9.90 ms), `title hook mirror` **13.90 ms** (vs 14.30 ms), `VICTORY_THIN` **209.00 ms** (vs 208.50 ms), terungkap CPU **`aside` `7.6–10.3 + 0.0–0.5 ms`** dan `victory-views` `9.4–12.8 ms`. `ERROR=0`, 0 crash, `0.00` null twins, `0` evict, RAM `14.7–15.6/15.9 GiB`, Shared `3.5–3.6 GiB`.
- **Keputusan (Aturan 4 — Auto-Revert):** **GAGAL / REVERTED** ke `2793754` karena bottleneck target (`heap_pass slots 21.60 -> 21.20 ms`, `wait_close 9.90 -> 9.90 ms`) nyaris tidak bergerak sementara `Series S release` turun (`15.00 -> 14.20 FPS` med, `p10 12.00 -> 9.20 FPS`) disertai tekanan RAM (`15.5–15.6 GiB`). Temuan penting `diag`: `aside` hanya ~8–10 ms CPU sedangkan sebagian besar biaya `heap_buffer_pass` ternyata bukan di materialisasi `mirror_for_pass`.
- **Screenshot Pass 3 (Visual World Normal):**
  - `Series S release`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-3/series-s-release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-3/series-s-release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-3/series-s-release/shot-3.png)
  - `Series S diag`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-3/series-s-diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-3/series-s-diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-3/series-s-diag/shot-3.png)

---

## Pass 4 (Reverted) — `CheckMinMax` Range Pruning, 4 KiB Diff-Copy (`copy_unwatched`), & Threshold `should_wide_update`

- **Target:** `submit refreshes` critical path ([shadow.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/shadow.rs), [watch.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/watch.rs), [host.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/host.rs)).
  - Menambahkan `CopiedState::max_page_ts`, `RangedCheck::CheckMinMax`/`CheckRanged(min_t, max_t)`, serta `JournalRead::ranged_check_minmax` untuk mem-prune `need-full` (`<= 512` halaman / 32 MiB); memparalelkan diff-copy 4 KiB (`copy_mapped`) pada card-backed unwatched buffer (`>= 256 KiB`) dan menurunkan ambang `should_wide_update` (`>= 2` span dengan total `>= 64 KiB`).
- **Hasil Live Test (vs Pass 1 `2793754`):**
  - **`Series S release`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-4/series-s-release/run.log)): `STEADY>=3k` (`n=71`, avg 6880 draws): med **14.20 FPS** (`-5.3%` vs 15.00), avg **13.33** (vs 14.31), **`p10` 9.50 FPS** (`-20.8%` vs 12.00), `p90` 16.20, `ms_p99 / maxMs` 179.6 / 179.6 ms (`ALL>=3k` 253.8 ms), `4k–7k` 13.90 (vs 15.30), `9k+` 12.30 (86.5 ms); HUD `6 / 16 / 15 FPS`, `1%` `6–9`, `MAX` `67–294`, RAM `14.2–14.9 / 15.9 GiB`, GPU `19–64%`. `ERROR=0`, 0 crash, `pressure_events=1`.
  - **`Series S diag`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-4/series-s-diag/run.log), `70.0s` in-world): `STEADY>=3k` (`n=74`, avg 6445 draws): med **14.10 FPS** (vs 13.60), `p10` 11.60 (vs 10.00), `maxMs` 95.2 ms (`ALL>=3k` 155.4 ms); `refresh need-full` **`671–772 -> 426.00`** (`-37%`), `refresh MB copied` **`91–110 -> 72.50 MB`**, `card sent MB` **`53.6 -> 38.30 MB`** (`-28.5%`, upload `0x...1412200` turun `21.05 -> 0.00 MB`), `GPU card_sync` `4.0 -> 3.10 ms`; namun `copy_mapped` per-4 KiB (~10.240 syscall `ReadProcessMemory`/frame) membuat CPU thread `wide` melonjak **`31.7 -> 50.5 ms` (+18.8 ms)** dan memundurkan critical path **`submit worker` (`63.50 -> 68.30 ms`)**, `submit refreshes wall` (`21.70 -> 25.30 ms`), serta `card_mem` (`4.90 -> 6.40 ms`). `ERROR=0`, 0 crash, `0.00` null twins, `0` evict, RAM `14.1–15.2 / 15.9 GiB`.
- **Keputusan (Aturan 4 — Auto-Revert):** **GAGAL / REVERTED** ke `2793754` karena `Series S release` mengalami regresi (`15.00 -> 14.20 FPS` med, `p10 12.00 -> 9.50 FPS`, `1%` turun `10 -> 6–9`) dan `submit worker` critical path memburuk (`63.50 -> 68.30 ms`) akibat overhead syscall 4 KiB `ReadProcessMemory`.
- **Screenshot Pass 4 (Visual World Normal):**
  - `Series S release`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-4/series-s-release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-4/series-s-release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-4/series-s-release/shot-3.png)
  - `Series S diag`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-4/series-s-diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-4/series-s-diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-4/series-s-diag/shot-3.png)

---

## Verifikasi Akhir `Series X release` pada `2793754` (Pass 1)

Setelah kode dikembalikan bersih ke `2793754`, build `release` (`--console series-x -Panel -NoRayTracing`) diuji live selama `68.3s` berturut-turut di in-game world (`draws >= 3000`):

- **`Series X release`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-x-release/run.log)):
  - `STEADY>=3k` (`n=76`, avg `7288` draws): **med `12.10 FPS`**, avg `10.92`, `p10 5.70`, `p90 14.70`, `ms_p99 / maxMs 282.8 / 282.8 ms`, `cpu 40.8 ms`, `back 26.0 ms` (`4k–7k` `10.10`, `7k–9k` `11.70`, `9k+` `9.20 FPS`).
  - `ALL>=3k` (`n=86`, avg `7293` draws): med `11.00 FPS`, avg `10.41`, `p10 5.50`, `p90 14.60`, `maxMs 282.8 ms`.
  - **HUD 4K (`3840x2160`):** pada scene berat (`shot-3`, `9351 DRAWS`), GPU RX 7600 XT mencapai **`79%` busy (`67.6 ms` GPU vs `59.8 ms` SUB)**, **`12 FPS`** (`11.7 FPS`), Local VRAM `10.9 GiB`, Shared GPU **`3.7 GiB`**, System RAM **`15.3/15.9 GiB`**; (`shot-2`, `3932 DRAWS`: `8 FPS`, `35%` GPU, Local `10.8 GiB`, Shared `3.7 GiB`, RAM `15.4/15.9 GiB`).
  - **Stabilitas:** `ERROR=0`, 0 crash, `0` eviksi VRAM, Shared GPU aman `3.3–3.7 GiB` (`< 5.5 GB`), `pressure_events=1`. *(Catatan screenshot: `shot-1.png` sempat tertangkap tepat saat transisi kamera `1 DRAWS` sebelum harness me-reset timer 68.3s in-world; `shot-2.png` dan `shot-3.png` menampilkan 3D world 4K penuh yang bersih dan normal.)*
- **Screenshot `Series X release` (`2793754`):** [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-x-release/shot-1.png) *(frame transisi `1 DRAWS`)*, [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-x-release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-1/series-x-release/shot-3.png).

---

## Kesimpulan & Batas Berhenti (Aturan 5)

- **State Terkomit Terbaik (`wadaw-overnight`, tanpa push ke remote):** **`2793754`** (Pass 1 — Zero-Clone `BufferSweep`, `mirror_for_pass`, & Incremental `CopySpans`):
  - **`Series S release`:** `STEADY>=3k` median meningkat **`13.50 -> 15.00 FPS` (+11.1%)**, `p10` **`9.80 -> 12.00 FPS` (+22.4%)**, HUD `1%` low naik **`4–5 -> 10` (`MAX 68–95 ms`)**, `0 ERROR`, `0` crash, `0` eviksi VRAM, `0.00` null raw twins, Shared GPU aman `3.4–3.6 GiB`.
  - **`Series X release`:** terverifikasi bersih `12.10 FPS` med (`9k+` `9.20 FPS`, `79%` GPU busy di `9351` draws 4K), `0 ERROR`, `0` crash, `0` eviksi VRAM, Shared GPU `3.7 GiB`.
- **Alasan Berhenti (Aturan 5b):** Tiga pass berturut-turut setelah Pass 1 (Pass 2, Pass 3, dan Pass 4) gagal memberikan peningkatan >3% FPS atau >10% penurunan spike pada `Series S release` tanpa regresi stabilitas/frametime dan otomatis di-revert ke `2793754`.
- **Temuan Teknis untuk Iterasi Berikutnya:**
  1. Pada CPU **Ryzen 5 5600 (6C/12T) + 16 GB RAM**, total kerja CPU lintas seluruh thread berada di **~580–640 ms/frame** (saturasi 6C/12T ~80–95%) sementara `physical_free` di akhir streaming kerap menyentuh `236–515 MiB` sehingga memicu noise `±5–10%` `p10`. Target 20 FPS stabil pada PC ini membutuhkan pemangkasan total CPU wall + contention yang jauh lebih besar tanpa menambah syscall kernel.
  2. Pelajaran Pass 4: melakukan diff-copy via 10k syscall 4 KiB `ReadProcessMemory` menambah `+18.8 ms` biaya CPU/kernel walaupun menghemat `-15.3 MB` GPU upload — implementasi berikutnya untuk `0x...1412200` harus membaca range secara besar/langsung (bukan 10k×4 KiB syscall) atau langsung membandingkan di user-space/SIMD jika aman.
  3. Kandidat terbersih untuk pass selanjutnya (di luar sesi berhenti ini): eliminasi `clr_epoch` `0.6/frame` yang mengosongkan 32k entri `ListViews` (per-`Shadow` `alive` bit), cache `intern_shape` TLS lock-free di `descriptor.rs`, serta peningkatan shard `GUARDS` (`1024`) untuk memangkas `17–22 ms` guard lock wait.

---

## Baseline Walk-Far `Series X` 4K pada `2793754` (`aed81e1`)

Pengukuran awal skenario jalan-jalan jarak jauh (`--console series-x`, 4K `3840x2160`, `-Panel -NoRayTracing`, ~79s berturut-turut di world `draws >= 3000`) dari `HEAD` bersih `aed81e1` (`2793754`):

- **`Series X release` Walk-Far** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/release/run.log)):
  - `STEADY>=3k` (`n=70`, avg `8621` draws): **med `9.95 FPS`**, avg `10.03`, `p10 8.00`, `p90 12.80`, `ms_p99 / maxMs 227.9 / 227.9 ms` (`4k-7k` `10.85`, `7k-9k` `10.50`, `9k+` `9.70 FPS`; pada jendela `n=98` penuh termasuk transisi: med `9.90 FPS`, `p10 7.10`, `maxMs 662.0 ms`, `9k+` `9.20 [p10 4.40]`). HUD `1% 2–4`, `GPU 55–85%`, `LOCAL 10.6–11.2 GiB`, `SHARED 3.7–3.8 GiB`, `RAM 15.3–15.5/15.9 GiB`, `ERROR=0`, `pressure=3`.
- **`Series X diag` Walk-Far** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/diag/run.log)):
  - `STEADY>=3k` (`n=70`, avg `8619` draws): **med `11.90 FPS`**, avg `12.00`, `p10 11.00`, `p90 13.70`, `maxMs 119.6 ms` (`4k-7k` `13.75`, `7k-9k` `11.80`, `9k+` `11.80`). `GPU` `64.3 ms` (`79%` busy), `VICTORY_THIN 249.6 ms`, `listed_slot 54.8 ms`, `veh_fault 29.4 ms` (`already 16.5 ms/150`, `open 12.7 ms/602`), `guards wait 7.4 ms`, `refreshes 17.3 ms wall` (`update 45.4 ms`, `need-full 870`), `list_views tex 11216 hit / 1 miss (clr epoch 0.0)`, `memory: 25,663 MB private`, `1,102 MB` unmapped placed textures (`0 let go`), `3,668–6,763` queued texture slots, `0 ERROR`, `0.00` null raw twins, `0` evict.
- **Screenshot Baseline `Series X`:**
  - `release`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/release/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/release/shot-4.png), [shot-5.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/release/shot-5.png)
  - `diag`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/diag/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/diag/shot-4.png), [shot-5.png](file:///d:/Codingan/victory-amd/out/overnight/series-x-baseline/diag/shot-5.png)

---

## Pass 5 (Reverted) — `TextureQueue::due` 4-Bucket Priority & Reclaim Placed Textures Tanpa First Page (`let_go_idle`)

- **Target:** [descriptor/host.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/descriptor/host.rs) (`TextureQueue::due`) & [resource.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/resource.rs) (`let_go_idle`).
  - Memisahkan antrean `TextureQueue::due` menjadi 4 bucket (`partly` -> `fresh` 3:1 `retry` -> `recheck`) dan membebaskan tekstur placed yang `!committed(data, 1)` di `let_go_idle` (`UNMAPPED_IDLE_MS = 2_000`, `MAX_UNMAPPED_PER_SWEEP = 256`).
- **Hasil Live Test (vs Baseline `Series X` `2793754`):**
  - **`Series X release`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-5/release/run.log)): `ALL>=3k` (`n=76`, avg `5403` draws): med `11.95 FPS`, **`p10 5.50 FPS`** (vs `7.90`), `maxMs 342.7 ms` (vs `227.9`); `STEADY>=3k` (`n=66`, avg `4827` draws): med `13.50`, **`p10 6.50 FPS`** (vs `8.00`), namun per bucket draws turun pada beban sama: **`4k-7k` `9.50` (vs `10.85`), `7k-9k` `8.90` (vs `10.50`)**, `9k+` `11.25` (`n=4`). HUD `COMMIT` turun **`22.3–23.7 GiB`** (`LOCAL 9.9–10.7 GiB`), `1% 2–9`, `RAM 15.5–15.8/15.9 GiB`, `ERROR=0`, `pressure=2`.
  - **`Series X diag`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-5/diag/run.log)): `STEADY>=3k` (`n=68`, avg `6352` draws): med `12.45 FPS`, `p10 9.90` (vs `11.00`), `maxMs 204.8 ms` (vs `119.6`), `4k-7k 13.80`, `7k-9k 11.60`; dead unmapped placed turun **`1,102 -> 610 MB`** (`VRAM 12,050 -> 11,021 MB`), `6,303 let go 548 MB`, namun:
    - **`192 made again 36 MB`** (> batas `50`, karena `!committed(r.data(), 1)` hanya mengecek halaman pertama sementara tekstur mip-streamed masih dipakai dari mip lebih kecil);
    - **`let_go_idle` memicu `invalidate_texture_views()` hampir tiap sweep**, membuat `ListViews` **`tex_clr_epoch` naik `0.0 -> 4.8/frame`** dan **`tex miss` melonjak `1 -> 1,214/frame`** (`listed_slot 54.8 -> 65.9 ms`, `miss 21.3 -> 29.2 ms`);
    - `heap_pass textures` naik **`5.1 -> 20.1 ms`** dan hook `placed` naik `13.2 -> 20.3 ms`. `ERROR=0`, `0.00` null twins, `0` evict.
- **Keputusan (Aturan 4 — Auto-Revert):** **GAGAL / REVERTED** ke `aed81e1` (`2793754`) karena `made again = 192` (`> 50`), regresi `p10` (`8.00 -> 6.50` / `ALL p10 7.90 -> 5.50`) & bucket `4k-7k`/`7k-9k` pada `release`, serta `tex_clr_epoch = 4.8` pada `ListViews`.
- **Screenshot Pass 5 (Visual World Normal):**
  - `Series X release`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-5/release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-5/release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-5/release/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/pass-5/release/shot-4.png)
  - `Series X diag`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-5/diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-5/diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-5/diag/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/pass-5/diag/shot-4.png)

---

## Pass 6 (Reverted) — `GUARDS` 1024-Sharding & `RECENTLY_OPENED` Fast-Path `veh_fault already`

- **Target:** [watch.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/watch.rs) (`GUARD_SHARDS = 1024`, `GUARD_STRIPE_SHIFT = 2`, `RECENTLY_OPENED[1024]` + `LAST_FAST_ALREADY`).
- **Hasil Live Test (vs Baseline `Series X` `2793754`):**
  - **`Series X release`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-6/release/run.log)): `ALL>=3k` (`n=75`, avg `7138` draws): med `10.30 FPS`, **`p10 5.60`** (vs `7.90`), `maxMs 348.1 ms` (vs `227.9`); `STEADY>=3k` (`n=65`, avg `6836`): med `10.40`, **`p10 6.10`** (vs `8.00`), `7k-9k` `10.05` (vs `10.50`), `9k+` `12.30` (`n=5`). HUD `shot-1` sempat stall `2 FPS` (`FRAME 482.3 ms`, `GPU 443.3 ms`). `ERROR=0`, `pressure=1`.
  - **`Series X diag`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-6/diag/run.log)): `STEADY>=3k` (`n=67`, avg `9111`): turun **`11.90 -> 9.60 FPS` med**, `p10 11.00 -> 7.80`, `maxMs 119.6 -> 205.3 ms` (`9k+ 9.45` vs `11.80`). `veh_fault already` hanya turun `16.5 -> 12.6 ms` (`136` calls — masih memicu `VirtualQuery`/overhead), sementara `kernel` time naik **`64 -> 91 ms/frame`** dan `user` naik **`655 -> 690 ms/frame`** (`256 KiB` stripes meningkatkan frekuensi iterasi shard). `ERROR=0`, `0.00` null twins, `0` evict, `24,715 MB` private.
- **Keputusan (Aturan 4 & Re-plan Trigger):** **GAGAL / REVERTED** `crates/` ke `aed81e1` (`2793754`) karena `veh_fault already` gagal turun di bawah `3 ms` (`12.6 ms`), `release p10`/`maxMs` & `diag` FPS mengalami regresi, serta kernel time meningkat.
- **Screenshot Pass 6 (Visual World Normal):**
  - `Series X release`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-6/release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-6/release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-6/release/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/pass-6/release/shot-4.png)
  - `Series X diag`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-6/diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-6/diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-6/diag/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/pass-6/diag/shot-4.png)

---

## Pass 7 (Reverted) — `CheckMinMax` Range Pruning & Bulk 256 KiB `safecopy` Diff-Copy

- **Target:** [watch.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/watch.rs), [shadow.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/shadow.rs), [host.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/host.rs).
  - Menambahkan `JournalRead::ranged_check_minmax` (`<= 2048` halaman) dan `Shadow::copy_unwatched_diff` (256 KiB `safecopy` bulk + 4 KiB diff run) untuk buffer card unwatched `>= 256 KiB`.
- **Hasil Live Test (vs Baseline `Series X` `2793754`):**
  - **`Series X release`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-7/release/run.log)): `ALL>=3k` (`n=76`, avg `7721`): med **`9.35 FPS`** (vs `10.00`), **`p10 5.40`** (vs `7.90`), `maxMs 406.6 ms` (vs `227.9`); `STEADY>=3k` (`n=66`, avg `7506`): med **`9.70 FPS`** (vs `9.95`), **`p10 6.30`** (vs `8.00`), `maxMs 250.7 ms` (`4k-7k 10.60`, `7k-9k 9.70`, **`9k+ 8.20` vs `9.70`**). HUD `1% 2–5`, `RAM 15.2–15.6/15.9 GiB`, `ERROR=0`, `pressure=2`.
  - **`Series X diag`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-7/diag/run.log)): `STEADY>=3k` (`n=65`, avg `6087`): turun **`11.90 -> 10.90 FPS` med**, **`p10 11.00 -> 5.70`**, `maxMs 235.7 ms` (`4k-7k 12.10` vs `13.75`, `7k-9k 8.10` vs `11.80`).
    - Positif: `refresh need-full` turun **`870 -> 473`**, `card sent` turun **`37.7 -> 30.0 MB`** (`95k -> 25k` ranges).
    - Negatif: memproses `min_t/max_t` sebelum `ranged != 0` di `classify_ranged_pages` menjatuhkan **`ranged-hit` (`2018 -> 60`)** dan menggandakan **`exact-miss` (`392 -> 840`)**; sementara 256 KiB staging + diff CPU di worker `wide` menaikkan `refresh update` **`45.4 -> 67.1–81.2 ms`** (`74.0 -> 93.2 MB`), `submit refreshes wall` **`17.3 -> 29.7 ms`**, `heaps wait_close` `1.6 -> 18.6 ms`, dan **`VICTORY_THIN` `249 -> 295–325 ms`**. `ERROR=0`, `0.00` null twins, `0` evict.
- **Keputusan (Aturan 4 — Auto-Revert):** **GAGAL / REVERTED** `crates/` ke `aed81e1` (`2793754`) karena regresi `release` (`9.95 -> 9.70`, `p10 8.00 -> 6.30`, `9k+ 9.70 -> 8.20`) & `diag` (`11.90 -> 10.90`), serta lonjakan beban CPU `update` & `VICTORY_THIN`.
- **Screenshot Pass 7 (Visual World Normal):**
  - `Series X release`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-7/release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-7/release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-7/release/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/pass-7/release/shot-4.png)
  - `Series X diag`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-7/diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-7/diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-7/diag/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/pass-7/diag/shot-4.png)

---

## Pass 8 (Reverted) — Eliminasi CPU Murni pada `VICTORY_THIN` (`draw::tables` + `ListViews`) & Hook Deskriptor

- **Target:** [descriptor.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/descriptor.rs), [resource.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/resource.rs), [descriptor/host.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/descriptor/host.rs), [draw.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/draw.rs), [host.rs](file:///d:/Codingan/victory-amd/crates/victory-gpu/src/host.rs).
  - *Hook deskriptor:* `SHADER_HEAPS_LO`/`HI` fast-reject sebelum `SHADER_HEAPS.read()` pada `mirror()`/`mirror_range()`, cache TLS direct-mapped 1024-slot `SHAPE_TLS` untuk `intern_shape` + `record_placed` fast-path.
  - *`ListViews` & `HostTexture`:* `HostTexture::view_gen` (`AtomicU32`) per tekstur menggantikan `invalidate_texture_views()` global pada `let_go_idle`/`levels_changed`/`levels_changed_after`/`texture_views_remade`, pengecekan `ListView::is_fresh()` + slot reuse `tex_next`/`tex_free` + probe `prefer_tex` untuk `is_table_record`, lewati tulis atomik `self.twins` saat `!record_slot_meta`, filter direct-mapped 16-slot `CopyBatch::last_kept_keys`.
  - *`draw::tables` & hashing:* `#[inline]` pada seluruh trait `Hasher` `AddressHash`, cache single-pass `State::table_records` saat hashing tabel, serta batching `lv_misses` di `plan.direct` ke satu `with_readable_table_epochs`.
- **Hasil Live Test (vs Baseline `Series X` `2793754`):**
  - **`Series X release`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-8/release/run.log), 75.3s in-world, `ERROR=0`, `pressure=3`):
    - `ALL>=3k` (`n=74`, avg `7065` draws): med `10.05 FPS` (vs `10.00`), avg `9.73`, **`p10 5.80`** (vs `7.90`), **`maxMs 769.3 ms`** (vs `227.9`), `cpu 52.4 ms` (vs `45.1`), `back 26.6 ms`.
    - `STEADY>=3k` (`n=64`, avg `6787` draws): med `10.05 FPS` (vs `9.95`), **`p10 6.80`** (vs `8.00`), `maxMs 239.6 ms` (vs `227.9`).
    - Per bucket `draws`: **`4k-7k` `10.45` (vs `10.85`, `maxMs 186.7` vs `125.6`)**, **`7k-9k` `10.10` (vs `10.50`, `p10 7.60` vs `8.10`, `maxMs 239.6` vs `183.9`, `cpu 56.1` vs `45.3`)**, **`9k+` `9.10` (vs `9.70`, `p10 5.00` vs `7.90`)**. HUD: `6–12 FPS`, `1% 1–4`, `MAX 120–267 ms`, `GPU 43–86%` (`64.5–70.7 ms`), `LOCAL 10.3–11.1 GiB`, `SHARED 3.8–4.0 GiB`, `RAM 14.2–15.6/15.9 GiB`, `COMMIT 23.1–23.6 GiB`.
  - **`Series X diag`** ([run.log](file:///d:/Codingan/victory-amd/out/overnight/pass-8/diag/run.log), 76.2s in-world, `ERROR=0`, `0.00` null twins, `0` evict):
    - Per bucket `draws`: **`4k-7k` turun `13.75 -> 11.60 FPS`** (`p10 13.20 -> 9.70`, `maxMs 76.8 -> 108.6`, `cpu 43.9 -> 56.3`), **`7k-9k` `11.80 -> 11.55 FPS`** (`p10 10.70 -> 9.40`, `maxMs 119.6 -> 134.3`).
    - Perbandingan subsistem pada beban draws setara (`8,840` vs `8,719` draws):
      - Positif: `tex clr epoch` turun **`5.2 -> 0.0`**, `tex miss` turun **`1,360 -> 58`**, `listed_slot` **`76.1 -> 70.5 ms`**.
      - Regresi/Flat: `hash` naik **`5.6 -> 7.6 ms`** (push `Vec` per slot di `table_records`), `direct` naik **`15.6 -> 17.8 ms`** (alokasi `Vec` `lv_misses`/`resolved`), `readable` **`17.6 -> 24.6 ms`**, `placed` `11.2 -> 11.5 ms`, `mirror` `21.7 -> 22.1 ms`, `copy_desc` `20.3 -> 21.1 ms`.
- **Keputusan (Aturan 4 — Auto-Revert):** **GAGAL / REVERTED** `crates/` ke `aed81e1` (`2793754`) via `git checkout -- crates/` karena regresi konsisten di semua bucket `draws` pada `release`, `p10` (`8.00 -> 6.80` / `ALL 7.90 -> 5.80`), `maxMs` (`227.9 -> 769.3 ms`), serta `diag` `4k-7k` & `7k-9k`.
- **Screenshot Pass 8 (Visual World Normal):**
  - `Series X release`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-8/release/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-8/release/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-8/release/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/pass-8/release/shot-4.png)
  - `Series X diag`: [shot-1.png](file:///d:/Codingan/victory-amd/out/overnight/pass-8/diag/shot-1.png), [shot-2.png](file:///d:/Codingan/victory-amd/out/overnight/pass-8/diag/shot-2.png), [shot-3.png](file:///d:/Codingan/victory-amd/out/overnight/pass-8/diag/shot-3.png), [shot-4.png](file:///d:/Codingan/victory-amd/out/overnight/pass-8/diag/shot-4.png)

---

## Ringkasan Stop Bersih (`Series X` Pass 5–8)

Sesuai batas berhenti (**3 pass Fase 2 berturut-turut non-progresif: Pass 6, Pass 7, dan Pass 8**), optimasi berhenti bersih di kode terkomit terbaik **`2793754`** (`crates/` 100% identik dengan `2793754`).

1. **Kesimpulan Fase 1 (Visual / Tekstur & Model Hitam):**
   - Diperiksa langsung pada **34 screenshot 4K** lintas baseline dan Pass 5–8 (`0 ERROR`, `0` crash, `0.00` null raw twins, `0` VRAM eviction). Seluruh geometri dunia, karakter, kendaraan, jalan, vegetasi, dan bangunan ter-render penuh dengan tekstur tajam; area gang/dinding teduh gelap adalah pencahayaan direct-sun tanpa GI saat `-NoRayTracing` (`docs/README-fxceless.md`), bukan bug streaming atau translasi shader.
2. **Temuan Akar Masalah Struktural Fase 2 (`Series X` 4K pada Ryzen 5 5600 6C/12T + 16 GB RAM + RX 7600 XT):**
   - **Saturasi CPU 6C/12T (`~715–719 ms/frame` total CPU):** GPU 4K membutuhkan `67.4–68.9 ms/frame` (batas keras ~`14.5 FPS` di 9k draws), namun saturasi semua core/thread membuat perubahan apa pun yang menambah alokasi heap/diff CPU (`wide` di Pass 7, `Vec` per-draw/tabel di Pass 8) atau kernel syscall (`256 KiB` stripes di Pass 6) langsung memakan jatah worker `VICTORY_THIN`.
   - **Saturasi RAM 16 GB (`15.3–15.8 / 15.9 GiB`, `25.6 GB` private commit):** Memicu paging SSD Windows saat jalan-jalan jauh (`1% low 1–4`, `p10 5.4–8.0`, spike `maxMs 228–769 ms`).
3. **Track Paling Menjanjikan untuk Sesi Berikutnya:**
   - **Pemangkasan RAM fisik 16 GB murni:** memangkas `542` idle `HostList` (`1,579 MB` private driver RAM di `list.rs` `idle_most`/`idle_trim_keep`) serta mengaudit `1,799 MB` alokasi sendiri dan `3,356 MB` windows di luar buffer (`1,057 MB` tak pernah tersentuh).
   - **Isolasi tunggal `per-HostTexture view_gen`:** mempertahankan kemenangan Pass 8 yang terbukti (`tex_clr_epoch 5.2 -> 0.0`, `tex miss 1,360 -> 58`, `listed_slot 76.1 -> 70.5 ms`) secara terpisah **tanpa** alokasi `table_records` / `lv_misses` `Vec` yang memicu regresi `hash` dan `direct`.
