//! Opt-in full-resolution swap-chain + HUD PNG capture via `VICTORY_SHOT_DIR`.
//!
//! Zero per-frame overhead when `VICTORY_SHOT_DIR` is unset. When set, polls
//! at most once every 200 ms for `<dir>/snap.req`, reads back the presented
//! backbuffer (after the overlay panel is drawn), and writes `<dir>/<name>.png`
//! asynchronously on a background thread.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Graphics::Direct3D12::{
    D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_READBACK,
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT, D3D12_RANGE, D3D12_RESOURCE_DESC,
    D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_STATE_COPY_DEST,
    D3D12_TEXTURE_COPY_LOCATION, D3D12_TEXTURE_COPY_LOCATION_0,
    D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT, D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
    D3D12_TEXTURE_LAYOUT_ROW_MAJOR, ID3D12Device, ID3D12GraphicsCommandList, ID3D12Resource,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
    DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_SAMPLE_DESC,
};

const POLL_EVERY: Duration = Duration::from_millis(200);

fn dir() -> Option<&'static PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| std::env::var_os("VICTORY_SHOT_DIR").filter(|s| !s.is_empty()).map(PathBuf::from))
        .as_ref()
}

/// Polls `<VICTORY_SHOT_DIR>/snap.req` (throttled to 200 ms). Returns the
/// sanitized output PNG path when a capture is requested.
pub(super) fn poll_request() -> Option<PathBuf> {
    let folder = dir()?;
    static LAST_POLL: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let mut guard = LAST_POLL.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if guard.is_some_and(|prev| now.saturating_duration_since(prev) < POLL_EVERY) {
            return None;
        }
        *guard = Some(now);
    }
    let req_path = folder.join("snap.req");
    let raw = std::fs::read_to_string(&req_path).ok()?;
    let _ = std::fs::remove_file(&req_path);
    let trimmed = raw.trim();
    let name = std::path::Path::new(trimmed).file_name()?.to_string_lossy().into_owned();
    if name.is_empty() {
        return None;
    }
    Some(folder.join(name))
}

/// Records a full-resolution readback copy of `texture` (assumed in `COPY_SOURCE`).
pub(super) fn read_back(
    device: &ID3D12Device,
    list: &ID3D12GraphicsCommandList,
    texture: &ID3D12Resource,
) -> Option<(ID3D12Resource, D3D12_PLACED_SUBRESOURCE_FOOTPRINT)> {
    let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
    let mut total = 0u64;
    // SAFETY: `texture` is a live swap-chain resource; output pointers are valid.
    unsafe {
        let desc = texture.GetDesc();
        device.GetCopyableFootprints(&desc, 0, 1, 0, Some(&mut footprint), None, None, Some(&mut total));
    }
    if total == 0 {
        return None;
    }
    let heap = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_READBACK,
        ..Default::default()
    };
    let desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: total,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        ..Default::default()
    };
    let mut buffer: Option<ID3D12Resource> = None;
    // SAFETY: Standard readback buffer allocation and GPU copy command recording.
    unsafe {
        device
            .CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COPY_DEST, None, &mut buffer)
            .ok()?;
        let buffer = buffer?;
        let to = D3D12_TEXTURE_COPY_LOCATION {
            pResource: std::mem::transmute_copy(&buffer),
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: footprint },
        };
        let from = D3D12_TEXTURE_COPY_LOCATION {
            pResource: std::mem::transmute_copy(texture),
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
        };
        list.CopyTextureRegion(&to, 0, 0, 0, &from, None);
        Some((buffer, footprint))
    }
}

/// Maps the completed readback buffer, copies its bytes on the caller thread,
/// and encodes + saves the PNG atomically on a background worker thread.
pub(super) fn save_async(
    path: PathBuf,
    buffer: &ID3D12Resource,
    footprint: &D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
) {
    let f = footprint.Footprint;
    let pitch = f.RowPitch as usize;
    let total = pitch.saturating_mul(f.Height as usize);
    if total == 0 || f.Width == 0 || f.Height == 0 {
        return;
    }
    let mut data = std::ptr::null_mut();
    // SAFETY: Caller has waited for GPU execution; map/copy/unmap is bounded by `total`.
    let mapped = unsafe {
        let range = D3D12_RANGE { Begin: 0, End: total };
        if buffer.Map(0, Some(&range), Some(&mut data)).is_err() || data.is_null() {
            return;
        }
        let slice = std::slice::from_raw_parts(data.cast::<u8>(), total);
        let owned = slice.to_vec();
        buffer.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
        owned
    };
    let format = f.Format;
    let width = f.Width;
    let height = f.Height;
    let _ = std::thread::Builder::new().name(String::from("victory-shot")).spawn(move || {
        let Some(png) = encode_surface_png(format, width, height, pitch, &mapped) else {
            tracing::warn!(format = format.0, "shot skipped: unsupported backbuffer format");
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("png.tmp");
        if std::fs::write(&tmp, &png).is_ok() {
            if std::fs::rename(&tmp, &path).is_err() {
                let _ = std::fs::write(&path, &png);
                let _ = std::fs::remove_file(&tmp);
            }
        }
    });
}

fn encode_surface_png(
    format: DXGI_FORMAT,
    width: u32,
    height: u32,
    pitch: usize,
    rows: &[u8],
) -> Option<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => {
            for y in 0..h {
                raw.push(0);
                let row = rows.get(y * pitch..y * pitch + w * 4)?;
                for px in row.chunks_exact(4) {
                    raw.extend_from_slice(&px[..3]);
                }
            }
        }
        DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => {
            for y in 0..h {
                raw.push(0);
                let row = rows.get(y * pitch..y * pitch + w * 4)?;
                for px in row.chunks_exact(4) {
                    raw.extend([px[2], px[1], px[0]]);
                }
            }
        }
        DXGI_FORMAT_R10G10B10A2_UNORM => {
            for y in 0..h {
                raw.push(0);
                let row = rows.get(y * pitch..y * pitch + w * 4)?;
                for px in row.chunks_exact(4) {
                    let v = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
                    let r = ((v & 0x3ff) * 255 / 1023) as u8;
                    let g = (((v >> 10) & 0x3ff) * 255 / 1023) as u8;
                    let b = (((v >> 20) & 0x3ff) * 255 / 1023) as u8;
                    raw.extend([r, g, b]);
                }
            }
        }
        DXGI_FORMAT_R16G16B16A16_FLOAT => {
            for y in 0..h {
                raw.push(0);
                let row = rows.get(y * pitch..y * pitch + w * 8)?;
                for px in row.chunks_exact(8) {
                    raw.extend([
                        f16_to_u8(u16::from_le_bytes([px[0], px[1]])),
                        f16_to_u8(u16::from_le_bytes([px[2], px[3]])),
                        f16_to_u8(u16::from_le_bytes([px[4], px[5]])),
                    ]);
                }
            }
        }
        _ => return None,
    }
    Some(encode_rgb_png(width, height, &raw))
}

fn f16_to_u8(bits: u16) -> u8 {
    let sign = (bits >> 15) & 1;
    if sign != 0 {
        return 0;
    }
    let exp = ((bits >> 10) & 0x1f) as i32;
    let frac = (bits & 0x3ff) as f32;
    let val = if exp == 0 {
        (frac / 1024.0) * 2.0f32.powi(-14)
    } else if exp == 31 {
        1.0
    } else {
        (1.0 + frac / 1024.0) * 2.0f32.powi(exp - 15)
    };
    let srgb = if val <= 0.003_130_8 {
        val * 12.92
    } else {
        1.055 * val.clamp(0.0, 1.0).powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn encode_rgb_png(w: u32, h: u32, raw_filtered: &[u8]) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend(w.to_be_bytes());
    header.extend(h.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    let mut compressed = vec![0u8; zlib_rs::compress_bound(raw_filtered.len())];
    let (idat, rc) = zlib_rs::compress_slice(&mut compressed, raw_filtered, zlib_rs::DeflateConfig::new(2));
    debug_assert_eq!(rc, zlib_rs::ReturnCode::Ok);
    chunk(&mut png, b"IDAT", idat);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend((data.len() as u32).to_be_bytes());
    let start = png.len();
    png.extend(kind);
    png.extend(data);
    let crc = crc32(&png[start..]);
    png.extend(crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_rgb_png_has_valid_signature_and_dimensions() {
        let mut bgra = Vec::new();
        for y in 0..2u32 {
            for x in 0..3u32 {
                // B, G, R, A
                bgra.extend([(x * 40) as u8, (y * 80) as u8, 200u8, 255u8]);
            }
        }
        let png = encode_surface_png(DXGI_FORMAT_B8G8R8A8_UNORM, 3, 2, 12, &bgra).expect("bgra png");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 3);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 2);

        let r10 = 1023u32 | (512u32 << 10) | (0u32 << 20);
        let png10 = encode_surface_png(DXGI_FORMAT_R10G10B10A2_UNORM, 1, 1, 4, &r10.to_le_bytes()).expect("10-bit png");
        assert_eq!(&png10[..8], b"\x89PNG\r\n\x1a\n");
    }
}
