use super::audio::RATE;
use std::{path::Path, ptr::null_mut};
use windows::{core::HSTRING, Win32::Media::MediaFoundation::*};

pub(super) struct Writer {
    pub(super) sink: IMFSinkWriter,
    pub(super) stream: u32,
    /// The AAC stream; `None` when the recording has no audio.
    pub(super) audio: Option<u32>,
}

pub(super) fn video_type(
    subtype: &windows::core::GUID,
    width: i32,
    height: i32,
    fps: u32,
) -> windows::core::Result<IMFMediaType> {
    // SAFETY: plain Media Foundation calls on objects created here.
    unsafe {
        let media = MFCreateMediaType()?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        media.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        media.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
        media.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
        media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1 << 32) | 1)?;
        Ok(media)
    }
}

/// An audio stream needs the AAC encoder, which some Windows editions lack; the
/// recording then goes on without sound.
pub(super) fn open(
    path: &Path,
    width: i32,
    height: i32,
    fps: u32,
    audio: bool,
) -> windows::core::Result<Writer> {
    match open_with(path, width, height, fps, audio) {
        Err(_) if audio => open_with(path, width, height, fps, false),
        result => result,
    }
}

/// 16-bit stereo audio at `RATE`: raw PCM, or AAC at 160 kbit/s.
pub(super) fn audio_type(aac: bool) -> windows::core::Result<IMFMediaType> {
    // SAFETY: plain Media Foundation calls on objects created here.
    unsafe {
        let media = MFCreateMediaType()?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        media.SetGUID(
            &MF_MT_SUBTYPE,
            if aac {
                &MFAudioFormat_AAC
            } else {
                &MFAudioFormat_PCM
            },
        )?;
        media.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
        media.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE)?;
        media.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        if aac {
            media.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 20_000)?;
            media.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
            media.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
        } else {
            media.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
            media.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, RATE * 4)?;
        }
        Ok(media)
    }
}

pub(super) fn open_with(
    path: &Path,
    width: i32,
    height: i32,
    fps: u32,
    audio: bool,
) -> windows::core::Result<Writer> {
    // SAFETY: plain Media Foundation calls on objects created here.
    unsafe {
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 2)?;
        let attributes = attributes.ok_or_else(windows::core::Error::empty)?;
        attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
        // The capture loop already owns frame pacing; avoid a second wait here.
        attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
        let url = HSTRING::from(path.to_string_lossy().as_ref());
        let sink = MFCreateSinkWriterFromURL(&url, None, &attributes)?;
        let output = video_type(&MFVideoFormat_H264, width, height, fps)?;
        let bitrate = (width as f64 * height as f64 * fps as f64 * 0.1)
            .clamp(2e6, 20e6 * f64::from(fps) / 30.0) as u32;
        output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
        let stream = sink.AddStream(&output)?;
        let input = video_type(&MFVideoFormat_RGB32, width, height, fps)?;
        // A positive stride marks the rows as top-down, like the DIB section.
        input.SetUINT32(&MF_MT_DEFAULT_STRIDE, (width * 4) as u32)?;
        sink.SetInputMediaType(stream, &input, None)?;
        let audio = if audio {
            let audio_stream = sink.AddStream(&audio_type(true)?)?;
            sink.SetInputMediaType(audio_stream, &audio_type(false)?, None)?;
            Some(audio_stream)
        } else {
            None
        };
        sink.BeginWriting()?;
        Ok(Writer {
            sink,
            stream,
            audio,
        })
    }
}

pub(super) fn write(
    writer: &Writer,
    pixels: &[u8],
    time: i64,
    duration: i64,
) -> windows::core::Result<()> {
    write_to(writer, writer.stream, pixels, time, duration)
}

pub(super) fn write_to(
    writer: &Writer,
    stream: u32,
    data: &[u8],
    time: i64,
    duration: i64,
) -> windows::core::Result<()> {
    // SAFETY: the buffer is locked while it is filled and holds `data.len()` bytes.
    unsafe {
        let pixels = data;
        let buffer = MFCreateMemoryBuffer(pixels.len() as u32)?;
        let mut data = null_mut();
        buffer.Lock(&mut data, None, None)?;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), data, pixels.len());
        buffer.Unlock()?;
        buffer.SetCurrentLength(pixels.len() as u32)?;
        let sample = MFCreateSample()?;
        sample.AddBuffer(&buffer)?;
        sample.SetSampleTime(time)?;
        sample.SetSampleDuration(duration)?;
        writer.sink.WriteSample(stream, &sample)
    }
}
