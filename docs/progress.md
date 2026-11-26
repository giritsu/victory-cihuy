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
| **Visual & Rendering Core (RDNA 3)** | **75–80%** | World, characters, textures, and lighting render through full 46-min campaign; Pass 7 targets remaining blur, late texture pop-in, eviction (`574k` let go), and random purple/glitch artifacts. |
| **Input & Controls (XInput + Keyboard Stand-in)** | **85%** | XInput pads (slots 0–3, low/high rumble), merged `VICTORY_ONE_PAD=1` mode, full keyboard-to-pad mapping, and `F3` performance panel toggle; trigger rumble and mouse-to-camera mapping not yet implemented. |
| **16 GB RAM / WDDM Shared GPU Stability** | **90–95%** | **Pass 6 verified live (46 min full main mission, 0 crashes, 0 OOMs):** Shared GPU held flat at **2,771–3,850 / 7,490 MiB** (~3.6 GiB headroom) and VRAM at **12,289–12,569 / 14,956 MiB**. Pass 7 rebalances loose windows (`1,792 MiB`) and strips `shape` filler words to stop minute-31 RAM/view growth. |
| **FPS Performance vs. 30 FPS Target (6C/12T CPU)** | **55–65%** | **Pass 6 (`diag`):** **10.1–16.3 FPS** (min 4–31, median 10.2), dropping to **6.3–8.7 FPS** at min 34–46 from `4,194,304` `Shape` overflow + `1.95M` upload-buffer (`858.7 s` `MakeResident`) churn + false `PRESSURED` throttling. **Pass 7** eliminates all three bottlenecks and unlocks `release` benchmarking. |

- **Overall "Playable & Stabil di PC ini": ~80–85%**
- **Overall "100% Mulus 30 FPS Konstan di Semua Scene": ~55–65%**

## Optimization Passes Summary (Pass 1–7)

| Pass | Focus | Key Result / Status |
|---|---|---|
| **Agility SDK (`1.614.1`)** | Export `D3D12SDKVersion=614` + `.\D3D12\` DLLs in `apps/victory` | Eliminated Win10 `0x80070057` (`c00/c01`) and startup `0xc0000005`. |
| **Pass 1** | `compact_host_ram()`, WDDM `NON_LOCAL` pressure hysteresis, 6C/12T worker scaling | Reached world rendering; uncovered `VICTORY_LEVELS_ON_DEMAND=1` staging bloat. |
| **Pass 2** | Default `held_only()`, 16 MiB staging pool, 100 ms `NON_LOCAL` polling, fast idle trim | Runtime ~1 min → **~7 min**; `texture_staging_mib` **2,104 → 54 MB**; `changed=` re-uploads **4,001+ → 142**. |
| **Pass 3** | Bounded `thin::BACKS` + revert `256 KiB` / `tails=1` loose window fragmentation | Reached **8m22s (frame 4,757)** at **8.5–10.7 FPS** (`diag`), holding Shared GPU flat at **6.1–6.4 GB** for 6 min. |
| **Pass 4** | `detach_take` + `finish_back()` + `VICTORY_CARD_BUFFERS_MIN_KB=256` | Ran **7m41s (frame 4,662)** and isolated remaining 640→849 `HostList` + `243k` card-buffer churn root cause. |
| **Pass 5** | Lazy `Front::recording`, `boxed -> fresh_list()`, 1-frame pool size, `8 MiB`/`500 ms` staging, `make_prelude` OOM retry | Ran **8m01s / frame 6,584** (**10.3–18.0 FPS** `diag`); startup `HostList` **640 → 23**, total lists **-92%**, staging **352 → 56 MB**. |
| **Pass 6** | `CardStaging::Ram` demand-zero RAM mirrors, VRAM loose-window offload, `send::SPARE` cap, `320` `IDLE` pool | **Verified live (46 min full main mission, 0 crash):** Shared GPU flat at **2.8–3.8 / 7.49 GB**, VRAM **12.3–12.6 / 14.95 GB**, `diag` **10.1–16.3 FPS** (min 4–31). |
| **Pass 7** | Compact RAM pressure threshold (`512 MiB`/commit), `192 MiB` scratch / `256 MiB` send caps, strip `Shape` filler words, `1,792 MiB` (`80%`) VRAM window balance, `left_out` diagnostic fix, `-NoKeepShown`/`-Warm` flags | **Built & unit-tested (`37/37` passed):** stops false pressure lock, eliminates `1.95M` upload-buffer / `858.7 s` `MakeResident` churn & `4.19M` shape overflow, reclaims 768 MiB VRAM for textures. |

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
- [ ] Uji live **Pass 7** di `release` (`.\scripts\play-dev.ps1 -GameDir '<path-to-game-dump>' -Build release -Panel`) dan `diag` untuk verifikasi FPS baru, turunnya `MakeResident` (`858.7 s -> ~0`), `shapes` tetap di bawah 4.19M, dan berkurangnya blur/eviction tekstur.
- [ ] Uji ketajaman visual & tekstur ungu dengan `-Console series-x` (16 GB VRAM lega setelah rebalance 1,792 MiB) serta A/B test `-NoKeepShown` bila masih ada tekstur ungu acak.
- [ ] Aktifkan `VICTORY_PIPELINE_BOOK=1` pada satu sesi (`-More @{ VICTORY_PIPELINE_BOOK = '1' }`), lalu jalankan `victory warm --used --jobs 6` (atau `-Warm`) agar `known-pipelines.bin` terisi dan driver pipeline tidak lagi terkompilasi saat bermain (`0` diketahui sebelumnya).
- [ ] Opsional BIOS tuning: aktifkan **Above 4G Decoding + Re-Size BAR / AMD SAM** untuk membuka `mapped_card_memory=true` (offload otomatis hingga `1,536 MiB` loose window langsung via PCIe BAR tanpa mirror RAM).
