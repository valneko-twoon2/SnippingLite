//! Audio for recordings: the system sound, the sound of one app (WASAPI process
//! loopback) and the microphone, mixed into 48 kHz stereo 16-bit PCM.

use super::TICKS;
use std::{
    mem::ManuallyDrop,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
use windows::{
    core::{implement, Interface, Ref, HRESULT},
    Win32::{
        Media::Audio::*,
        System::{
            Com::{
                CoCreateInstance, IAgileObject, IAgileObject_Impl, StructuredStorage::PROPVARIANT,
                BLOB, CLSCTX_ALL,
            },
            Variant::VT_BLOB,
        },
    },
};

pub const RATE: u32 = 48_000;
/// Audio is held back this long so a device that delivers in bursts never leaves gaps.
const LATENCY: usize = (RATE / 20) as usize;
/// `LATENCY` as a duration.
const LATENCY_TIME: Duration = Duration::from_millis(50);

/// Where the sound of a recording comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Loopback {
    /// Everything that plays on the default output.
    System,
    /// One process and the processes it started, such as a browser's audio service.
    Process(u32),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Audio {
    pub loopback: Option<Loopback>,
    pub mic: bool,
}

impl Audio {
    pub fn any(self) -> bool {
        self.loopback.is_some() || self.mic
    }
}

/// Audio due up to some moment, as PCM and its position in the file timeline.
pub struct Chunk {
    pub pcm: Vec<u8>,
    pub time: i64,
    pub duration: i64,
}

/// Adds the sources together on one timeline that follows the wall clock. A
/// source that has nothing to say (an idle loopback delivers no data at all)
/// contributes silence.
struct Mixer {
    queues: Vec<Vec<f32>>,
    emitted: usize,
}

impl Mixer {
    fn new(sources: usize) -> Mixer {
        Mixer {
            queues: vec![Vec::new(); sources],
            emitted: 0,
        }
    }

    /// Queues interleaved stereo samples of one source.
    fn push(&mut self, source: usize, samples: &[f32]) {
        self.queues[source].extend_from_slice(samples);
    }

    /// Mixes the frames that are due `elapsed` after the start.
    fn take(&mut self, elapsed: Duration) -> Option<Chunk> {
        let due = (elapsed.as_secs_f64() * f64::from(RATE)) as usize;
        let frames = due.saturating_sub(LATENCY).saturating_sub(self.emitted);
        if frames == 0 {
            return None;
        }
        let mut mix = vec![0f32; frames * 2];
        for queue in &mut self.queues {
            let used = queue.len().min(mix.len());
            for (sum, sample) in mix.iter_mut().zip(queue.drain(..used)) {
                *sum += sample;
            }
            // A device clock that runs fast piles up a backlog; drop it so the
            // sound cannot drift away from the picture.
            if queue.len() > LATENCY * 2 * 8 {
                queue.drain(..queue.len() - LATENCY * 2 * 2);
            }
        }
        let pcm = mix
            .iter()
            .flat_map(|sample| ((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
            .collect();
        let time = (self.emitted as i64) * TICKS / i64::from(RATE);
        self.emitted += frames;
        Some(Chunk {
            pcm,
            time,
            duration: (frames as i64) * TICKS / i64::from(RATE),
        })
    }
}

/// Where a source takes its sound from, so it can be opened again.
#[derive(Clone, Copy)]
enum Kind {
    Loopback(Loopback),
    Mic,
}

/// How long a lost device is left alone before it is opened again.
const REOPEN_AFTER: Duration = Duration::from_secs(1);

struct Source {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    kind: Kind,
    /// When the device stopped delivering (unplugged, or the default output changed).
    lost: Option<Instant>,
}

impl Source {
    /// Moves everything captured so far into `out`; `false` means the device is gone.
    unsafe fn read(&self, out: &mut Vec<f32>) -> bool {
        loop {
            match self.capture.GetNextPacketSize() {
                Ok(0) => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
            if self
                .capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .is_err()
            {
                return false;
            }
            let samples = frames as usize * 2;
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                out.resize(out.len() + samples, 0.0);
            } else {
                out.extend_from_slice(std::slice::from_raw_parts(data.cast::<f32>(), samples));
            }
            if self.capture.ReleaseBuffer(frames).is_err() {
                return false;
            }
        }
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        // SAFETY: the client was initialized and is released right after.
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

/// Every source is asked for the same format; the audio engine converts.
fn format() -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
        nChannels: 2,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * 8,
        nBlockAlign: 8,
        wBitsPerSample: 32,
        cbSize: 0,
    }
}

unsafe fn initialize(
    client: IAudioClient,
    loopback: bool,
    kind: Kind,
) -> windows::core::Result<Source> {
    let format = format();
    let flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
        | if loopback {
            AUDCLNT_STREAMFLAGS_LOOPBACK
        } else {
            0
        };
    // One second of buffer covers a recorder thread that is busy encoding.
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, TICKS, 0, &format, None)?;
    let capture = client.GetService::<IAudioCaptureClient>()?;
    Ok(Source {
        client,
        capture,
        kind,
        lost: None,
    })
}

unsafe fn device(flow: EDataFlow, loopback: bool, kind: Kind) -> windows::core::Result<Source> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(flow, eConsole)?;
    initialize(
        device.Activate::<IAudioClient>(CLSCTX_ALL, None)?,
        loopback,
        kind,
    )
}

unsafe fn open_source(kind: Kind) -> windows::core::Result<Source> {
    match kind {
        Kind::Loopback(Loopback::System) => device(eRender, true, kind),
        Kind::Loopback(Loopback::Process(id)) => process(id, kind),
        Kind::Mic => device(eCapture, false, kind),
    }
}

/// Signals that `ActivateAudioInterfaceAsync` finished. The handler has to be
/// agile because the system calls it from one of its own threads.
#[implement(IActivateAudioInterfaceCompletionHandler, IAgileObject)]
struct Completion(Arc<(Mutex<bool>, Condvar)>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Completion_Impl {
    fn ActivateCompleted(
        &self,
        _: Ref<IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let (done, signal) = &*self.0;
        if let Ok(mut done) = done.lock() {
            *done = true;
        }
        signal.notify_all();
        Ok(())
    }
}

impl IAgileObject_Impl for Completion_Impl {}

unsafe fn process(id: u32, kind: Kind) -> windows::core::Result<Source> {
    let mut parameters = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: id,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    // Dropping a PROPVARIANT frees its blob, which here lives on the stack.
    let mut variant = ManuallyDrop::new(PROPVARIANT::default());
    let inner = &mut *variant.Anonymous.Anonymous;
    inner.vt = VT_BLOB;
    inner.Anonymous.blob = BLOB {
        cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
        pBlobData: (&mut parameters as *mut AUDIOCLIENT_ACTIVATION_PARAMS).cast(),
    };
    let state = Arc::new((Mutex::new(false), Condvar::new()));
    let handler: IActivateAudioInterfaceCompletionHandler = Completion(state.clone()).into();
    let operation = ActivateAudioInterfaceAsync(
        VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
        &IAudioClient::IID,
        Some(&*variant),
        &handler,
    )?;
    let (done, signal) = &*state;
    let finished = done
        .lock()
        .ok()
        .and_then(|done| {
            signal
                .wait_timeout_while(done, Duration::from_secs(3), |done| !*done)
                .ok()
        })
        .is_some_and(|(done, _)| *done);
    if !finished {
        return Err(windows::core::Error::from_hresult(HRESULT(
            0x8000_4005u32 as i32,
        )));
    }
    let mut result = HRESULT(0);
    let mut client = None;
    operation.GetActivateResult(&mut result, &mut client)?;
    result.ok()?;
    let client: IAudioClient = client.ok_or_else(windows::core::Error::empty)?.cast()?;
    initialize(client, true, kind)
}

pub struct AudioCapture {
    sources: Vec<Source>,
    mixer: Mixer,
    origin: Option<Instant>,
    scratch: Vec<f32>,
}

impl AudioCapture {
    /// Prepares the requested sources. A source that cannot start (no
    /// microphone, no permission, a process without audio) is left out without
    /// a message; `None` means nothing is left to record.
    pub unsafe fn open(audio: Audio) -> Option<AudioCapture> {
        let mut sources = Vec::new();
        if let Some(loopback) = audio.loopback {
            sources.extend(open_source(Kind::Loopback(loopback)).ok());
        }
        if audio.mic {
            sources.extend(open_source(Kind::Mic).ok());
        }
        (!sources.is_empty()).then(|| AudioCapture {
            mixer: Mixer::new(sources.len()),
            sources,
            origin: None,
            scratch: Vec::new(),
        })
    }

    /// Starts the devices; time zero of the audio is `origin`.
    pub unsafe fn start(&mut self, origin: Instant) {
        self.origin = Some(origin);
        for source in &self.sources {
            let _ = source.client.Start();
        }
    }

    /// The audio captured up to `now`, if enough time has passed to be worth writing.
    pub unsafe fn read(&mut self, now: Instant) -> Option<Chunk> {
        self.collect(now);
        self.mixer.take(now.saturating_duration_since(self.origin?))
    }

    /// The audio still held back at the end, so the recording does not lose its last moments.
    pub unsafe fn finish(&mut self, now: Instant) -> Option<Chunk> {
        self.collect(now);
        self.mixer
            .take(now.saturating_duration_since(self.origin?) + LATENCY_TIME)
    }

    /// Queues what every source captured. A device that disappeared (headphones
    /// unplugged, another default output) is opened again after a short wait and
    /// is silent until then.
    unsafe fn collect(&mut self, now: Instant) {
        for (index, source) in self.sources.iter_mut().enumerate() {
            self.scratch.clear();
            if source.read(&mut self.scratch) {
                self.mixer.push(index, &self.scratch);
                continue;
            }
            self.mixer.push(index, &self.scratch);
            match source.lost {
                None => source.lost = Some(now),
                Some(since) if now.saturating_duration_since(since) >= REOPEN_AFTER => {
                    source.lost = Some(now);
                    if let Ok(fresh) = open_source(source.kind) {
                        if fresh.client.Start().is_ok() {
                            *source = fresh;
                        }
                    }
                }
                Some(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests;
