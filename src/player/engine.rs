//! Playback of a recorded video: the WinRT media player decodes, plays the sound and seeks,
//! and serves its frames to a Direct3D texture that is read back as BGRA pixels.

use crate::window_capture::Apartment;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use windows::{
    core::{Error, Interface, Result, HSTRING},
    Foundation::{TimeSpan, TypedEventHandler, Uri},
    Graphics::DirectX::Direct3D11::IDirect3DSurface,
    Media::{
        Core::MediaSource,
        Playback::{MediaPlaybackState, MediaPlayer},
    },
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, HMODULE},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::*,
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGISurface,
            },
        },
        System::WinRT::Direct3D11::CreateDirect3D11SurfaceFromDXGISurface,
    },
};

/// One decoded video frame, top-down BGRA.
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// The Direct3D side, built once the video is open and its size is known.
struct Surface {
    context: ID3D11DeviceContext,
    target: ID3D11Texture2D,
    surface: IDirect3DSurface,
    staging: ID3D11Texture2D,
    width: usize,
    height: usize,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Status {
    Opening,
    Ready,
    Failed,
}

pub struct Engine {
    player: MediaPlayer,
    opened: Arc<AtomicBool>,
    broken: Arc<AtomicBool>,
    fresh: Arc<AtomicBool>,
    surface: Option<Surface>,
    // COM references above must be released before the apartment is uninitialized.
    _apartment: Apartment,
}

fn failure() -> Error {
    Error::from_hresult(E_FAIL)
}

fn time(ticks: i64) -> Duration {
    Duration::from_nanos(ticks.max(0) as u64 * 100)
}

fn span(at: Duration) -> TimeSpan {
    TimeSpan {
        Duration: (at.as_nanos() / 100).min(i64::MAX as u128) as i64,
    }
}

/// The `file:` address of a local path, which the media source opens.
fn address(path: &Path) -> Result<Uri> {
    let full = std::fs::canonicalize(path).map_err(|_| failure())?;
    // Canonical paths carry the `\\?\` prefix, which is not part of the address.
    let text = full.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    Uri::CreateUri(&HSTRING::from(format!(
        "file:///{}",
        text.replace('\\', "/")
    )))
}

impl Engine {
    /// Starts opening `path`. The player reports through events that the window's message
    /// loop delivers, so this does not wait; ask `status` until it leaves `Opening`.
    pub fn open(path: &Path) -> Result<Self> {
        let apartment = Apartment::new()?;
        let player = MediaPlayer::new()?;
        player.SetAutoPlay(false)?;
        player.SetIsVideoFrameServerEnabled(true)?;
        let (opened, broken, fresh) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let flag = |value: &Arc<AtomicBool>| {
            let value = value.clone();
            TypedEventHandler::new(move |_, _| {
                value.store(true, Ordering::Release);
                Ok(())
            })
        };
        player.MediaOpened(&flag(&opened))?;
        let failed = broken.clone();
        player.MediaFailed(&TypedEventHandler::new(move |_, _| {
            failed.store(true, Ordering::Release);
            Ok(())
        }))?;
        player.VideoFrameAvailable(&flag(&fresh))?;
        player.SetSource(&MediaSource::CreateFromUri(&address(path)?)?)?;
        Ok(Self {
            player,
            opened,
            broken,
            fresh,
            surface: None,
            _apartment: apartment,
        })
    }

    /// Where the opening stands. Once the video is open this builds the Direct3D surface.
    pub fn status(&mut self) -> Status {
        if self.broken.load(Ordering::Acquire) {
            return Status::Failed;
        }
        if self.surface.is_some() {
            return Status::Ready;
        }
        if !self.opened.load(Ordering::Acquire) {
            return Status::Opening;
        }
        match self.build() {
            Ok(surface) => {
                self.surface = Some(surface);
                Status::Ready
            }
            Err(_) => {
                self.broken.store(true, Ordering::Release);
                Status::Failed
            }
        }
    }

    fn build(&self) -> Result<Surface> {
        let session = self.player.PlaybackSession()?;
        let (width, height) = (
            session.NaturalVideoWidth()? as usize,
            session.NaturalVideoHeight()? as usize,
        );
        if width == 0 || height == 0 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        // SAFETY: the Direct3D objects are created and used on this thread only.
        unsafe {
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
            let device = device.ok_or_else(failure)?;
            let context = context.ok_or_else(failure)?;
            let mut description = D3D11_TEXTURE2D_DESC {
                Width: width as u32,
                Height: height as u32,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                ..Default::default()
            };
            let mut target = None;
            device.CreateTexture2D(&description, None, Some(&mut target))?;
            let target = target.ok_or_else(failure)?;
            description.Usage = D3D11_USAGE_STAGING;
            description.BindFlags = 0;
            description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            let mut staging = None;
            device.CreateTexture2D(&description, None, Some(&mut staging))?;
            let dxgi: IDXGISurface = target.cast()?;
            let surface: IDirect3DSurface =
                CreateDirect3D11SurfaceFromDXGISurface(&dxgi)?.cast()?;
            Ok(Surface {
                context,
                target,
                surface,
                staging: staging.ok_or_else(failure)?,
                width,
                height,
            })
        }
    }

    pub fn duration(&self) -> Duration {
        self.player
            .PlaybackSession()
            .and_then(|s| s.NaturalDuration())
            .map_or(Duration::ZERO, |d| time(d.Duration))
    }

    pub fn position(&self) -> Duration {
        self.player
            .PlaybackSession()
            .and_then(|s| s.Position())
            .map_or(Duration::ZERO, |d| time(d.Duration))
    }

    pub fn seek(&self, at: Duration) {
        if let Ok(session) = self.player.PlaybackSession() {
            let _ = session.SetPosition(span(at.min(self.duration())));
        }
    }

    pub fn play(&self) {
        let _ = self.player.Play();
    }

    pub fn pause(&self) {
        let _ = self.player.Pause();
    }

    pub fn playing(&self) -> bool {
        self.player
            .PlaybackSession()
            .and_then(|s| s.PlaybackState())
            .is_ok_and(|state| state == MediaPlaybackState::Playing)
    }

    /// Whether playback has reached the end of the file.
    pub fn ended(&self) -> bool {
        let duration = self.duration();
        !duration.is_zero()
            && !self.playing()
            && self.position() + Duration::from_millis(50) >= duration
    }

    pub fn set_volume(&self, volume: f64) {
        let _ = self.player.SetVolume(volume.clamp(0.0, 1.0));
    }

    pub fn volume(&self) -> f64 {
        self.player.Volume().unwrap_or(1.0)
    }

    /// The newest frame, or `None` while the player has no new one.
    pub fn frame(&mut self) -> Result<Option<Picture>> {
        if self.status() != Status::Ready || !self.fresh.swap(false, Ordering::AcqRel) {
            return Ok(None);
        }
        let Some(gpu) = &self.surface else {
            return Ok(None);
        };
        self.player.CopyFrameToVideoSurface(&gpu.surface)?;
        // SAFETY: the mapped rows are bounds checked against the pitch.
        unsafe {
            gpu.context.CopyResource(&gpu.staging, &gpu.target);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            gpu.context
                .Map(&gpu.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let row = gpu.width * 4;
            if mapped.pData.is_null() || (mapped.RowPitch as usize) < row {
                gpu.context.Unmap(&gpu.staging, 0);
                return Err(failure());
            }
            let mut pixels = vec![0u8; row * gpu.height];
            for y in 0..gpu.height {
                std::ptr::copy_nonoverlapping(
                    (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                    pixels.as_mut_ptr().add(y * row),
                    row,
                );
            }
            gpu.context.Unmap(&gpu.staging, 0);
            Ok(Some(Picture {
                width: gpu.width,
                height: gpu.height,
                pixels,
            }))
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.player.Close();
    }
}
