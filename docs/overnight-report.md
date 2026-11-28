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
