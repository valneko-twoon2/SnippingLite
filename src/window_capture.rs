//! Window-only DWM capture. Desktop pixels are never used as a fallback.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
use windows::{
    core::{factory, Error, Interface, Result, HRESULT},
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{
            Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
            GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Foundation::{E_FAIL, HMODULE, HWND, RPC_E_CHANGED_MODE},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::*,
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIDevice,
            },
        },
        System::{
            Com::CoIncrementMTAUsage,
            WinRT::{
                Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
                Graphics::Capture::IGraphicsCaptureItemInterop,
                RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED,
            },
        },
    },
};
use windows_sys::Win32::{
    Foundation::{BOOL, LPARAM, POINT, RECT},
    Graphics::{
        Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS},
        Gdi::{
            ClientToScreen, GetMonitorInfoW, MonitorFromWindow, MONITORINFO,
            MONITOR_DEFAULTTONEAREST,
        },
    },
    UI::WindowsAndMessaging::{
        EnumChildWindows, GetClassNameW, GetClientRect, GetWindowRect, GetWindowThreadProcessId,
        IsIconic, IsWindow, IsZoomed,
    },
};

#[derive(Clone, Copy, Debug)]
pub struct Target {
    // HWND is an opaque, process-wide identifier, not an owned pointer.
    id: usize,
    client_only: bool,
}

impl Target {
    pub fn new(hwnd: windows_sys::Win32::Foundation::HWND, client_only: bool) -> Self {
        Self {
            id: hwnd as usize,
            client_only,
        }
    }

    /// The process whose sound belongs to this window. A UWP app is shown in a
    /// frame window of another process, so its own window is looked up inside.
    pub fn process_id(self) -> Option<u32> {
        // SAFETY: Windows validates this borrowed window identifier.
        unsafe {
            let hwnd = self.id as windows_sys::Win32::Foundation::HWND;
            let mut id = 0;
            GetWindowThreadProcessId(hwnd, &mut id);
            if id == 0 {
                return None;
            }
            let mut class = [0u16; 64];
            let length = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
            if String::from_utf16_lossy(&class[..length.max(0) as usize])
                == "ApplicationFrameWindow"
            {
                let mut found = (id, 0u32);
                EnumChildWindows(hwnd, Some(other_process), &mut found as *mut _ as LPARAM);
                if found.1 != 0 {
                    return Some(found.1);
                }
            }
            Some(id)
        }
    }

    /// The screen area of a captured frame: the visible window frame.
    fn frame(self) -> Result<RECT> {
        // SAFETY: Windows validates this borrowed window identifier.
        unsafe {
            let hwnd = self.id as windows_sys::Win32::Foundation::HWND;
            if IsWindow(hwnd) == 0 || IsIconic(hwnd) != 0 {
                return Err(Error::from_hresult(E_FAIL));
            }
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return Err(Error::from_thread());
            }
            let _ = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut rect as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            );
            Ok(rect)
        }
    }

    /// The captured screen area, a part of `frame`.
    pub fn bounds(self) -> Result<RECT> {
        self.bounds_within(self.frame()?)
    }

    /// 読んである枠 `rect`（`frame` の結果）のうちの、撮る範囲。枠を読み直さないので、撮った絵の
    /// どこを写すかを、範囲と同じ時に読んだ枠から決められる。
    fn bounds_within(self, mut rect: RECT) -> Result<RECT> {
        // SAFETY: Windows validates this borrowed window identifier.
        unsafe {
            let hwnd = self.id as windows_sys::Win32::Foundation::HWND;
            // DWM draws a translucent 1px border and rounded corners on restored
            // windows; flattened onto black they look like pixels from outside.
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            let fullscreen =
                GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info) != 0
                    && (rect.left, rect.top, rect.right, rect.bottom)
                        == (
                            info.rcMonitor.left,
                            info.rcMonitor.top,
                            info.rcMonitor.right,
                            info.rcMonitor.bottom,
                        );
            if IsZoomed(hwnd) == 0 && !fullscreen {
                rect.left += 1;
                rect.top += 1;
                rect.right -= 1;
                rect.bottom -= 1;
            }
            if self.client_only {
                let mut client: RECT = std::mem::zeroed();
                if GetClientRect(hwnd, &mut client) == 0 {
                    return Err(Error::from_thread());
                }
                let mut origin = POINT { x: 0, y: 0 };
                if ClientToScreen(hwnd, &mut origin) == 0 {
                    return Err(Error::from_thread());
                }
                // Custom title bars extend the client area into the border.
                rect.left = rect.left.max(client.left + origin.x);
                rect.top = rect.top.max(client.top + origin.y);
                rect.right = rect.right.min(client.right + origin.x);
                rect.bottom = rect.bottom.min(client.bottom + origin.y);
            }
        }
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return Err(Error::from_hresult(E_FAIL));
        }
        Ok(rect)
    }
}

/// `EnumChildWindows` callback; `lparam` points at (frame process, other process).
unsafe extern "system" fn other_process(
    child: windows_sys::Win32::Foundation::HWND,
    lparam: LPARAM,
) -> BOOL {
    let found = &mut *(lparam as *mut (u32, u32));
    let mut id = 0;
    GetWindowThreadProcessId(child, &mut id);
    if id != 0 && id != found.0 {
        found.1 = id;
        return 0;
    }
    1
}

pub(crate) struct Apartment(bool);
impl Apartment {
    pub(crate) fn new() -> Result<Self> {
        // WinRT activation factories are cached for the process. Keep the MTA
        // alive across individual snapshots and recorder threads as well.
        static MTA: OnceLock<std::result::Result<(), HRESULT>> = OnceLock::new();
        if let Err(code) = MTA.get_or_init(|| unsafe {
            CoIncrementMTAUsage()
                .map(|_| ())
                .map_err(|error| error.code())
        }) {
            return Err(Error::from_hresult(*code));
        }
        // SAFETY: balanced on this thread; an existing STA is also usable.
        match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            Ok(()) => Ok(Self(true)),
            Err(error) if error.code() == RPC_E_CHANGED_MODE => Ok(Self(false)),
            Err(error) => Err(error),
        }
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe { RoUninitialize() };
        }
    }
}

struct Frame(Direct3D11CaptureFrame);
impl Drop for Frame {
    fn drop(&mut self) {
        let _ = self.0.Close();
    }
}

pub struct WindowCapture {
    target: Target,
    item: GraphicsCaptureItem,
    closed: Arc<AtomicBool>,
    closed_token: i64,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    winrt_device: IDirect3DDevice,
    context: ID3D11DeviceContext,
    staging: ID3D11Texture2D,
    pool_size: SizeInt32,
    width: i32,
    height: i32,
    pixels: Vec<u8>,
    has_frame: bool,
    // COM references above must be released before the apartment is uninitialized.
    _apartment: Apartment,
}

impl WindowCapture {
    pub fn new(target: Target, width: i32, height: i32, cursor: bool) -> Result<Self> {
        let apartment = Apartment::new()?;
        target.bounds()?;
        if width < 1 || height < 1 || !GraphicsCaptureSession::IsSupported()? {
            return Err(Error::from_hresult(E_FAIL));
        }
        // SAFETY: D3D and WinRT objects remain on their creating thread.
        unsafe {
            let factory: IGraphicsCaptureItemInterop = factory::<GraphicsCaptureItem, _>()?;
            let item: GraphicsCaptureItem = factory.CreateForWindow(HWND(target.id as *mut _))?;
            let size = item.Size()?;
            let (mut device, mut context) = (None, None);
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device = device.ok_or_else(|| Error::from_hresult(E_FAIL))?;
            let context = context.ok_or_else(|| Error::from_hresult(E_FAIL))?;
            let dxgi: IDXGIDevice = device.cast()?;
            let winrt_device: IDirect3DDevice =
                CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()?;
            let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                &winrt_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                size,
            )?;
            let session = pool.CreateCaptureSession(&item)?;
            session.SetIsCursorCaptureEnabled(cursor)?;
            // Borderless access can be unavailable. Do not request permissions or
            // substitute a desktop grab; the OS border can remain in that case.
            let _ = session.SetIsBorderRequired(false);
            let closed = Arc::new(AtomicBool::new(false));
            let signal = closed.clone();
            let closed_token = item.Closed(&TypedEventHandler::new(move |_, _| {
                signal.store(true, Ordering::Release);
                Ok(())
            }))?;
            let description = D3D11_TEXTURE2D_DESC {
                Width: width as u32,
                Height: height as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..Default::default()
            };
            let mut staging = None;
            device.CreateTexture2D(&description, None, Some(&mut staging))?;
            let length = (width as usize)
                .checked_mul(height as usize)
                .and_then(|n| n.checked_mul(4))
                .ok_or_else(|| Error::from_hresult(E_FAIL))?;
            let capture = Self {
                target,
                item,
                closed,
                closed_token,
                pool,
                session,
                winrt_device,
                context,
                staging: staging.ok_or_else(|| Error::from_hresult(E_FAIL))?,
                pool_size: size,
                width,
                height,
                pixels: vec![0; length],
                has_frame: false,
                _apartment: apartment,
            };
            capture.session.StartCapture()?;
            Ok(capture)
        }
    }

    pub fn grab(&mut self) -> Result<&[u8]> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if self.closed.load(Ordering::Acquire) {
                return Err(Error::from_hresult(E_FAIL));
            }
            self.target.bounds()?;
            let Some(mut frame) = self.pool.TryGetNextFrame().ok().map(Frame) else {
                if self.has_frame {
                    return Ok(&self.pixels);
                }
                if Instant::now() >= deadline {
                    return Err(Error::from_hresult(E_FAIL));
                }
                std::thread::sleep(Duration::from_millis(1));
                continue;
            };
            // Keep only the newest available frame, returning older buffers.
            while let Ok(next) = self.pool.TryGetNextFrame() {
                frame = Frame(next);
            }
            let size = frame.0.ContentSize()?;
            if size.Width < 1 || size.Height < 1 {
                return Err(Error::from_hresult(E_FAIL));
            }
            if size != self.pool_size {
                drop(frame);
                self.pool.Recreate(
                    &self.winrt_device,
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    2,
                    size,
                )?;
                self.pool_size = size;
                continue;
            }
            // 枠は 1 回だけ読み、撮る範囲もその枠から決める。別々に読むと、間で窓の大きさが
            // 変わったとき（0 の大きさから戻るときなど）に、範囲と枠が食い違う。
            let full = self.target.frame()?;
            let crop = self.target.bounds_within(full)?;
            let Some((left, top, width, height)) =
                source_box(full, crop, size, (self.width, self.height))
            else {
                return Err(Error::from_hresult(E_FAIL));
            };
            // SAFETY: the copy box fits the frame; mapped rows are bounds checked.
            unsafe {
                let access: IDirect3DDxgiInterfaceAccess = frame.0.Surface()?.cast()?;
                let texture: ID3D11Texture2D = access.GetInterface()?;
                let region = D3D11_BOX {
                    left,
                    top,
                    right: left + width,
                    bottom: top + height,
                    front: 0,
                    back: 1,
                };
                self.context.CopySubresourceRegion(
                    &self.staging,
                    0,
                    0,
                    0,
                    0,
                    &texture,
                    0,
                    Some(&region),
                );
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                self.context
                    .Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                if mapped.pData.is_null() || mapped.RowPitch < width * 4 {
                    self.context.Unmap(&self.staging, 0);
                    return Err(Error::from_hresult(E_FAIL));
                }
                // Resizing keeps the encoder dimensions fixed, padding uncovered
                // pixels with black rather than including neighboring windows.
                self.pixels.fill(0);
                for y in 0..height as usize {
                    std::ptr::copy_nonoverlapping(
                        (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                        self.pixels.as_mut_ptr().add(y * self.width as usize * 4),
                        width as usize * 4,
                    );
                }
                self.context.Unmap(&self.staging, 0);
            }
            self.has_frame = true;
            return Ok(&self.pixels);
        }
    }
}

impl Drop for WindowCapture {
    fn drop(&mut self) {
        let _ = self.item.RemoveClosed(self.closed_token);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// 撮った絵（大きさ `size`）のうち、写す範囲（左, 上, 幅, 高さ）。絵は窓の枠 `full` の全体なので、
/// 撮る範囲 `crop` の左上の、枠の左上からの位置を、枠と絵の大きさの比で絵の画素に直す。幅と高さは、
/// 絵の残りと出力の大きさ `limit` の小さいほう。枠の幅か高さが 1 未満だと比が決まらない（0 で
/// 割ることになる）ので、写すものがないときと同じく `None` を返す。
fn source_box(
    full: RECT,
    crop: RECT,
    size: SizeInt32,
    limit: (i32, i32),
) -> Option<(u32, u32, u32, u32)> {
    // 枠の `near` から `far` までの中の `at` を、`length` 画素の絵の中の位置に直す。座標は i64 で
    // 引くので、画面の端の大きな値でもあふれない（差は 2^32 未満、掛けても 2^63 未満）。
    let scale = |near: i32, far: i32, at: i32, length: i32| {
        let whole = i64::from(far) - i64::from(near);
        if whole < 1 || length < 1 {
            return None;
        }
        let offset = (i64::from(at) - i64::from(near)) * i64::from(length) / whole;
        Some(offset.clamp(0, i64::from(length)) as u32)
    };
    let left = scale(full.left, full.right, crop.left, size.Width)?;
    let top = scale(full.top, full.bottom, crop.top, size.Height)?;
    let width = (size.Width as u32 - left).min(limit.0.max(0) as u32);
    let height = (size.Height as u32 - top).min(limit.1.max(0) as u32);
    (width > 0 && height > 0).then_some((left, top, width, height))
}

pub struct Snapshot {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
}

pub fn snapshot(target: Target) -> Result<Snapshot> {
    let bounds = target.bounds()?;
    let (width, height) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
    let mut capture = WindowCapture::new(target, width, height, false)?;
    let mut pixels = capture.grab()?.to_vec();
    // The screenshot API has always returned an opaque window image.
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    Ok(Snapshot {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
