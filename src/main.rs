use portaudio as pa;
use sample::{signal, Frame, ring_buffer, Sample, Signal, ToFrameSliceMut};

const FRAMES_PER_BUFFER: u32 = 512;
const NUM_CHANNELS: i32 = 1;
const SAMPLE_RATE: f64 = 44_100.0;

fn main() {
    run().unwrap();
}

fn run() -> Result<(), pa::Error> {
    let hz = signal::rate(SAMPLE_RATE).const_hz(440.0);
    let half_sec = (SAMPLE_RATE / 4.0) as usize;

    let input = signal::from_iter(hz.clone()
        .sine()
        .mul_amp(signal::gen(|| unsafe { static mut VAL: f64 = 1.0; VAL *= 0.9999; [VAL] }))
        .take(half_sec)
        .chain(signal::equilibrium().take(half_sec))
        .chain(hz.clone().saw().mul_amp(signal::gen(|| unsafe { static mut VAL: f64 = 1.0; VAL *= 0.9999; [VAL] })).take(half_sec))
        .chain(signal::equilibrium().take(half_sec))
        .chain(hz.clone().square().mul_amp(signal::gen(|| unsafe { static mut VAL: f64 = 1.0; VAL *= 0.9999; [VAL] })).take(half_sec))
        .chain(signal::equilibrium().take(half_sec))
        .chain(hz.clone().noise_simplex().mul_amp(signal::gen(|| unsafe { static mut VAL: f64 = 1.0; VAL *= 0.9999; [VAL] })).take(half_sec))
        .chain(signal::equilibrium().take(half_sec))
        .chain(signal::noise(0).mul_amp(signal::gen(|| unsafe { static mut VAL: f64 = 1.0; VAL *= 0.9999; [VAL] })).take(half_sec))
        .chain(signal::equilibrium().take(half_sec)));

    let delayed = input.clone().delay(4_410 * 2);

    let ring_buffer = ring_buffer::Fixed::from([[0.0]; SAMPLE_RATE]);
    let fbdelay = FeedbackDelay::new(ring_buffer);

    let mut output = delayed.scale_amp(0.5).add_amp(input).until_exhausted().map(|f| f.map(|s| s.to_sample::<f32>()));

    let pa = pa::PortAudio::new()?;
    let settings = pa.default_output_stream_settings::<f32>(
            NUM_CHANNELS,
            SAMPLE_RATE,
            FRAMES_PER_BUFFER,
    )?;
    
    let callback = move |pa::OutputStreamCallbackArgs { buffer, ..}| {
        let buffer: &mut [[f32; 1]] = buffer.to_frame_slice_mut().unwrap();
        for out_frame in buffer {
            match output.next() {
                Some(frame) => *out_frame = frame,
                None => return pa::Complete,
            }
        }
        pa::Continue
    };

    let mut stream = pa.open_non_blocking_stream(settings, callback)?;
    stream.start()?;

    while let Ok(true) = stream.is_active() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    stream.stop()?;
    stream.close()?;

    Ok(())
}

pub struct FeedbackDelay<S> {
    frames: ring_buffer::Fixed<S>,
    feedback: f64,
    wet: f64,
    dry: f64,
    delay_frames: usize,
}

impl<S> FeedbackDelay<S> {
    fn new(signal: dyn Signal<S>, feedback: f64, mix: f64, delay: usize, buf: ring_buffer::Fixed<S>) -> FeedbackDelay<S> {
        assert!(mix <= 1.0);
        assert!(feedback <= 1.0);
        assert!(feedback >= 0.0);

        FeedbackDelay {
            frames: buf,
            feedback: feedback,
            wet: mix,
            dry: 1.0 - mix,
            delay_frames: delay,
        }
    }
}

impl<S> Signal for FeedbackDelay<S> {
    fn next(&mut self) -> Self::Frame {
        let f = self.next().unwrap_or_default([0.0]);
        let wet_frame = self.frames[-self.delay_frames].scale_amp(self.feedback);
        let save_frame = f.add_amp(wet_frame);
        let _ = self.frames.push(save_frame);
        f.scale_amp(self.dry).add_amp(wet_frame.scale_amp(self.wet))
    }
}
