//! Silent COM fallback for WASAPI / Spatial Audio when the host PC has zero
//! active audio render endpoints (for example, HDMI display asleep or no
//! speakers connected).
//!
//! When a host render endpoint exists, `CoCreateInstance` returns the host's
//! real `IMMDeviceEnumerator` untouched unless `VICTORY_SILENT_AUDIO=1` is set.
//! When `GetDefaultAudioEndpoint(eRender, eConsole)` fails (e.g.
//! `ERROR_NOT_FOUND`), we return a minimal virtual render device that drives the
//! title's buffer/event pump on a 10 ms cadence so audio command rings drain
//! normally instead of overflowing.

#![allow(non_snake_case)]

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetEvent};

use crate::modules::xgameruntime::{HRESULT, S_OK};
use crate::{HleModule, hle_export};

const S_FALSE: HRESULT = 1;
const E_NOINTERFACE: HRESULT = 0x8000_4002_u32 as i32;
const E_POINTER: HRESULT = 0x8000_4003_u32 as i32;
const E_FAIL: HRESULT = 0x8000_4005_u32 as i32;
const E_INVALIDARG: HRESULT = 0x8007_0057_u32 as i32;
const E_NOTFOUND: HRESULT = 0x8007_0490_u32 as i32;

const DEVICE_STATE_ACTIVE: u32 = 0x0000_0001;
const DATA_FLOW_RENDER: i32 = 0;
const DATA_FLOW_ALL: i32 = 2;
const VT_EMPTY: u16 = 0;
const VT_UI4: u16 = 19;
const VT_BLOB: u16 = 65;
const FORM_FACTOR_SPEAKERS: u32 = 1;
const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xFFFE;
const WAVE_FORMAT_IEEE_FLOAT_TAG: u16 = 0x0003;
const STATIC_MASK_STEREO: i32 = 0x06; // FrontLeft | FrontRight
const MAX_DYNAMIC_OBJECTS: u32 = 16;
const FRAMES_PER_10MS: u32 = 480; // 48 kHz * 10 ms
const PERIOD_10MS_100NS: i64 = 100_000;

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) struct Guid(u32, u16, u16, [u8; 8]);

const IID_IUNKNOWN: Guid = Guid(
    0x00000000,
    0x0000,
    0x0000,
    [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
);
const CLSID_MM_DEVICE_ENUMERATOR: Guid = Guid(
    0xBCDE0395,
    0xE52F,
    0x467C,
    [0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E],
);
const IID_IMM_DEVICE_ENUMERATOR: Guid = Guid(
    0xA95664D2,
    0x9614,
    0x4F35,
    [0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6],
);
const IID_IMM_DEVICE_COLLECTION: Guid = Guid(
    0x0BD7A1BE,
    0x7A1A,
    0x44DB,
    [0x83, 0x97, 0xCC, 0x53, 0x92, 0x38, 0x7B, 0x5E],
);
const IID_IMM_DEVICE: Guid = Guid(
    0xD666063F,
    0x1587,
    0x4E43,
    [0x81, 0xF1, 0xB9, 0x48, 0xE8, 0x07, 0x36, 0x3F],
);
const IID_IPROPERTY_STORE: Guid = Guid(
    0x886D8EEB,
    0x8CF2,
    0x4446,
    [0x8D, 0x02, 0xCD, 0xBA, 0x1D, 0xBD, 0xCF, 0x99],
);
const IID_IAUDIO_CLIENT: Guid = Guid(
    0x1CB9AD4C,
    0xDBFA,
    0x4C32,
    [0xB1, 0x78, 0xC2, 0xF5, 0x68, 0xA7, 0x03, 0xB2],
);
const IID_IAUDIO_RENDER_CLIENT: Guid = Guid(
    0xF294ACFC,
    0x3146,
    0x4483,
    [0xA7, 0xBF, 0xAD, 0xDC, 0xA7, 0xC2, 0x60, 0xE2],
);
const IID_ISPATIAL_AUDIO_CLIENT: Guid = Guid(
    0xBBF8E066,
    0xAAAA,
    0x49BE,
    [0x9A, 0x4D, 0xFD, 0x2A, 0x85, 0x8E, 0xA2, 0x7F],
);
const IID_IAUDIO_FORMAT_ENUMERATOR: Guid = Guid(
    0xDCDAA858,
    0x895A,
    0x4A22,
    [0xA5, 0xEB, 0x67, 0xBD, 0xA5, 0x06, 0x09, 0x6D],
);
const IID_ISPATIAL_AUDIO_OBJECT_RENDER_STREAM_BASE: Guid = Guid(
    0xFEAAF403,
    0xC1D8,
    0x450D,
    [0xAA, 0x05, 0xE0, 0xCC, 0xEE, 0x75, 0x02, 0xA8],
);
const IID_ISPATIAL_AUDIO_OBJECT_RENDER_STREAM: Guid = Guid(
    0xBAB5F473,
    0xB423,
    0x477B,
    [0x85, 0xF5, 0xB5, 0xA3, 0x32, 0xA0, 0x41, 0x53],
);
const IID_ISPATIAL_AUDIO_OBJECT_BASE: Guid = Guid(
    0xCCE0B8F2,
    0x8D4D,
    0x4EFB,
    [0xA8, 0xCF, 0x3D, 0x6E, 0xCF, 0x1C, 0x30, 0xE0],
);
const IID_ISPATIAL_AUDIO_OBJECT: Guid = Guid(
    0xDDE28967,
    0x521B,
    0x46E5,
    [0x8F, 0x00, 0xBD, 0x6F, 0x2B, 0xC8, 0xAB, 0x1D],
);
const PKEY_AUDIO_ENDPOINT_FORM_FACTOR_FMTID: Guid = Guid(
    0x1DA5D803,
    0xD492,
    0x4EDD,
    [0x8C, 0x23, 0xE0, 0xC0, 0xFF, 0xEE, 0x7F, 0x0E],
);
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: Guid = Guid(
    0x00000003,
    0x0000,
    0x0010,
    [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
);

#[repr(C)]
#[derive(Copy, Clone)]
struct PropertyKey {
    fmtid: Guid,
    pid: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct PropVariant {
    vt: u16,
    reserved: [u16; 3],
    val1: u64,
    val2: u64,
}

#[repr(C, packed(1))]
#[derive(Copy, Clone)]
struct WaveFormatEx {
    w_format_tag: u16,
    n_channels: u16,
    n_samples_per_sec: u32,
    n_avg_bytes_per_sec: u32,
    n_block_align: u16,
    w_bits_per_sample: u16,
    cb_size: u16,
}

#[repr(C, packed(1))]
#[derive(Copy, Clone)]
struct WaveFormatExtensible {
    format: WaveFormatEx,
    samples: u16,
    dw_channel_mask: u32,
    sub_format: Guid,
}

#[repr(C, packed(1))]
#[derive(Copy, Clone)]
struct SpatialAudioActivationParams {
    object_format: *const WaveFormatEx,
    static_object_type_mask: i32,
    min_dynamic_object_count: u32,
    max_dynamic_object_count: u32,
    category: i32,
    event_handle: HANDLE,
    notify_object: *mut c_void,
}

// Generic COM header: every object here has `vtbl` at offset 0 and `refs` right after.
#[repr(C)]
struct UnknownVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

#[repr(C)]
struct ComHeader<V: 'static> {
    vtbl: &'static V,
    refs: AtomicU32,
}

trait ComObject: Sized + 'static {
    fn header(this: *mut Self) -> *mut ComHeader<UnknownVtbl> {
        this.cast()
    }
    fn matches_iid(iid: &Guid) -> bool;
}

unsafe fn query_impl<T: ComObject>(
    this: *mut c_void,
    riid: *const Guid,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    if riid.is_null() {
        unsafe { *ppv = null_mut() };
        return E_POINTER;
    }
    let iid = unsafe { *riid };
    if iid == IID_IUNKNOWN || T::matches_iid(&iid) {
        let header = unsafe { &*T::header(this.cast()) };
        header.refs.fetch_add(1, Ordering::Relaxed);
        unsafe { *ppv = this };
        return S_OK;
    }
    unsafe { *ppv = null_mut() };
    E_NOINTERFACE
}

unsafe fn add_ref_impl<T: ComObject>(this: *mut c_void) -> u32 {
    let header = unsafe { &*T::header(this.cast()) };
    header.refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe fn release_impl<T: ComObject>(this: *mut c_void) -> u32 {
    let header = unsafe { &*T::header(this.cast()) };
    let prev = header.refs.fetch_sub(1, Ordering::Release);
    if prev == 1 {
        std::sync::atomic::fence(Ordering::Acquire);
        // SAFETY: allocated with `Box::into_raw` when created and refcount hit 0.
        unsafe { drop(Box::from_raw(this.cast::<T>())) };
        return 0;
    }
    prev - 1
}

macro_rules! iunknown_methods {
    ($ty:ty, $fn_qi:ident, $fn_add:ident, $fn_rel:ident) => {
        unsafe extern "system" fn $fn_qi(
            this: *mut c_void,
            riid: *const Guid,
            ppv: *mut *mut c_void,
        ) -> HRESULT {
            unsafe { query_impl::<$ty>(this, riid, ppv) }
        }
        unsafe extern "system" fn $fn_add(this: *mut c_void) -> u32 {
            unsafe { add_ref_impl::<$ty>(this) }
        }
        unsafe extern "system" fn $fn_rel(this: *mut c_void) -> u32 {
            unsafe { release_impl::<$ty>(this) }
        }
    };
}

fn host_co_create_instance() -> Option<
    unsafe extern "system" fn(*const Guid, *mut c_void, u32, *const Guid, *mut *mut c_void) -> HRESULT,
> {
    let addr = crate::host::function("api-ms-win-core-com-l1-1-0.dll", "CoCreateInstance")
        .or_else(|| crate::host::function("ole32.dll", "CoCreateInstance"))?;
    // SAFETY: signature matches COM `CoCreateInstance`.
    Some(unsafe { std::mem::transmute(addr) })
}

fn host_co_task_mem_alloc(bytes: usize) -> *mut c_void {
    let Some(addr) = crate::host::function("api-ms-win-core-com-l1-1-0.dll", "CoTaskMemAlloc")
        .or_else(|| crate::host::function("ole32.dll", "CoTaskMemAlloc"))
    else {
        return null_mut();
    };
    // SAFETY: signature matches COM `CoTaskMemAlloc`.
    let alloc: unsafe extern "system" fn(usize) -> *mut c_void = unsafe { std::mem::transmute(addr) };
    unsafe { alloc(bytes) }
}

unsafe fn com_release_raw(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: valid COM object with `IUnknown` layout at offset 0.
    unsafe {
        let vtbl = *(ptr as *const *const UnknownVtbl);
        ((*vtbl).release)(ptr);
    }
}

fn force_silent_audio() -> bool {
    static FORCE: OnceLock<bool> = OnceLock::new();
    *FORCE.get_or_init(|| {
        std::env::var_os("VICTORY_SILENT_AUDIO").is_some_and(|v| v == "1" || v == "true")
    })
}

fn log_fallback_once() {
    static LOGGED: OnceLock<()> = OnceLock::new();
    LOGGED.get_or_init(|| {
        tracing::info!("no active host audio render endpoint; using silent WASAPI/SpatialAudio sink");
    });
}

/// Drops in for `CoCreateInstance`: passes through to Windows, and only steps
/// in with a silent `IMMDeviceEnumerator` when creating `MMDeviceEnumerator` and
/// the host has no active render endpoint (e.g. sleeping display / headless).
pub extern "system" fn CoCreateInstance(
    rclsid: *const Guid,
    p_unk_outer: *mut c_void,
    dw_cls_context: u32,
    riid: *const Guid,
    ppv: *mut *mut c_void,
) -> HRESULT {
    let Some(real) = host_co_create_instance() else {
        return E_FAIL;
    };
    // SAFETY: forwards arguments directly to host `CoCreateInstance`.
    let hr = unsafe { real(rclsid, p_unk_outer, dw_cls_context, riid, ppv) };
    if hr != S_OK || rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return hr;
    }
    // SAFETY: validated non-null pointers.
    let (clsid, iid, host_obj) = unsafe { (*rclsid, *riid, *ppv) };
    if clsid != CLSID_MM_DEVICE_ENUMERATOR
        || iid != IID_IMM_DEVICE_ENUMERATOR
        || host_obj.is_null()
    {
        return hr;
    }
    if !force_silent_audio() {
        // Probe slot 4 of `IMMDeviceEnumerator`: `GetDefaultAudioEndpoint(eRender, eConsole, &dev)`.
        let mut dev: *mut c_void = null_mut();
        // SAFETY: `host_obj` is a live `IMMDeviceEnumerator` returned by the host.
        let hr_dev = unsafe {
            let vtbl = *(host_obj as *const *const IMMDeviceEnumeratorVtbl);
            ((*vtbl).get_default_audio_endpoint)(host_obj, DATA_FLOW_RENDER, 0, &mut dev)
        };
        if hr_dev == S_OK && !dev.is_null() {
            // SAFETY: release probe reference; return the real host enumerator.
            unsafe { com_release_raw(dev) };
            return S_OK;
        }
    }
    // Host has no active render endpoint (or silent fallback forced): replace with silent sink.
    // SAFETY: `host_obj` is no longer needed; `*ppv` is valid for write.
    unsafe {
        com_release_raw(host_obj);
        *ppv = Box::into_raw(Box::new(NullDeviceEnumerator::new())).cast();
    }
    log_fallback_once();
    S_OK
}

// ---------------------------------------------------------------------------
// Background 10 ms EventHandle pump (shared by AudioClient & SpatialStream)
// ---------------------------------------------------------------------------

struct EventPump {
    running: Arc<AtomicBool>,
    handle_raw: usize,
    thread: Option<JoinHandle<()>>,
}

impl EventPump {
    fn new(title_event: HANDLE) -> Self {
        let mut dup: HANDLE = null_mut();
        if !title_event.is_null() {
            // SAFETY: duplicates event handle in current process so title CloseHandle cannot race.
            let ok = unsafe {
                let proc_handle = GetCurrentProcess();
                DuplicateHandle(
                    proc_handle,
                    title_event,
                    proc_handle,
                    &mut dup,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            };
            if ok == 0 {
                dup = null_mut();
            }
        }
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle_raw: dup as usize,
            thread: None,
        }
    }

    fn start(&mut self) {
        if self.thread.is_some() || self.handle_raw == 0 {
            return;
        }
        self.running.store(true, Ordering::Release);
        let running = Arc::clone(&self.running);
        let raw = self.handle_raw;
        self.thread = std::thread::Builder::new()
            .name(String::from("victory-null-audio"))
            .spawn(move || {
                let ev = raw as HANDLE;
                while running.load(Ordering::Acquire) {
                    // SAFETY: `ev` is an owned duplicated handle alive until thread joins.
                    unsafe { SetEvent(ev) };
                    std::thread::sleep(Duration::from_millis(10));
                }
            })
            .ok();
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for EventPump {
    fn drop(&mut self) {
        self.stop();
        if self.handle_raw != 0 {
            // SAFETY: thread has joined; `handle_raw` is owned by `self`.
            unsafe { CloseHandle(self.handle_raw as HANDLE) };
            self.handle_raw = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// IMMDeviceEnumerator
// ---------------------------------------------------------------------------

#[repr(C)]
struct IMMDeviceEnumeratorVtbl {
    unknown: UnknownVtbl,
    enum_audio_endpoints:
        unsafe extern "system" fn(*mut c_void, i32, u32, *mut *mut c_void) -> HRESULT,
    get_default_audio_endpoint:
        unsafe extern "system" fn(*mut c_void, i32, i32, *mut *mut c_void) -> HRESULT,
    get_device: unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut c_void) -> HRESULT,
    register_cb: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    unregister_cb: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
}

#[repr(C)]
struct NullDeviceEnumerator {
    vtbl: &'static IMMDeviceEnumeratorVtbl,
    refs: AtomicU32,
}

impl NullDeviceEnumerator {
    fn new() -> Self {
        Self {
            vtbl: &ENUMERATOR_VTBL,
            refs: AtomicU32::new(1),
        }
    }
}

impl ComObject for NullDeviceEnumerator {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_IMM_DEVICE_ENUMERATOR
    }
}

iunknown_methods!(NullDeviceEnumerator, enum_qi, enum_add_ref, enum_release);

unsafe extern "system" fn enum_audio_endpoints(
    _this: *mut c_void,
    data_flow: i32,
    state_mask: u32,
    pp_devices: *mut *mut c_void,
) -> HRESULT {
    if pp_devices.is_null() {
        return E_POINTER;
    }
    let has_render = (data_flow == DATA_FLOW_RENDER || data_flow == DATA_FLOW_ALL)
        && (state_mask & DEVICE_STATE_ACTIVE) != 0;
    let count = u32::from(has_render);
    // SAFETY: `pp_devices` is non-null.
    unsafe { *pp_devices = Box::into_raw(Box::new(NullDeviceCollection::new(count))).cast() };
    S_OK
}

unsafe extern "system" fn enum_get_default_audio_endpoint(
    _this: *mut c_void,
    data_flow: i32,
    _role: i32,
    pp_endpoint: *mut *mut c_void,
) -> HRESULT {
    if pp_endpoint.is_null() {
        return E_POINTER;
    }
    if data_flow != DATA_FLOW_RENDER && data_flow != DATA_FLOW_ALL {
        unsafe { *pp_endpoint = null_mut() };
        return E_NOTFOUND;
    }
    // SAFETY: `pp_endpoint` is non-null.
    unsafe { *pp_endpoint = Box::into_raw(Box::new(NullDevice::new())).cast() };
    S_OK
}

unsafe extern "system" fn enum_get_device(
    _this: *mut c_void,
    pwstr_id: *const u16,
    pp_device: *mut *mut c_void,
) -> HRESULT {
    if pp_device.is_null() {
        return E_POINTER;
    }
    if pwstr_id.is_null() {
        unsafe { *pp_device = null_mut() };
        return E_INVALIDARG;
    }
    // SAFETY: `pp_device` is non-null.
    unsafe { *pp_device = Box::into_raw(Box::new(NullDevice::new())).cast() };
    S_OK
}

unsafe extern "system" fn enum_noop_cb(_this: *mut c_void, _client: *mut c_void) -> HRESULT {
    S_OK
}

static ENUMERATOR_VTBL: IMMDeviceEnumeratorVtbl = IMMDeviceEnumeratorVtbl {
    unknown: UnknownVtbl {
        query_interface: enum_qi,
        add_ref: enum_add_ref,
        release: enum_release,
    },
    enum_audio_endpoints,
    get_default_audio_endpoint: enum_get_default_audio_endpoint,
    get_device: enum_get_device,
    register_cb: enum_noop_cb,
    unregister_cb: enum_noop_cb,
};

// ---------------------------------------------------------------------------
// IMMDeviceCollection
// ---------------------------------------------------------------------------

#[repr(C)]
struct IMMDeviceCollectionVtbl {
    unknown: UnknownVtbl,
    get_count: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    item: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct NullDeviceCollection {
    vtbl: &'static IMMDeviceCollectionVtbl,
    refs: AtomicU32,
    count: u32,
}

impl NullDeviceCollection {
    fn new(count: u32) -> Self {
        Self {
            vtbl: &COLLECTION_VTBL,
            refs: AtomicU32::new(1),
            count,
        }
    }
}

impl ComObject for NullDeviceCollection {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_IMM_DEVICE_COLLECTION
    }
}

iunknown_methods!(NullDeviceCollection, col_qi, col_add_ref, col_release);

unsafe extern "system" fn col_get_count(this: *mut c_void, pc_devices: *mut u32) -> HRESULT {
    if pc_devices.is_null() {
        return E_POINTER;
    }
    let me = unsafe { &*(this.cast::<NullDeviceCollection>()) };
    unsafe { *pc_devices = me.count };
    S_OK
}

unsafe extern "system" fn col_item(
    this: *mut c_void,
    n_device: u32,
    pp_device: *mut *mut c_void,
) -> HRESULT {
    if pp_device.is_null() {
        return E_POINTER;
    }
    let me = unsafe { &*(this.cast::<NullDeviceCollection>()) };
    if n_device >= me.count {
        unsafe { *pp_device = null_mut() };
        return E_INVALIDARG;
    }
    unsafe { *pp_device = Box::into_raw(Box::new(NullDevice::new())).cast() };
    S_OK
}

static COLLECTION_VTBL: IMMDeviceCollectionVtbl = IMMDeviceCollectionVtbl {
    unknown: UnknownVtbl {
        query_interface: col_qi,
        add_ref: col_add_ref,
        release: col_release,
    },
    get_count: col_get_count,
    item: col_item,
};

// ---------------------------------------------------------------------------
// IMMDevice
// ---------------------------------------------------------------------------

#[repr(C)]
struct IMMDeviceVtbl {
    unknown: UnknownVtbl,
    activate: unsafe extern "system" fn(
        *mut c_void,
        *const Guid,
        u32,
        *const PropVariant,
        *mut *mut c_void,
    ) -> HRESULT,
    open_property_store: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT,
    get_id: unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> HRESULT,
    get_state: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
}

#[repr(C)]
struct NullDevice {
    vtbl: &'static IMMDeviceVtbl,
    refs: AtomicU32,
}

impl NullDevice {
    fn new() -> Self {
        Self {
            vtbl: &DEVICE_VTBL,
            refs: AtomicU32::new(1),
        }
    }
}

impl ComObject for NullDevice {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_IMM_DEVICE
    }
}

iunknown_methods!(NullDevice, dev_qi, dev_add_ref, dev_release);

unsafe extern "system" fn dev_activate(
    _this: *mut c_void,
    iid: *const Guid,
    _cls_ctx: u32,
    _activation_params: *const PropVariant,
    pp_iface: *mut *mut c_void,
) -> HRESULT {
    if pp_iface.is_null() {
        return E_POINTER;
    }
    if iid.is_null() {
        unsafe { *pp_iface = null_mut() };
        return E_POINTER;
    }
    let wanted = unsafe { *iid };
    if wanted == IID_IAUDIO_CLIENT {
        unsafe { *pp_iface = Box::into_raw(Box::new(NullAudioClient::new())).cast() };
        return S_OK;
    }
    if wanted == IID_ISPATIAL_AUDIO_CLIENT {
        unsafe { *pp_iface = Box::into_raw(Box::new(NullSpatialAudioClient::new())).cast() };
        return S_OK;
    }
    unsafe { *pp_iface = null_mut() };
    E_NOINTERFACE
}

unsafe extern "system" fn dev_open_property_store(
    _this: *mut c_void,
    _stgm_access: u32,
    pp_props: *mut *mut c_void,
) -> HRESULT {
    if pp_props.is_null() {
        return E_POINTER;
    }
    unsafe { *pp_props = Box::into_raw(Box::new(NullPropertyStore::new())).cast() };
    S_OK
}

unsafe extern "system" fn dev_get_id(_this: *mut c_void, ppstr_id: *mut *mut u16) -> HRESULT {
    if ppstr_id.is_null() {
        return E_POINTER;
    }
    const ID_UTF16: &[u16] = &[
        b'v' as u16, b'i' as u16, b'c' as u16, b't' as u16, b'o' as u16, b'r' as u16, b'y' as u16,
        b'-' as u16, b'n' as u16, b'u' as u16, b'l' as u16, b'l' as u16, 0,
    ];
    let bytes = std::mem::size_of_val(ID_UTF16);
    let mem = host_co_task_mem_alloc(bytes).cast::<u16>();
    if mem.is_null() {
        return E_FAIL;
    }
    // SAFETY: `mem` was allocated by `CoTaskMemAlloc` for `bytes` bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(ID_UTF16.as_ptr(), mem, ID_UTF16.len());
        *ppstr_id = mem;
    }
    S_OK
}

unsafe extern "system" fn dev_get_state(_this: *mut c_void, pdw_state: *mut u32) -> HRESULT {
    if pdw_state.is_null() {
        return E_POINTER;
    }
    unsafe { *pdw_state = DEVICE_STATE_ACTIVE };
    S_OK
}

static DEVICE_VTBL: IMMDeviceVtbl = IMMDeviceVtbl {
    unknown: UnknownVtbl {
        query_interface: dev_qi,
        add_ref: dev_add_ref,
        release: dev_release,
    },
    activate: dev_activate,
    open_property_store: dev_open_property_store,
    get_id: dev_get_id,
    get_state: dev_get_state,
};

// ---------------------------------------------------------------------------
// IPropertyStore
// ---------------------------------------------------------------------------

#[repr(C)]
struct IPropertyStoreVtbl {
    unknown: UnknownVtbl,
    get_count: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    get_at: unsafe extern "system" fn(*mut c_void, u32, *mut PropertyKey) -> HRESULT,
    get_value: unsafe extern "system" fn(*mut c_void, *const PropertyKey, *mut PropVariant) -> HRESULT,
    set_value:
        unsafe extern "system" fn(*mut c_void, *const PropertyKey, *const PropVariant) -> HRESULT,
    commit: unsafe extern "system" fn(*mut c_void) -> HRESULT,
}

#[repr(C)]
struct NullPropertyStore {
    vtbl: &'static IPropertyStoreVtbl,
    refs: AtomicU32,
}

impl NullPropertyStore {
    fn new() -> Self {
        Self {
            vtbl: &PROP_STORE_VTBL,
            refs: AtomicU32::new(1),
        }
    }
}

impl ComObject for NullPropertyStore {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_IPROPERTY_STORE
    }
}

iunknown_methods!(NullPropertyStore, ps_qi, ps_add_ref, ps_release);

unsafe extern "system" fn ps_get_count(_this: *mut c_void, c_props: *mut u32) -> HRESULT {
    if c_props.is_null() {
        return E_POINTER;
    }
    unsafe { *c_props = 1 };
    S_OK
}

unsafe extern "system" fn ps_get_at(
    _this: *mut c_void,
    i_prop: u32,
    pkey: *mut PropertyKey,
) -> HRESULT {
    if pkey.is_null() {
        return E_POINTER;
    }
    if i_prop != 0 {
        return E_INVALIDARG;
    }
    unsafe {
        *pkey = PropertyKey {
            fmtid: PKEY_AUDIO_ENDPOINT_FORM_FACTOR_FMTID,
            pid: 0,
        };
    }
    S_OK
}

unsafe extern "system" fn ps_get_value(
    _this: *mut c_void,
    key: *const PropertyKey,
    pv: *mut PropVariant,
) -> HRESULT {
    if pv.is_null() || key.is_null() {
        return E_POINTER;
    }
    let k = unsafe { *key };
    let val = if k.fmtid == PKEY_AUDIO_ENDPOINT_FORM_FACTOR_FMTID && k.pid == 0 {
        PropVariant {
            vt: VT_UI4,
            reserved: [0; 3],
            val1: u64::from(FORM_FACTOR_SPEAKERS),
            val2: 0,
        }
    } else {
        PropVariant {
            vt: VT_EMPTY,
            reserved: [0; 3],
            val1: 0,
            val2: 0,
        }
    };
    unsafe { *pv = val };
    S_OK
}

unsafe extern "system" fn ps_set_value(
    _this: *mut c_void,
    _key: *const PropertyKey,
    _pv: *const PropVariant,
) -> HRESULT {
    S_OK
}

unsafe extern "system" fn ps_commit(_this: *mut c_void) -> HRESULT {
    S_OK
}

static PROP_STORE_VTBL: IPropertyStoreVtbl = IPropertyStoreVtbl {
    unknown: UnknownVtbl {
        query_interface: ps_qi,
        add_ref: ps_add_ref,
        release: ps_release,
    },
    get_count: ps_get_count,
    get_at: ps_get_at,
    get_value: ps_get_value,
    set_value: ps_set_value,
    commit: ps_commit,
};

// ---------------------------------------------------------------------------
// IAudioClient & IAudioRenderClient
// ---------------------------------------------------------------------------

fn stereo_float_extensible() -> WaveFormatExtensible {
    WaveFormatExtensible {
        format: WaveFormatEx {
            w_format_tag: WAVE_FORMAT_EXTENSIBLE_TAG,
            n_channels: 2,
            n_samples_per_sec: 48_000,
            n_avg_bytes_per_sec: 48_000 * 8,
            n_block_align: 8,
            w_bits_per_sample: 32,
            cb_size: 22,
        },
        samples: 32,
        dw_channel_mask: 0x3, // SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT
        sub_format: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
    }
}

fn mono_float_waveformat() -> WaveFormatEx {
    WaveFormatEx {
        w_format_tag: WAVE_FORMAT_IEEE_FLOAT_TAG,
        n_channels: 1,
        n_samples_per_sec: 48_000,
        n_avg_bytes_per_sec: 48_000 * 4,
        n_block_align: 4,
        w_bits_per_sample: 32,
        cb_size: 0,
    }
}

#[repr(C)]
struct IAudioClientVtbl {
    unknown: UnknownVtbl,
    initialize: unsafe extern "system" fn(
        *mut c_void,
        i32,
        u32,
        i64,
        i64,
        *const WaveFormatEx,
        *const Guid,
    ) -> HRESULT,
    get_buffer_size: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    get_stream_latency: unsafe extern "system" fn(*mut c_void, *mut i64) -> HRESULT,
    get_current_padding: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    is_format_supported: unsafe extern "system" fn(
        *mut c_void,
        i32,
        *const WaveFormatEx,
        *mut *mut WaveFormatEx,
    ) -> HRESULT,
    get_mix_format: unsafe extern "system" fn(*mut c_void, *mut *mut WaveFormatEx) -> HRESULT,
    get_device_period: unsafe extern "system" fn(*mut c_void, *mut i64, *mut i64) -> HRESULT,
    start: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    stop: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    reset: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    set_event_handle: unsafe extern "system" fn(*mut c_void, HANDLE) -> HRESULT,
    get_service: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> HRESULT,
}

struct AudioClientState {
    buffer_frames: u32,
    bytes_per_frame: usize,
    pump: Option<EventPump>,
    buffer: Vec<u8>,
}

#[repr(C)]
struct NullAudioClient {
    vtbl: &'static IAudioClientVtbl,
    refs: AtomicU32,
    state: Mutex<AudioClientState>,
}

impl NullAudioClient {
    fn new() -> Self {
        Self {
            vtbl: &AUDIO_CLIENT_VTBL,
            refs: AtomicU32::new(1),
            state: Mutex::new(AudioClientState {
                buffer_frames: FRAMES_PER_10MS * 2,
                bytes_per_frame: 8,
                pump: None,
                buffer: Vec::new(),
            }),
        }
    }
}

impl ComObject for NullAudioClient {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_IAUDIO_CLIENT
    }
}

iunknown_methods!(NullAudioClient, ac_qi, ac_add_ref, ac_release);

unsafe extern "system" fn ac_initialize(
    this: *mut c_void,
    _share_mode: i32,
    _stream_flags: u32,
    hns_buffer_duration: i64,
    _hns_periodicity: i64,
    p_format: *const WaveFormatEx,
    _session_guid: *const Guid,
) -> HRESULT {
    let me = unsafe { &*(this.cast::<NullAudioClient>()) };
    let mut st = me.state.lock().unwrap_or_else(|e| e.into_inner());
    let (rate, align) = if p_format.is_null() {
        (48_000u64, 8usize)
    } else {
        let wf = unsafe { p_format.read_unaligned() };
        (
            u64::from(wf.n_samples_per_sec.max(8_000)),
            usize::from(wf.n_block_align.max(4)),
        )
    };
    let frames = if hns_buffer_duration > 0 {
        ((rate * hns_buffer_duration as u64) / 10_000_000).clamp(FRAMES_PER_10MS as u64, 48_000)
            as u32
    } else {
        FRAMES_PER_10MS * 2
    };
    st.buffer_frames = frames;
    st.bytes_per_frame = align;
    st.buffer.resize((frames as usize) * align, 0);
    S_OK
}

unsafe extern "system" fn ac_get_buffer_size(
    this: *mut c_void,
    p_num_frames: *mut u32,
) -> HRESULT {
    if p_num_frames.is_null() {
        return E_POINTER;
    }
    let me = unsafe { &*(this.cast::<NullAudioClient>()) };
    let st = me.state.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { *p_num_frames = st.buffer_frames };
    S_OK
}

unsafe extern "system" fn ac_get_stream_latency(
    _this: *mut c_void,
    phns_latency: *mut i64,
) -> HRESULT {
    if phns_latency.is_null() {
        return E_POINTER;
    }
    unsafe { *phns_latency = PERIOD_10MS_100NS };
    S_OK
}

unsafe extern "system" fn ac_get_current_padding(
    _this: *mut c_void,
    p_padding: *mut u32,
) -> HRESULT {
    if p_padding.is_null() {
        return E_POINTER;
    }
    unsafe { *p_padding = 0 };
    S_OK
}

unsafe extern "system" fn ac_is_format_supported(
    _this: *mut c_void,
    _share_mode: i32,
    p_format: *const WaveFormatEx,
    pp_closest_match: *mut *mut WaveFormatEx,
) -> HRESULT {
    if !pp_closest_match.is_null() {
        unsafe { *pp_closest_match = null_mut() };
    }
    if p_format.is_null() {
        return E_POINTER;
    }
    S_OK
}

unsafe extern "system" fn ac_get_mix_format(
    _this: *mut c_void,
    pp_format: *mut *mut WaveFormatEx,
) -> HRESULT {
    if pp_format.is_null() {
        return E_POINTER;
    }
    let mem = host_co_task_mem_alloc(std::mem::size_of::<WaveFormatExtensible>())
        .cast::<WaveFormatExtensible>();
    if mem.is_null() {
        return E_FAIL;
    }
    // SAFETY: `mem` allocated with `CoTaskMemAlloc`.
    unsafe {
        mem.write_unaligned(stereo_float_extensible());
        *pp_format = mem.cast();
    }
    S_OK
}

unsafe extern "system" fn ac_get_device_period(
    _this: *mut c_void,
    phns_default: *mut i64,
    phns_min: *mut i64,
) -> HRESULT {
    if !phns_default.is_null() {
        unsafe { *phns_default = PERIOD_10MS_100NS };
    }
    if !phns_min.is_null() {
        unsafe { *phns_min = PERIOD_10MS_100NS };
    }
    S_OK
}

unsafe extern "system" fn ac_start(this: *mut c_void) -> HRESULT {
    let me = unsafe { &*(this.cast::<NullAudioClient>()) };
    let mut st = me.state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(pump) = st.pump.as_mut() {
        pump.start();
    }
    S_OK
}

unsafe extern "system" fn ac_stop(this: *mut c_void) -> HRESULT {
    let me = unsafe { &*(this.cast::<NullAudioClient>()) };
    let mut st = me.state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(pump) = st.pump.as_mut() {
        pump.stop();
    }
    S_OK
}

unsafe extern "system" fn ac_reset(_this: *mut c_void) -> HRESULT {
    S_OK
}

unsafe extern "system" fn ac_set_event_handle(this: *mut c_void, event_handle: HANDLE) -> HRESULT {
    let me = unsafe { &*(this.cast::<NullAudioClient>()) };
    let mut st = me.state.lock().unwrap_or_else(|e| e.into_inner());
    st.pump = Some(EventPump::new(event_handle));
    S_OK
}

unsafe extern "system" fn ac_get_service(
    this: *mut c_void,
    riid: *const Guid,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    if riid.is_null() {
        unsafe { *ppv = null_mut() };
        return E_POINTER;
    }
    if unsafe { *riid } != IID_IAUDIO_RENDER_CLIENT {
        unsafe { *ppv = null_mut() };
        return E_NOINTERFACE;
    }
    // Keep `NullAudioClient` alive for the lifetime of `NullAudioRenderClient`.
    unsafe { ac_add_ref(this) };
    unsafe {
        *ppv = Box::into_raw(Box::new(NullAudioRenderClient::new(this.cast()))).cast();
    }
    S_OK
}

static AUDIO_CLIENT_VTBL: IAudioClientVtbl = IAudioClientVtbl {
    unknown: UnknownVtbl {
        query_interface: ac_qi,
        add_ref: ac_add_ref,
        release: ac_release,
    },
    initialize: ac_initialize,
    get_buffer_size: ac_get_buffer_size,
    get_stream_latency: ac_get_stream_latency,
    get_current_padding: ac_get_current_padding,
    is_format_supported: ac_is_format_supported,
    get_mix_format: ac_get_mix_format,
    get_device_period: ac_get_device_period,
    start: ac_start,
    stop: ac_stop,
    reset: ac_reset,
    set_event_handle: ac_set_event_handle,
    get_service: ac_get_service,
};

#[repr(C)]
struct IAudioRenderClientVtbl {
    unknown: UnknownVtbl,
    get_buffer: unsafe extern "system" fn(*mut c_void, u32, *mut *mut u8) -> HRESULT,
    release_buffer: unsafe extern "system" fn(*mut c_void, u32, u32) -> HRESULT,
}

#[repr(C)]
struct NullAudioRenderClient {
    vtbl: &'static IAudioRenderClientVtbl,
    refs: AtomicU32,
    client: *mut NullAudioClient,
}

impl NullAudioRenderClient {
    fn new(client: *mut NullAudioClient) -> Self {
        Self {
            vtbl: &RENDER_CLIENT_VTBL,
            refs: AtomicU32::new(1),
            client,
        }
    }
}

impl Drop for NullAudioRenderClient {
    fn drop(&mut self) {
        if !self.client.is_null() {
            // SAFETY: balanced with `ac_add_ref` in `ac_get_service`.
            unsafe { ac_release(self.client.cast()) };
        }
    }
}

impl ComObject for NullAudioRenderClient {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_IAUDIO_RENDER_CLIENT
    }
}

iunknown_methods!(NullAudioRenderClient, rc_qi, rc_add_ref, rc_release);

unsafe extern "system" fn rc_get_buffer(
    this: *mut c_void,
    num_frames: u32,
    pp_data: *mut *mut u8,
) -> HRESULT {
    if pp_data.is_null() {
        return E_POINTER;
    }
    let me = unsafe { &*(this.cast::<NullAudioRenderClient>()) };
    let client = unsafe { &*me.client };
    let mut st = client.state.lock().unwrap_or_else(|e| e.into_inner());
    let needed = (num_frames as usize).saturating_mul(st.bytes_per_frame);
    if st.buffer.len() < needed {
        st.buffer.resize(needed, 0);
    }
    unsafe { *pp_data = st.buffer.as_mut_ptr() };
    S_OK
}

unsafe extern "system" fn rc_release_buffer(
    _this: *mut c_void,
    _num_frames_written: u32,
    _dw_flags: u32,
) -> HRESULT {
    S_OK
}

static RENDER_CLIENT_VTBL: IAudioRenderClientVtbl = IAudioRenderClientVtbl {
    unknown: UnknownVtbl {
        query_interface: rc_qi,
        add_ref: rc_add_ref,
        release: rc_release,
    },
    get_buffer: rc_get_buffer,
    release_buffer: rc_release_buffer,
};

// ---------------------------------------------------------------------------
// ISpatialAudioClient & IAudioFormatEnumerator
// ---------------------------------------------------------------------------

#[repr(C)]
struct IAudioFormatEnumeratorVtbl {
    unknown: UnknownVtbl,
    get_count: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    get_format: unsafe extern "system" fn(*mut c_void, u32, *mut *mut WaveFormatEx) -> HRESULT,
}

#[repr(C)]
struct NullAudioFormatEnumerator {
    vtbl: &'static IAudioFormatEnumeratorVtbl,
    refs: AtomicU32,
    format: WaveFormatEx,
}

impl NullAudioFormatEnumerator {
    fn new() -> Self {
        Self {
            vtbl: &FORMAT_ENUM_VTBL,
            refs: AtomicU32::new(1),
            format: mono_float_waveformat(),
        }
    }
}

impl ComObject for NullAudioFormatEnumerator {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_IAUDIO_FORMAT_ENUMERATOR
    }
}

iunknown_methods!(NullAudioFormatEnumerator, fe_qi, fe_add_ref, fe_release);

unsafe extern "system" fn fe_get_count(_this: *mut c_void, count: *mut u32) -> HRESULT {
    if count.is_null() {
        return E_POINTER;
    }
    unsafe { *count = 1 };
    S_OK
}

unsafe extern "system" fn fe_get_format(
    this: *mut c_void,
    index: u32,
    format: *mut *mut WaveFormatEx,
) -> HRESULT {
    if format.is_null() {
        return E_POINTER;
    }
    if index != 0 {
        unsafe { *format = null_mut() };
        return E_INVALIDARG;
    }
    let me = unsafe { &mut *(this.cast::<NullAudioFormatEnumerator>()) };
    unsafe { *format = &raw mut me.format };
    S_OK
}

static FORMAT_ENUM_VTBL: IAudioFormatEnumeratorVtbl = IAudioFormatEnumeratorVtbl {
    unknown: UnknownVtbl {
        query_interface: fe_qi,
        add_ref: fe_add_ref,
        release: fe_release,
    },
    get_count: fe_get_count,
    get_format: fe_get_format,
};

#[repr(C)]
struct ISpatialAudioClientVtbl {
    unknown: UnknownVtbl,
    get_static_object_position:
        unsafe extern "system" fn(*mut c_void, i32, *mut f32, *mut f32, *mut f32) -> HRESULT,
    get_native_static_object_type_mask: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    get_max_dynamic_object_count: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    get_supported_audio_object_format_enumerator:
        unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    get_max_frame_count:
        unsafe extern "system" fn(*mut c_void, *const WaveFormatEx, *mut u32) -> HRESULT,
    is_audio_object_format_supported:
        unsafe extern "system" fn(*mut c_void, *const WaveFormatEx) -> HRESULT,
    is_spatial_audio_stream_available:
        unsafe extern "system" fn(*mut c_void, *const Guid, *const PropVariant) -> HRESULT,
    activate_spatial_audio_stream: unsafe extern "system" fn(
        *mut c_void,
        *const PropVariant,
        *const Guid,
        *mut *mut c_void,
    ) -> HRESULT,
}

#[repr(C)]
struct NullSpatialAudioClient {
    vtbl: &'static ISpatialAudioClientVtbl,
    refs: AtomicU32,
}

impl NullSpatialAudioClient {
    fn new() -> Self {
        Self {
            vtbl: &SPATIAL_CLIENT_VTBL,
            refs: AtomicU32::new(1),
        }
    }
}

impl ComObject for NullSpatialAudioClient {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_ISPATIAL_AUDIO_CLIENT
    }
}

iunknown_methods!(NullSpatialAudioClient, sac_qi, sac_add_ref, sac_release);

unsafe extern "system" fn sac_get_static_object_position(
    _this: *mut c_void,
    obj_type: i32,
    x: *mut f32,
    y: *mut f32,
    z: *mut f32,
) -> HRESULT {
    if x.is_null() || y.is_null() || z.is_null() {
        return E_POINTER;
    }
    let (px, py, pz) = match obj_type {
        0x02 => (-1.0, 0.0, 1.0), // FrontLeft
        0x04 => (1.0, 0.0, 1.0),  // FrontRight
        0x08 => (0.0, 0.0, 1.0),  // FrontCenter
        0x80 => (-1.0, 0.0, -1.0),
        0x100 => (1.0, 0.0, -1.0),
        _ => (0.0, 0.0, 1.0),
    };
    unsafe {
        *x = px;
        *y = py;
        *z = pz;
    }
    S_OK
}

unsafe extern "system" fn sac_get_native_mask(_this: *mut c_void, mask: *mut i32) -> HRESULT {
    if mask.is_null() {
        return E_POINTER;
    }
    unsafe { *mask = STATIC_MASK_STEREO };
    S_OK
}

unsafe extern "system" fn sac_get_max_dynamic(_this: *mut c_void, value: *mut u32) -> HRESULT {
    if value.is_null() {
        return E_POINTER;
    }
    unsafe { *value = MAX_DYNAMIC_OBJECTS };
    S_OK
}

unsafe extern "system" fn sac_get_supported_formats(
    _this: *mut c_void,
    enumerator: *mut *mut c_void,
) -> HRESULT {
    if enumerator.is_null() {
        return E_POINTER;
    }
    unsafe {
        *enumerator = Box::into_raw(Box::new(NullAudioFormatEnumerator::new())).cast();
    }
    S_OK
}

unsafe extern "system" fn sac_get_max_frame_count(
    _this: *mut c_void,
    object_format: *const WaveFormatEx,
    frame_count: *mut u32,
) -> HRESULT {
    if frame_count.is_null() || object_format.is_null() {
        return E_POINTER;
    }
    let wf = unsafe { object_format.read_unaligned() };
    let frames = ((u64::from(wf.n_samples_per_sec.max(8_000)) * 10) / 1000).max(64) as u32;
    unsafe { *frame_count = frames };
    S_OK
}

unsafe extern "system" fn sac_is_format_supported(
    _this: *mut c_void,
    object_format: *const WaveFormatEx,
) -> HRESULT {
    if object_format.is_null() {
        return E_POINTER;
    }
    let wf = unsafe { object_format.read_unaligned() };
    if wf.w_format_tag == WAVE_FORMAT_IEEE_FLOAT_TAG || wf.w_bits_per_sample == 32 {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "system" fn sac_is_stream_available(
    _this: *mut c_void,
    stream_uuid: *const Guid,
    _aux: *const PropVariant,
) -> HRESULT {
    if stream_uuid.is_null() {
        return E_POINTER;
    }
    let iid = unsafe { *stream_uuid };
    if iid == IID_ISPATIAL_AUDIO_OBJECT_RENDER_STREAM
        || iid == IID_ISPATIAL_AUDIO_OBJECT_RENDER_STREAM_BASE
    {
        S_OK
    } else {
        E_NOINTERFACE
    }
}

unsafe extern "system" fn sac_activate_stream(
    _this: *mut c_void,
    activation_params: *const PropVariant,
    riid: *const Guid,
    stream: *mut *mut c_void,
) -> HRESULT {
    if stream.is_null() {
        return E_POINTER;
    }
    if activation_params.is_null() || riid.is_null() {
        unsafe { *stream = null_mut() };
        return E_POINTER;
    }
    let wanted = unsafe { *riid };
    if wanted != IID_ISPATIAL_AUDIO_OBJECT_RENDER_STREAM
        && wanted != IID_ISPATIAL_AUDIO_OBJECT_RENDER_STREAM_BASE
    {
        unsafe { *stream = null_mut() };
        return E_NOINTERFACE;
    }
    let pv = unsafe { *activation_params };
    if pv.vt != VT_BLOB || (pv.val1 as usize) < std::mem::size_of::<SpatialAudioActivationParams>() {
        unsafe { *stream = null_mut() };
        return E_INVALIDARG;
    }
    let blob_ptr = pv.val2 as *const SpatialAudioActivationParams;
    if blob_ptr.is_null() {
        unsafe { *stream = null_mut() };
        return E_INVALIDARG;
    }
    let params = unsafe { blob_ptr.read_unaligned() };
    let (frame_count, bytes_per_frame) = if params.object_format.is_null() {
        (FRAMES_PER_10MS, 4usize)
    } else {
        let wf = unsafe { params.object_format.read_unaligned() };
        let fc = ((u64::from(wf.n_samples_per_sec.max(8_000)) * 10) / 1000).max(64) as u32;
        let bpf = usize::from(wf.n_block_align.max(4));
        (fc, bpf)
    };
    let max_dyn = params.max_dynamic_object_count.max(MAX_DYNAMIC_OBJECTS);
    let obj = NullSpatialAudioRenderStream::new(params.event_handle, frame_count, bytes_per_frame, max_dyn);
    unsafe { *stream = Box::into_raw(Box::new(obj)).cast() };
    S_OK
}

static SPATIAL_CLIENT_VTBL: ISpatialAudioClientVtbl = ISpatialAudioClientVtbl {
    unknown: UnknownVtbl {
        query_interface: sac_qi,
        add_ref: sac_add_ref,
        release: sac_release,
    },
    get_static_object_position: sac_get_static_object_position,
    get_native_static_object_type_mask: sac_get_native_mask,
    get_max_dynamic_object_count: sac_get_max_dynamic,
    get_supported_audio_object_format_enumerator: sac_get_supported_formats,
    get_max_frame_count: sac_get_max_frame_count,
    is_audio_object_format_supported: sac_is_format_supported,
    is_spatial_audio_stream_available: sac_is_stream_available,
    activate_spatial_audio_stream: sac_activate_stream,
};

// ---------------------------------------------------------------------------
// ISpatialAudioObjectRenderStream & ISpatialAudioObject
// ---------------------------------------------------------------------------

#[repr(C)]
struct ISpatialAudioObjectRenderStreamVtbl {
    unknown: UnknownVtbl,
    get_available_dynamic_object_count: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
    get_service: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> HRESULT,
    start: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    stop: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    reset: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    begin_updating: unsafe extern "system" fn(*mut c_void, *mut u32, *mut u32) -> HRESULT,
    end_updating: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    activate_object:
        unsafe extern "system" fn(*mut c_void, i32, *mut *mut c_void) -> HRESULT,
}

#[repr(C)]
struct NullSpatialAudioRenderStream {
    vtbl: &'static ISpatialAudioObjectRenderStreamVtbl,
    refs: AtomicU32,
    frame_count: u32,
    bytes_per_frame: usize,
    max_dynamic: u32,
    pump: Mutex<EventPump>,
}

impl NullSpatialAudioRenderStream {
    fn new(event_handle: HANDLE, frame_count: u32, bytes_per_frame: usize, max_dynamic: u32) -> Self {
        Self {
            vtbl: &SPATIAL_STREAM_VTBL,
            refs: AtomicU32::new(1),
            frame_count,
            bytes_per_frame,
            max_dynamic,
            pump: Mutex::new(EventPump::new(event_handle)),
        }
    }
}

impl ComObject for NullSpatialAudioRenderStream {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_ISPATIAL_AUDIO_OBJECT_RENDER_STREAM
            || *iid == IID_ISPATIAL_AUDIO_OBJECT_RENDER_STREAM_BASE
    }
}

iunknown_methods!(NullSpatialAudioRenderStream, sas_qi, sas_add_ref, sas_release);

unsafe extern "system" fn sas_get_avail_dyn(this: *mut c_void, count: *mut u32) -> HRESULT {
    if count.is_null() {
        return E_POINTER;
    }
    let me = unsafe { &*(this.cast::<NullSpatialAudioRenderStream>()) };
    unsafe { *count = me.max_dynamic };
    S_OK
}

unsafe extern "system" fn sas_get_service(
    _this: *mut c_void,
    _riid: *const Guid,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if !ppv.is_null() {
        unsafe { *ppv = null_mut() };
    }
    E_NOINTERFACE
}

unsafe extern "system" fn sas_start(this: *mut c_void) -> HRESULT {
    let me = unsafe { &*(this.cast::<NullSpatialAudioRenderStream>()) };
    me.pump.lock().unwrap_or_else(|e| e.into_inner()).start();
    S_OK
}

unsafe extern "system" fn sas_stop(this: *mut c_void) -> HRESULT {
    let me = unsafe { &*(this.cast::<NullSpatialAudioRenderStream>()) };
    me.pump.lock().unwrap_or_else(|e| e.into_inner()).stop();
    S_OK
}

unsafe extern "system" fn sas_reset(_this: *mut c_void) -> HRESULT {
    S_OK
}

unsafe extern "system" fn sas_begin_updating(
    this: *mut c_void,
    avail_dynamic: *mut u32,
    frame_count: *mut u32,
) -> HRESULT {
    let me = unsafe { &*(this.cast::<NullSpatialAudioRenderStream>()) };
    if !avail_dynamic.is_null() {
        unsafe { *avail_dynamic = me.max_dynamic };
    }
    if !frame_count.is_null() {
        unsafe { *frame_count = me.frame_count };
    }
    S_OK
}

unsafe extern "system" fn sas_end_updating(_this: *mut c_void) -> HRESULT {
    S_OK
}

unsafe extern "system" fn sas_activate_object(
    this: *mut c_void,
    obj_type: i32,
    audio_object: *mut *mut c_void,
) -> HRESULT {
    if audio_object.is_null() {
        return E_POINTER;
    }
    let me = unsafe { &*(this.cast::<NullSpatialAudioRenderStream>()) };
    let byte_len = (me.frame_count as usize).saturating_mul(me.bytes_per_frame);
    unsafe {
        *audio_object =
            Box::into_raw(Box::new(NullSpatialAudioObject::new(obj_type, byte_len))).cast();
    }
    S_OK
}

static SPATIAL_STREAM_VTBL: ISpatialAudioObjectRenderStreamVtbl =
    ISpatialAudioObjectRenderStreamVtbl {
        unknown: UnknownVtbl {
            query_interface: sas_qi,
            add_ref: sas_add_ref,
            release: sas_release,
        },
        get_available_dynamic_object_count: sas_get_avail_dyn,
        get_service: sas_get_service,
        start: sas_start,
        stop: sas_stop,
        reset: sas_reset,
        begin_updating: sas_begin_updating,
        end_updating: sas_end_updating,
        activate_object: sas_activate_object,
    };

#[repr(C)]
struct ISpatialAudioObjectVtbl {
    unknown: UnknownVtbl,
    get_buffer: unsafe extern "system" fn(*mut c_void, *mut *mut u8, *mut u32) -> HRESULT,
    set_end_of_stream: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    is_active: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    get_audio_object_type: unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT,
    set_position: unsafe extern "system" fn(*mut c_void, f32, f32, f32) -> HRESULT,
    set_volume: unsafe extern "system" fn(*mut c_void, f32) -> HRESULT,
}

#[repr(C)]
struct NullSpatialAudioObject {
    vtbl: &'static ISpatialAudioObjectVtbl,
    refs: AtomicU32,
    obj_type: i32,
    active: AtomicBool,
    buffer: Vec<u8>,
}

impl NullSpatialAudioObject {
    fn new(obj_type: i32, byte_len: usize) -> Self {
        Self {
            vtbl: &SPATIAL_OBJECT_VTBL,
            refs: AtomicU32::new(1),
            obj_type,
            active: AtomicBool::new(true),
            buffer: vec![0u8; byte_len.max(4)],
        }
    }
}

impl ComObject for NullSpatialAudioObject {
    fn matches_iid(iid: &Guid) -> bool {
        *iid == IID_ISPATIAL_AUDIO_OBJECT || *iid == IID_ISPATIAL_AUDIO_OBJECT_BASE
    }
}

iunknown_methods!(NullSpatialAudioObject, sao_qi, sao_add_ref, sao_release);

unsafe extern "system" fn sao_get_buffer(
    this: *mut c_void,
    buffer: *mut *mut u8,
    buffer_length: *mut u32,
) -> HRESULT {
    if buffer.is_null() || buffer_length.is_null() {
        return E_POINTER;
    }
    let me = unsafe { &mut *(this.cast::<NullSpatialAudioObject>()) };
    unsafe {
        *buffer = me.buffer.as_mut_ptr();
        *buffer_length = me.buffer.len() as u32;
    }
    S_OK
}

unsafe extern "system" fn sao_set_end_of_stream(this: *mut c_void, _frames: u32) -> HRESULT {
    let me = unsafe { &*(this.cast::<NullSpatialAudioObject>()) };
    me.active.store(false, Ordering::Release);
    S_OK
}

unsafe extern "system" fn sao_is_active(this: *mut c_void, is_active: *mut i32) -> HRESULT {
    if is_active.is_null() {
        return E_POINTER;
    }
    let me = unsafe { &*(this.cast::<NullSpatialAudioObject>()) };
    unsafe { *is_active = i32::from(me.active.load(Ordering::Acquire)) };
    S_OK
}

unsafe extern "system" fn sao_get_type(this: *mut c_void, obj_type: *mut i32) -> HRESULT {
    if obj_type.is_null() {
        return E_POINTER;
    }
    let me = unsafe { &*(this.cast::<NullSpatialAudioObject>()) };
    unsafe { *obj_type = me.obj_type };
    S_OK
}

unsafe extern "system" fn sao_set_position(
    _this: *mut c_void,
    _x: f32,
    _y: f32,
    _z: f32,
) -> HRESULT {
    S_OK
}

unsafe extern "system" fn sao_set_volume(_this: *mut c_void, _vol: f32) -> HRESULT {
    S_OK
}

static SPATIAL_OBJECT_VTBL: ISpatialAudioObjectVtbl = ISpatialAudioObjectVtbl {
    unknown: UnknownVtbl {
        query_interface: sao_qi,
        add_ref: sao_add_ref,
        release: sao_release,
    },
    get_buffer: sao_get_buffer,
    set_end_of_stream: sao_set_end_of_stream,
    is_active: sao_is_active,
    get_audio_object_type: sao_get_type,
    set_position: sao_set_position,
    set_volume: sao_set_volume,
};

pub(crate) fn module() -> HleModule {
    HleModule {
        name: "wasapi_null",
        dlls: &["api-ms-win-core-com-l1-1-0.dll", "ole32.dll", "combase.dll"],
        exports: vec![hle_export!(CoCreateInstance)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null;
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

    #[test]
    fn silent_wasapi_and_spatial_audio_pump_events() {
        let enumerator: *mut c_void = Box::into_raw(Box::new(NullDeviceEnumerator::new())).cast();
        let enum_vt = unsafe { *(enumerator as *const *const IMMDeviceEnumeratorVtbl) };

        let mut dev: *mut c_void = null_mut();
        assert_eq!(
            unsafe { ((*enum_vt).get_default_audio_endpoint)(enumerator, DATA_FLOW_RENDER, 0, &mut dev) },
            S_OK
        );
        assert!(!dev.is_null());
        let dev_vt = unsafe { *(dev as *const *const IMMDeviceVtbl) };

        // Property store FormFactor == Speakers (1).
        let mut store: *mut c_void = null_mut();
        assert_eq!(unsafe { ((*dev_vt).open_property_store)(dev, 0, &mut store) }, S_OK);
        let store_vt = unsafe { *(store as *const *const IPropertyStoreVtbl) };
        let key = PropertyKey {
            fmtid: PKEY_AUDIO_ENDPOINT_FORM_FACTOR_FMTID,
            pid: 0,
        };
        let mut pv = PropVariant {
            vt: 0,
            reserved: [0; 3],
            val1: 0,
            val2: 0,
        };
        assert_eq!(unsafe { ((*store_vt).get_value)(store, &key, &mut pv) }, S_OK);
        assert_eq!(pv.vt, VT_UI4);
        assert_eq!(pv.val1 as u32, FORM_FACTOR_SPEAKERS);

        // IAudioClient::GetMixFormat + CoTaskMemFree.
        let mut audio_client: *mut c_void = null_mut();
        assert_eq!(
            unsafe { ((*dev_vt).activate)(dev, &IID_IAUDIO_CLIENT, 1, null(), &mut audio_client) },
            S_OK
        );
        let ac_vt = unsafe { *(audio_client as *const *const IAudioClientVtbl) };
        let mut mix: *mut WaveFormatEx = null_mut();
        assert_eq!(unsafe { ((*ac_vt).get_mix_format)(audio_client, &mut mix) }, S_OK);
        assert!(!mix.is_null());
        let mix_wf = unsafe { mix.read_unaligned() };
        let rate = mix_wf.n_samples_per_sec;
        let channels = mix_wf.n_channels;
        assert_eq!((rate, channels), (48_000, 2));
        if let Some(free_addr) = crate::host::function("api-ms-win-core-com-l1-1-0.dll", "CoTaskMemFree") {
            let co_free: unsafe extern "system" fn(*mut c_void) = unsafe { std::mem::transmute(free_addr) };
            unsafe { co_free(mix.cast()) };
        }

        // ISpatialAudioClient -> GetMaxFrameCount -> ActivateSpatialAudioStream -> Event pump.
        let mut spatial: *mut c_void = null_mut();
        assert_eq!(
            unsafe { ((*dev_vt).activate)(dev, &IID_ISPATIAL_AUDIO_CLIENT, 1, null(), &mut spatial) },
            S_OK
        );
        let sac_vt = unsafe { *(spatial as *const *const ISpatialAudioClientVtbl) };
        let mut fmt_enum: *mut c_void = null_mut();
        assert_eq!(
            unsafe { ((*sac_vt).get_supported_audio_object_format_enumerator)(spatial, &mut fmt_enum) },
            S_OK
        );
        let fe_vt = unsafe { *(fmt_enum as *const *const IAudioFormatEnumeratorVtbl) };
        let mut obj_fmt: *mut WaveFormatEx = null_mut();
        assert_eq!(unsafe { ((*fe_vt).get_format)(fmt_enum, 0, &mut obj_fmt) }, S_OK);
        let mut max_frames = 0u32;
        assert_eq!(
            unsafe { ((*sac_vt).get_max_frame_count)(spatial, obj_fmt, &mut max_frames) },
            S_OK
        );
        assert_eq!(max_frames, FRAMES_PER_10MS);

        let ev = unsafe { CreateEventW(null(), 0, 0, null()) };
        assert!(!ev.is_null());
        let mut act = SpatialAudioActivationParams {
            object_format: obj_fmt,
            static_object_type_mask: STATIC_MASK_STEREO,
            min_dynamic_object_count: 0,
            max_dynamic_object_count: 8,
            category: 6,
            event_handle: ev,
            notify_object: null_mut(),
        };
        let mut act_pv = PropVariant {
            vt: VT_BLOB,
            reserved: [0; 3],
            val1: std::mem::size_of::<SpatialAudioActivationParams>() as u64,
            val2: (&raw mut act) as u64,
        };
        let mut stream: *mut c_void = null_mut();
        assert_eq!(
            unsafe {
                ((*sac_vt).activate_spatial_audio_stream)(
                    spatial,
                    &act_pv,
                    &IID_ISPATIAL_AUDIO_OBJECT_RENDER_STREAM,
                    &mut stream,
                )
            },
            S_OK
        );
        let st_vt = unsafe { *(stream as *const *const ISpatialAudioObjectRenderStreamVtbl) };
        assert_eq!(unsafe { ((*st_vt).start)(stream) }, S_OK);
        assert_eq!(unsafe { WaitForSingleObject(ev, 500) }, WAIT_OBJECT_0);

        let mut avail = 0u32;
        let mut fc = 0u32;
        assert_eq!(unsafe { ((*st_vt).begin_updating)(stream, &mut avail, &mut fc) }, S_OK);
        assert_eq!(fc, FRAMES_PER_10MS);
        let mut obj: *mut c_void = null_mut();
        assert_eq!(unsafe { ((*st_vt).activate_object)(stream, 0x02, &mut obj) }, S_OK);
        let obj_vt = unsafe { *(obj as *const *const ISpatialAudioObjectVtbl) };
        let mut buf: *mut u8 = null_mut();
        let mut blen = 0u32;
        assert_eq!(unsafe { ((*obj_vt).get_buffer)(obj, &mut buf, &mut blen) }, S_OK);
        assert!(!buf.is_null() && blen >= FRAMES_PER_10MS * 4);
        assert_eq!(unsafe { ((*st_vt).end_updating)(stream) }, S_OK);
        assert_eq!(unsafe { ((*st_vt).stop)(stream) }, S_OK);

        unsafe {
            com_release_raw(obj);
            com_release_raw(stream);
            com_release_raw(fmt_enum);
            com_release_raw(spatial);
            com_release_raw(audio_client);
            com_release_raw(store);
            com_release_raw(dev);
            com_release_raw(enumerator);
            CloseHandle(ev);
        }
    }
}
