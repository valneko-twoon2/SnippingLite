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

struct Source {
    client: IAudioClient,
    capture: IAudioCaptureClient,
}

impl Source {
    /// Moves everything captured so far into `out`.
    unsafe fn read(&self, out: &mut Vec<f32>) {
        while self.capture.GetNextPacketSize().is_ok_and(|size| size > 0) {
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
            if self
                .capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .is_err()
            {
                return;
            }
            let samples = frames as usize * 2;
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                out.resize(out.len() + samples, 0.0);
            } else {
                out.extend_from_slice(std::slice::from_raw_parts(data.cast::<f32>(), samples));
            }
            if self.capture.ReleaseBuffer(frames).is_err() {
                return;
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

unsafe fn initialize(client: IAudioClient, loopback: bool) -> windows::core::Result<Source> {
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
    Ok(Source { client, capture })
}

unsafe fn device(flow: EDataFlow, loopback: bool) -> windows::core::Result<Source> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(flow, eConsole)?;
    initialize(device.Activate::<IAudioClient>(CLSCTX_ALL, None)?, loopback)
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

unsafe fn process(id: u32) -> windows::core::Result<Source> {
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
    initialize(client, true)
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
        match audio.loopback {
            Some(Loopback::System) => sources.extend(device(eRender, true).ok()),
            Some(Loopback::Process(id)) => sources.extend(process(id).ok()),
            None => {}
        }
        if audio.mic {
            sources.extend(device(eCapture, false).ok());
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
        let origin = self.origin?;
        for (index, source) in self.sources.iter().enumerate() {
            self.scratch.clear();
            source.read(&mut self.scratch);
            self.mixer.push(index, &self.scratch);
        }
        self.mixer.take(now.saturating_duration_since(origin))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(chunk: &Chunk) -> Vec<i16> {
        chunk
            .pcm
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect()
    }

    #[test]
    fn mixer_adds_sources_and_fills_silence() {
        let mut mixer = Mixer::new(2);
        assert!(mixer.take(Duration::from_millis(40)).is_none());
        let second = Duration::from_secs(1);
        // The first source plays only for a quarter of the time due.
        mixer.push(0, &vec![0.25; 2 * 6000]);
        mixer.push(1, &vec![0.5; 2 * 3000]);
        let chunk = mixer.take(second).expect("Audio is due");
        let frames = RATE as usize - LATENCY;
        assert_eq!(chunk.pcm.len(), frames * 4);
        assert_eq!(chunk.time, 0);
        assert_eq!(chunk.duration, (frames as i64) * TICKS / i64::from(RATE));
        let pcm = samples(&chunk);
        assert_eq!(pcm[0], (0.75 * 32767.0) as i16);
        assert_eq!(pcm[2 * 3000], (0.25 * 32767.0) as i16);
        assert_eq!(pcm[2 * 6000], 0);
        // The next chunk continues the timeline exactly where this one ended.
        let next = mixer.take(second + Duration::from_millis(100)).unwrap();
        assert_eq!(next.time, chunk.duration);
    }

    /// The moment `frames` frames are due, rounded up past the floating point error.
    fn at(frames: usize) -> Duration {
        Duration::from_nanos((frames as u64 * 1_000_000_000).div_ceil(u64::from(RATE)) + 1)
    }

    #[test]
    fn mixer_holds_back_exactly_the_latency() {
        let mut mixer = Mixer::new(1);
        assert!(mixer.take(Duration::ZERO).is_none());
        assert!(mixer.take(at(LATENCY)).is_none());
        let chunk = mixer.take(at(LATENCY + 1)).expect("One frame is due");
        assert_eq!(chunk.pcm.len(), 4);
        assert_eq!(chunk.time, 0);
        // Nothing new is due at the same moment.
        assert!(mixer.take(at(LATENCY + 1)).is_none());
    }

    #[test]
    fn mixer_keeps_a_continuous_timeline_over_many_reads() {
        let mut mixer = Mixer::new(1);
        let (mut elapsed, mut pushed, mut emitted) = (0usize, 0usize, 0usize);
        let mut output = Vec::new();
        for i in 0..2000usize {
            // Irregular read intervals from one frame to about 40 ms.
            elapsed += 1 + (i * 7919) % 1900;
            // A source on time: every frame up to now, as a ramp to check the order.
            let ramp: Vec<f32> = (pushed..elapsed)
                .flat_map(|n| {
                    let value = (n % 1000) as f32 / 2000.0;
                    [value, -value]
                })
                .collect();
            mixer.push(0, &ramp);
            pushed = elapsed;
            if let Some(chunk) = mixer.take(at(elapsed)) {
                let frames = chunk.pcm.len() / 4;
                assert_eq!(chunk.time, emitted as i64 * TICKS / i64::from(RATE));
                emitted += frames;
                output.extend(samples(&chunk));
            }
        }
        assert_eq!(emitted, elapsed - LATENCY);
        // Nothing was dropped, repeated or reordered.
        for (n, pair) in output.as_chunks::<2>().0.iter().enumerate() {
            let value = ((n % 1000) as f32 / 2000.0 * 32767.0) as i16;
            assert_eq!(*pair, [value, -value], "frame {n}");
        }
    }

    #[test]
    fn mixer_bounds_the_backlog_of_a_fast_source_over_time() {
        let mut mixer = Mixer::new(2);
        let mut emitted = 0;
        for i in 1..=500usize {
            // The first source delivers 50% too much, the second nothing at all.
            mixer.push(0, &vec![0.1; 2 * 720]);
            if let Some(chunk) = mixer.take(at(i * 480 + LATENCY)) {
                emitted += chunk.pcm.len() / 4;
            }
            assert!(mixer.queues[0].len() <= LATENCY * 2 * 8);
            assert!(mixer.queues[1].is_empty());
        }
        // The file timeline follows the clock, not the amount of sound delivered.
        assert_eq!(emitted, 500 * 480);
    }

    #[test]
    fn mixer_tolerates_a_clock_going_back_and_no_sources() {
        let mut mixer = Mixer::new(1);
        let first = mixer.take(Duration::from_secs(1)).unwrap();
        assert!(mixer.take(Duration::from_millis(500)).is_none());
        let next = mixer.take(Duration::from_secs(2)).unwrap();
        assert_eq!(
            next.time,
            (first.pcm.len() / 4) as i64 * TICKS / i64::from(RATE)
        );
        // No sources, or empty deliveries, still give silence on the timeline.
        let mut silent = Mixer::new(0);
        let chunk = silent.take(Duration::from_secs(1)).unwrap();
        assert!(samples(&chunk).iter().all(|&s| s == 0));
        let mut empty = Mixer::new(1);
        empty.push(0, &[]);
        assert!(samples(&empty.take(Duration::from_secs(1)).unwrap())
            .iter()
            .all(|&s| s == 0));
    }

    #[test]
    fn mixer_clips_and_drops_a_backlog() {
        let mut mixer = Mixer::new(2);
        mixer.push(0, &[0.9, -0.9].repeat(LATENCY * 30));
        mixer.push(1, &[0.9, -0.9]);
        let chunk = mixer.take(Duration::from_secs(1)).unwrap();
        let pcm = samples(&chunk);
        assert_eq!(pcm[0], 32767);
        assert_eq!(pcm[1], -32767);
        assert!(mixer.queues[0].len() <= LATENCY * 2 * 2);
    }
}
