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
    Foundation::{POINT, RECT},
    Graphics::{
        Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS},
        Gdi::{
            ClientToScreen, GetMonitorInfoW, MonitorFromWindow, MONITORINFO,
            MONITOR_DEFAULTTONEAREST,
        },
    },
    UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, IsIconic, IsWindow, IsZoomed},
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
        let mut rect = self.frame()?;
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

struct Apartment(bool);
impl Apartment {
    fn new() -> Result<Self> {
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
            let full = self.target.frame()?;
            let crop = self.target.bounds()?;
            let left = ((i64::from(crop.left - full.left) * i64::from(size.Width))
                / i64::from(full.right - full.left))
            .clamp(0, i64::from(size.Width)) as u32;
            let top = ((i64::from(crop.top - full.top) * i64::from(size.Height))
                / i64::from(full.bottom - full.top))
            .clamp(0, i64::from(size.Height)) as u32;
            let width = (size.Width as u32 - left).min(self.width as u32);
            let height = (size.Height as u32 - top).min(self.height as u32);
            if width == 0 || height == 0 {
                return Err(Error::from_hresult(E_FAIL));
            }
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
pub(crate) mod test_support {
    use super::*;
    use std::{ptr::null, sync::atomic::AtomicUsize};
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{Dwm::DwmFlush, Gdi::*},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{
                SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            },
            WindowsAndMessaging::*,
        },
    };

    static NEXT: AtomicUsize = AtomicUsize::new(0);
    pub const GREEN: u32 = 0x0020c040;
    pub const RED: u32 = 0x002040e0;
    pub const BLUE: u32 = 0x00e06020;

    unsafe extern "system" fn paint(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_PAINT {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let brush = CreateSolidBrush(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32);
            if !brush.is_null() {
                FillRect(dc, &ps.rcPaint, brush);
                DeleteObject(brush);
            }
            EndPaint(hwnd, &ps);
            0
        } else {
            DefWindowProcW(hwnd, msg, wp, lp)
        }
    }

    pub struct CoveredWindow {
        pub target: HWND,
        cover: HWND,
        class: Vec<u16>,
        dpi: DPI_AWARENESS_CONTEXT,
    }
    impl CoveredWindow {
        pub fn new() -> Self {
            // SAFETY: these are owned test windows, painted on this test thread.
            unsafe {
                let dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                let class = crate::w(&format!(
                    "SnippingLiteCaptureTest{}",
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                let spec = WNDCLASSW {
                    lpfnWndProc: Some(paint),
                    hInstance: GetModuleHandleW(null()),
                    lpszClassName: class.as_ptr(),
                    ..std::mem::zeroed()
                };
                assert_ne!(RegisterClassW(&spec), 0);
                let create = |color, style| {
                    let hwnd = CreateWindowExW(
                        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                        class.as_ptr(),
                        crate::w("Snipping Lite capture validation").as_ptr(),
                        style,
                        80,
                        80,
                        320,
                        240,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        GetModuleHandleW(null()),
                        null(),
                    );
                    assert!(!hwnd.is_null());
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, color as isize);
                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    UpdateWindow(hwnd);
                    hwnd
                };
                let target = create(GREEN, WS_OVERLAPPEDWINDOW);
                let cover = create(RED, WS_POPUP);
                SetWindowPos(cover, HWND_TOPMOST, 80, 80, 320, 240, SWP_NOACTIVATE);
                DwmFlush();
                Self {
                    target,
                    cover,
                    class,
                    dpi,
                }
            }
        }

        pub fn recolor(&self, color: u32) {
            unsafe {
                SetWindowLongPtrW(self.target, GWLP_USERDATA, color as isize);
                InvalidateRect(self.target, null(), 1);
                UpdateWindow(self.target);
                DwmFlush();
            }
        }

        pub fn desktop_is_red(&self) -> bool {
            unsafe {
                let dc = GetDC(std::ptr::null_mut());
                let color = GetPixel(dc, 200, 200);
                ReleaseDC(std::ptr::null_mut(), dc);
                color & 0xff > ((color >> 8) & 0xff) + 60
            }
        }
    }
    impl Drop for CoveredWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.cover);
                DestroyWindow(self.target);
                UnregisterClassW(self.class.as_ptr(), GetModuleHandleW(null()));
                if !self.dpi.is_null() {
                    SetThreadDpiAwarenessContext(self.dpi);
                }
            }
        }
    }

    pub fn center(pixels: &[u8], width: i32, height: i32) -> [u8; 4] {
        let offset = ((height / 2 * width + width / 2) * 4) as usize;
        [
            pixels[offset],
            pixels[offset + 1],
            pixels[offset + 2],
            pixels[offset + 3],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::*;

    #[test]
    fn invalid_window_never_falls_back_to_the_desktop() {
        let target = Target::new(std::ptr::null_mut(), false);
        assert!(snapshot(target).is_err());
        let path = std::env::temp_dir().join(format!(
            "snippinglite-invalid-window-{}.mp4",
            std::process::id()
        ));
        assert!(crate::record::start(
            windows_sys::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: 320,
                bottom: 240
            },
            path.clone(),
            Vec::new(),
            crate::record::Options {
                fps: 60,
                cursor: false,
                window: Some(target)
            }
        )
        .is_err());
        assert!(!path.exists());
    }

    #[test]
    #[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
    fn capture_ignores_occluding_windows_and_keeps_updating() {
        let fixture = CoveredWindow::new();
        assert!(
            fixture.desktop_is_red(),
            "The target must actually be completely covered"
        );
        let image =
            snapshot(Target::new(fixture.target, false)).expect("Capture the covered window");
        let pixel = center(&image.pixels, image.width, image.height);
        assert!(
            u16::from(pixel[1]) > u16::from(pixel[0]) + 60
                && u16::from(pixel[1]) > u16::from(pixel[2]) + 60,
            "Window pixels: {pixel:?}"
        );
        let client = snapshot(Target::new(fixture.target, true)).unwrap();
        let client_bounds = Target::new(fixture.target, true).bounds().unwrap();
        assert_eq!(
            (client.width, client.height),
            (
                client_bounds.right - client_bounds.left,
                client_bounds.bottom - client_bounds.top
            )
        );
        for (x, y) in [(3, 3), (client.width - 4, client.height - 4)] {
            let offset = ((y * client.width + x) * 4) as usize;
            assert!(
                client.pixels[offset + 1] > 150 && client.pixels[offset + 2] < 120,
                "The client-only crop includes non-client pixels"
            );
        }
        let mut capture =
            WindowCapture::new(Target::new(fixture.target, false), 320, 240, false).unwrap();
        capture.grab().unwrap();
        fixture.recolor(BLUE);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let pixel = center(capture.grab().unwrap(), 320, 240);
            if u16::from(pixel[0]) > u16::from(pixel[1]) + 60
                && u16::from(pixel[0]) > u16::from(pixel[2]) + 60
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Covered window did not update: {pixel:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            fixture.desktop_is_red(),
            "The covering window must remain above the target"
        );
    }
}
