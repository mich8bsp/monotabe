use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};

pub struct AudioPlayer {
    _sink: MixerDeviceSink,
    player: Option<Player>,
    pub path: Option<String>,
    pub duration: Option<Duration>,
}

impl AudioPlayer {
    pub fn try_new() -> Option<Self> {
        let sink = DeviceSinkBuilder::open_default_sink().ok()?;
        Some(Self {
            _sink: sink,
            player: None,
            path: None,
            duration: None,
        })
    }

    pub fn load(&mut self, path: String, semitones: i32) -> Result<(), String> {
        self.player = None;
        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;

        if semitones == 0 {
            // Seekable path — original behaviour.
            let source = Decoder::try_from(file).map_err(|e| e.to_string())?;
            let total = source.total_duration();
            let player = Player::connect_new(self._sink.mixer());
            player.append(source);
            player.pause();
            self.player = Some(player);
            self.path = Some(path);
            self.duration = total;
        } else {
            // Pitch-shifted: decode all samples, process, play from buffer.
            let source = Decoder::try_from(file).map_err(|e| e.to_string())?;
            let sample_rate = source.sample_rate(); // NonZero<u32>
            let channels = source.channels();       // NonZero<u16>
            let raw: Vec<f32> = source.collect();   // Item = f32 in rodio 0.22
            let shifted = crate::audio::pitch::pitch_shift(&raw, channels.get(), semitones);
            let total = Some(Duration::from_secs_f64(
                shifted.len() as f64 / (sample_rate.get() as f64 * channels.get() as f64),
            ));
            let buffer = rodio::buffer::SamplesBuffer::new(channels, sample_rate, shifted);
            let player = Player::connect_new(self._sink.mixer());
            player.append(buffer);
            player.pause();
            self.player = Some(player);
            self.path = Some(path);
            self.duration = total;
        }

        Ok(())
    }

    pub fn play(&mut self) {
        if let Some(p) = &self.player {
            p.play();
        }
    }

    pub fn pause(&mut self) {
        if let Some(p) = &self.player {
            p.pause();
        }
    }

    pub fn stop(&mut self) {
        self.player = None;
        self.duration = None;
        self.path = None;
    }

    pub fn seek(&mut self, pos: Duration) {
        if let Some(p) = &self.player {
            let _ = p.try_seek(pos);
        }
    }

    pub fn toggle(&mut self) {
        if self.is_playing() { self.pause() } else { self.play() }
    }

    pub fn is_playing(&self) -> bool {
        self.player.as_ref().map(|p| !p.is_paused() && !p.empty()).unwrap_or(false)
    }

    pub fn is_loaded(&self) -> bool {
        self.player.is_some()
    }

    pub fn position(&self) -> Duration {
        self.player.as_ref().map(|p| p.get_pos()).unwrap_or(Duration::ZERO)
    }

    pub fn has_finished(&self) -> bool {
        self.player.as_ref().map(|p| p.empty()).unwrap_or(false)
    }
}
