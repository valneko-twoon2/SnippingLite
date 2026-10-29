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
fn finishing_flushes_the_held_back_tail() {
    let mut mixer = Mixer::new(1);
    let stop = Duration::from_millis(1000);
    mixer.push(0, &vec![0.25; 2 * RATE as usize]);
    let early = mixer.take(stop).unwrap();
    let tail = mixer.take(stop + Duration::from_millis(50)).unwrap();
    // Everything up to the stop is written once the latency is released.
    let frames = (early.pcm.len() + tail.pcm.len()) / 4;
    assert_eq!(frames, RATE as usize);
    assert_eq!(tail.time, early.duration);
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
