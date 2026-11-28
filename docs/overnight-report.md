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

## Pass 1 — Zero-Clone `BufferSweep` Fast-Path, `mirror_for_pass`, & Incremental `CopySpans`

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
