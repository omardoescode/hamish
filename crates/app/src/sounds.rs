//! Cycle sounds and the Variant delete sound. Core decides which
//! sound plays and makes its samples; a [`SoundSink`] plays them.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample, Stream, StreamConfig};
use gpui::{App, Global};
use hamish_core::{Sound, Toggle};

use crate::settings;

/// Plays sounds. It returns at once; the sound plays off the UI thread.
pub trait SoundSink: Send + Sync {
    fn play(&self, sound: Sound);
}

/// The sound sink the app plays to. `main` sets the audio device; tests set a fake.
/// With none set, no sound plays.
pub struct Sounds(pub Arc<dyn SoundSink>);

impl Global for Sounds {}

/// Plays `sound` when the "Cycle sounds" setting is on. A Variant delete plays
/// [`Sound::Explosion`].
pub(crate) fn play(sound: Sound, cx: &App) {
    if !settings::is_on(cx, Toggle::CycleSounds) {
        return;
    }
    if let Some(Sounds(sink)) = cx.try_global::<Sounds>() {
        sink.play(sound);
    }
}

/// The default audio output device, on its own thread. The device opens at the first
/// sound. With no device, or one that does not open, the sounds are silent.
pub struct DeviceSounds(Sender<Sound>);

impl DeviceSounds {
    #[must_use]
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::channel();
        let started = thread::Builder::new()
            .name("hamish-sounds".to_owned())
            .spawn(move || play_on_device(&receiver));
        drop(started);
        Self(sender)
    }
}

impl SoundSink for DeviceSounds {
    fn play(&self, sound: Sound) {
        self.0.send(sound).ok();
    }
}

fn play_on_device(receiver: &Receiver<Sound>) {
    let Ok(first) = receiver.recv() else {
        return;
    };
    let Some(output) = Output::open() else {
        while receiver.recv().is_ok() {}
        return;
    };
    output.play(first);
    while let Ok(sound) = receiver.recv() {
        output.play(sound);
    }
}

/// An open output stream. It plays the samples in `queue`, and silence when it is empty.
struct Output {
    _stream: Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
}

impl Output {
    fn open() -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let config = device.default_output_config().ok()?.config();
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let stream = build::<f32>(&device, config, &queue)
            .or_else(|| build::<i16>(&device, config, &queue))?;
        stream.play().ok()?;
        Some(Self {
            _stream: stream,
            queue,
            sample_rate: config.sample_rate,
        })
    }

    /// A new sound stops the one that plays, so quick cycling does not queue up tones.
    fn play(&self, sound: Sound) {
        let samples = sound.samples(self.sample_rate);
        if let Ok(mut queue) = self.queue.lock() {
            queue.clear();
            queue.extend(samples);
        }
    }
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: StreamConfig,
    queue: &Arc<Mutex<VecDeque<f32>>>,
) -> Option<Stream> {
    let queue = Arc::clone(queue);
    let channels = usize::from(config.channels).max(1);
    device
        .build_output_stream(
            config,
            move |frames: &mut [T], _: &cpal::OutputCallbackInfo| {
                let Ok(mut queue) = queue.lock() else {
                    return;
                };
                for frame in frames.chunks_mut(channels) {
                    let sample = T::from_sample(queue.pop_front().unwrap_or(0.));
                    frame.fill(sample);
                }
            },
            |_| {},
            None,
        )
        .ok()
}
