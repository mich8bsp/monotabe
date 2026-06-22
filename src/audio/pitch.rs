use std::f32::consts::PI;

use rustfft::{num_complex::Complex, FftPlanner};

/// Pitch-shift interleaved f32 samples by `semitones` half-steps, preserving tempo.
///
/// Uses a phase vocoder: frequency-domain bin remapping with phase accumulation,
/// which avoids the phasing/flanging artifacts of basic OLA.
pub fn pitch_shift(samples: &[f32], channels: u16, semitones: i32) -> Vec<f32> {
    if semitones == 0 || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = 2f32.powf(semitones as f32 / 12.0);
    let n_ch = channels as usize;

    let processed: Vec<Vec<f32>> = (0..n_ch)
        .map(|c| {
            let ch: Vec<f32> = samples.iter().skip(c).step_by(n_ch).copied().collect();
            shift_mono(&ch, ratio)
        })
        .collect();

    let out_len = processed.iter().map(|ch| ch.len()).min().unwrap_or(0);
    let mut output = vec![0f32; out_len * n_ch];
    for (c, ch) in processed.iter().enumerate() {
        for (i, &s) in ch.iter().take(out_len).enumerate() {
            output[i * n_ch + c] = s.clamp(-1.0, 1.0);
        }
    }
    output
}

fn shift_mono(input: &[f32], ratio: f32) -> Vec<f32> {
    const WIN: usize = 4096;
    const HOP: usize = WIN / 8; // 512 — 8× overlap for quality
    let n_bins = WIN / 2 + 1;
    let n = input.len();

    if n < WIN {
        return input.to_vec();
    }

    let hann: Vec<f32> = (0..WIN)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / (WIN - 1) as f32).cos())
        .collect();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(WIN);
    let ifft = planner.plan_fft_inverse(WIN);

    let mut prev_phase = vec![0.0f32; n_bins];
    let mut syn_phase = vec![0.0f32; n_bins];
    let mut buf: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); WIN];
    let mut synth: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); WIN];

    let n_frames = (n - WIN) / HOP + 1;
    let out_len = (n_frames - 1) * HOP + WIN;
    let mut output = vec![0f32; out_len];
    let mut weight = vec![0f32; out_len];

    for frame in 0..n_frames {
        let src = frame * HOP;

        for i in 0..WIN {
            buf[i].re = if src + i < n { input[src + i] * hann[i] } else { 0.0 };
            buf[i].im = 0.0;
        }
        fft.process(&mut buf);

        // Compute instantaneous frequency for each analysis bin.
        let mut inst_f = vec![0.0f32; n_bins];
        for k in 0..n_bins {
            let phi = buf[k].arg();
            let expected = prev_phase[k] + 2.0 * PI * k as f32 * HOP as f32 / WIN as f32;
            inst_f[k] = 2.0 * PI * k as f32 / WIN as f32 + wrap(phi - expected) / HOP as f32;
            prev_phase[k] = phi;
        }

        // Build pitch-shifted synthesis spectrum.
        // Output bin j draws from source bin k = j / ratio (fractional).
        for s in synth.iter_mut() {
            s.re = 0.0;
            s.im = 0.0;
        }
        for j in 0..n_bins {
            let kf = j as f32 / ratio;
            let k0 = kf as usize;
            if k0 + 1 >= n_bins {
                break;
            }
            let t = kf - k0 as f32;
            let mag = buf[k0].norm() * (1.0 - t) + buf[k0 + 1].norm() * t;
            let ifreq = inst_f[k0] * (1.0 - t) + inst_f[k0 + 1] * t;
            // Advance synthesis phase at the output bin's expected rate.
            syn_phase[j] += ifreq * ratio * HOP as f32;
            synth[j] = Complex::from_polar(mag, syn_phase[j]);
        }

        // Hermitian symmetry so IFFT produces a real signal.
        for k in 1..WIN / 2 {
            synth[WIN - k] = synth[k].conj();
        }

        ifft.process(&mut synth);

        let scale = 1.0 / WIN as f32;
        let dst = frame * HOP;
        for i in 0..WIN {
            if dst + i < out_len {
                let w = hann[i];
                output[dst + i] += synth[i].re * scale * w;
                weight[dst + i] += w * w;
            }
        }
    }

    for i in 0..out_len {
        if weight[i] > 1e-10 {
            output[i] /= weight[i];
        }
    }

    output.truncate(n);
    output
}

fn wrap(p: f32) -> f32 {
    let two_pi = 2.0 * PI;
    p - two_pi * (p / two_pi).round()
}
