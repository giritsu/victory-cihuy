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
| **FPS Performance vs. 30 FPS Target (6C/12T CPU)** | **65–70%** | **Pass 8 (`diag` Series X):** median **`10.5 FPS`** (`8.2–12.3`), GPU busy only **`15.8 ms`** (`> 30 FPS` GPU headroom). **Pass 9** attacks CPU preparation bottlenecks (`card_steps` `FillTo`/batched `constant_slots`, `PLACED_ORDER=1`, lock-free `watch::ask_ahead`/`Shadow::pending_any`, chunked `wide_each`, `COPY_BESIDE=1`/`STORE_BESIDE=1`, and 6C/12T thread scaling). |

- **Overall "Playable & Stabil di PC ini": ~85–90%**
- **Overall "100% Mulus 30 FPS Konstan di Semua Scene": ~65–70%**

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
| **Pass 9** | CPU FPS pass: `card_steps` per-list `FillTo` + `plan()` zero-alloc + batched `descriptor::constant_slots`, lock-free `watch::ask_ahead_needed()` & `Shadow::pending_any`, chunked `wide_each_chunked`, `VICTORY_PLACED_ORDER=1`, `COPY_BESIDE=1`/`STORE_BESIDE=1`, `CARD_BUFFERS_MIN_KB=64`, 6C/12T pools | **Built & unit-tested (`116` passed, `1` ignored):** targets remaining CPU walls (`15.8 ms` `BEHIND_PLACED` settles, `9.8 ms` `card_steps`, `19.4–22.2 ms` refreshes, `104.6 ms` guard lock waits). Ready for `release` FPS test. |

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
- [ ] Uji live **Pass 9** di `release` (`.\scripts\play-dev.ps1 -GameDir '<path-to-game-dump>' -Build release -Console series-x -Panel`) untuk mengukur peningkatan FPS CPU murni (dan `-Build diag` untuk cek turunnya `settles`, `card_steps`, `refreshes`, serta `guards`).
- [ ] Uji opsi `-Safe` jika terjadi regresi visual/stabilitas dari `PLACED_ORDER` / `COPY_BESIDE` / `STORE_BESIDE`, serta opsional **Above 4G Decoding + Re-Size BAR / AMD SAM** di BIOS untuk membuka `mapped_card_memory=true`.
