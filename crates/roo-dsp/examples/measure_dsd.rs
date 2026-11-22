use std::f64::consts::PI;
use std::time::Instant;
use rustfft::{FftPlanner, num_complex::Complex};
use roo_dsp::{DsdModulator, DitherType, DSD_BLOCK_SIZE, fir::build_poly_ext2_filter};

fn blackman_harris_window(n: usize) -> Vec<f64> {
    let a0 = 0.35875;
    let a1 = 0.48829;
    let a2 = 0.14128;
    let a3 = 0.01168;
    (0..n)
        .map(|i| {
            let f = 2.0 * PI * i as f64 / (n - 1) as f64;
            a0 - a1 * f.cos() + a2 * (2.0 * f).cos() - a3 * (3.0 * f).cos()
        })
        .collect()
}

#[allow(dead_code)]
struct ModMetrics {
    order: u8,
    rate_name: &'static str,
    dsd_rate: f64,
    snr_db: f64,
    thd_pct: f64,
    thd_n_db: f64,
    inband_noise_db: f64,
    noise_20k_50k_db: f64,
    noise_50k_100k_db: f64,
    noise_above_100k_db: f64,
    ntf_slope_db_oct: f64,
    max_integrator_state: f64,
    throughput_msps: f64,
    execution_time_ms: f64,
    bit_density_pct: f64,
}

fn measure_modulator(order: u8, dsd_rate: f64, rate_name: &'static str) -> (ModMetrics, Vec<u8>) {
    let src_rate = 44100.0;
    let inv_ratio = src_rate / dsd_rate;
    let duration_sec = 2.0;
    let n_src = (src_rate * duration_sec) as usize;
    let freq = 1000.0;
    let amplitude = 0.501187; // -6 dBFS

    let left_pcm: Vec<f64> = (0..n_src)
        .map(|i| amplitude * (2.0 * PI * freq * i as f64 / src_rate).sin())
        .collect();
    let right_pcm = left_pcm.clone();

    let filter = build_poly_ext2_filter(dsd_rate, src_rate);
    let mut modulator = DsdModulator::new(order, "poly-ext2", DitherType::Gauss1, Some(filter));

    let samples_per_block = 32768usize;
    let total_dsd_samples = (n_src as f64 * (dsd_rate / src_rate)).round() as usize;
    let num_blocks = (total_dsd_samples + samples_per_block - 1) / samples_per_block;
    let mut out_bytes = vec![0u8; num_blocks * DSD_BLOCK_SIZE * 2];

    // Measure execution time
    let start_time = Instant::now();
    modulator.run_blocks(&left_pcm, &right_pcm, n_src, 0, num_blocks, inv_ratio, &mut out_bytes);
    let elapsed = start_time.elapsed();
    let elapsed_sec = elapsed.as_secs_f64();
    let throughput_msps = (total_dsd_samples as f64 / 1_000_000.0) / elapsed_sec;

    // Extract Left channel bitstream as +/- 1.0 samples
    let mut dsd_bits = Vec::with_capacity(total_dsd_samples);
    let mut ones_count = 0usize;
    for b in 0..num_blocks {
        let block_left_start = b * 2 * DSD_BLOCK_SIZE;
        for byte_idx in 0..DSD_BLOCK_SIZE {
            let byte_val = out_bytes[block_left_start + byte_idx];
            for bit_shift in (0..8).rev() {
                if (byte_val >> bit_shift) & 1 == 1 {
                    dsd_bits.push(1.0f64);
                    ones_count += 1;
                } else {
                    dsd_bits.push(-1.0f64);
                }
            }
        }
    }
    let bit_density_pct = 100.0 * (ones_count as f64) / dsd_bits.len() as f64;

    // FFT Spectral Analysis
    let fft_size = 131072usize.min(dsd_bits.len());
    // Skip startup transient (first 32768 samples)
    let start_idx = 32768;
    let window = blackman_harris_window(fft_size);
    let win_sum: f64 = window.iter().sum();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);

    let mut input_c: Vec<Complex<f64>> = (0..fft_size)
        .map(|i| Complex {
            re: dsd_bits[start_idx + i] * window[i],
            im: 0.0,
        })
        .collect();

    fft.process(&mut input_c);

    let mut power_spectrum = vec![0.0f64; fft_size / 2];
    for i in 0..(fft_size / 2) {
        let mag = (input_c[i].re.powi(2) + input_c[i].im.powi(2)).sqrt() / (win_sum / 2.0);
        let power = mag.powi(2);
        power_spectrum[i] = power;
    }

    let bin_hz = dsd_rate / fft_size as f64;
    let bin_for_hz = |hz: f64| -> usize { ((hz / bin_hz).round() as usize).min(fft_size / 2 - 1) };

    // Find fundamental peak around 1kHz
    let f0_bin_center = bin_for_hz(1000.0);
    let f0_search_span = 10;
    let mut peak_bin = f0_bin_center;
    let mut peak_val = power_spectrum[f0_bin_center];
    for b in (f0_bin_center.saturating_sub(f0_search_span))..=(f0_bin_center + f0_search_span) {
        if power_spectrum[b] > peak_val {
            peak_val = power_spectrum[b];
            peak_bin = b;
        }
    }

    // Integrate fundamental signal power (peak +/- 3 bins)
    let mut signal_power = 0.0;
    let peak_span = 3;
    for b in (peak_bin.saturating_sub(peak_span))..=(peak_bin + peak_span) {
        signal_power += power_spectrum[b];
    }

    // Harmonics (2kHz, 3kHz, 4kHz, 5kHz, up to 20kHz)
    let mut harmonic_power = 0.0;
    for h in 2..=20 {
        let h_bin = bin_for_hz(h as f64 * 1000.0);
        for b in (h_bin.saturating_sub(2))..=(h_bin + 2) {
            harmonic_power += power_spectrum[b];
        }
    }

    // In-band noise integration: 20 Hz to 20,000 Hz (excluding signal & harmonics)
    let bin_20hz = bin_for_hz(20.0);
    let bin_20khz = bin_for_hz(20000.0);
    let mut inband_noise_power = 0.0;
    for b in bin_20hz..=bin_20khz {
        let is_f0 = (b as isize - peak_bin as isize).abs() <= (peak_span as isize + 1);
        let mut is_harm = false;
        for h in 2..=20 {
            let h_bin = bin_for_hz(h as f64 * 1000.0);
            if (b as isize - h_bin as isize).abs() <= 3 {
                is_harm = true;
                break;
            }
        }
        if !is_f0 && !is_harm {
            inband_noise_power += power_spectrum[b];
        }
    }

    // Out-of-band noise bands
    let bin_50khz = bin_for_hz(50000.0);
    let bin_100khz = bin_for_hz(100000.0);
    let bin_nyquist = fft_size / 2 - 1;

    let mut noise_20k_50k_power = 0.0;
    for b in bin_20khz..bin_50khz {
        noise_20k_50k_power += power_spectrum[b];
    }

    let mut noise_50k_100k_power = 0.0;
    for b in bin_50khz..bin_100khz {
        noise_50k_100k_power += power_spectrum[b];
    }

    let mut noise_above_100k_power = 0.0;
    for b in bin_100khz..bin_nyquist {
        noise_above_100k_power += power_spectrum[b];
    }

    // NTF Slope: calculate slope of noise spectrum between 25kHz and 80kHz (dB per octave)
    let p_25k = power_spectrum[bin_for_hz(25000.0)].max(1e-25);
    let p_50k = power_spectrum[bin_for_hz(50000.0)].max(1e-25);
    let ntf_slope_db_oct = 10.0 * (p_50k / p_25k).log10();

    let snr_db = 10.0 * (signal_power / inband_noise_power.max(1e-25)).log10();
    let thd_pct = 100.0 * (harmonic_power / signal_power.max(1e-25)).sqrt();
    let thd_n_db = 10.0 * ((harmonic_power + inband_noise_power) / signal_power.max(1e-25)).log10();
    let inband_noise_db = 10.0 * (inband_noise_power / signal_power.max(1e-25)).log10();
    let noise_20k_50k_db = 10.0 * (noise_20k_50k_power / signal_power.max(1e-25)).log10();
    let noise_50k_100k_db = 10.0 * (noise_50k_100k_power / signal_power.max(1e-25)).log10();
    let noise_above_100k_db = 10.0 * (noise_above_100k_power / signal_power.max(1e-25)).log10();

    let max_integrator_state = if order == 7 {
        modulator.s1_l.abs().max(modulator.s2_l.abs())
            .max(modulator.s3_l.abs()).max(modulator.s4_l.abs())
            .max(modulator.s5_l.abs()).max(modulator.s6_l.abs())
            .max(modulator.s7_l.abs())
    } else {
        modulator.s1_l.abs().max(modulator.s2_l.abs())
            .max(modulator.s3_l.abs()).max(modulator.s4_l.abs())
            .max(modulator.s5_l.abs())
    };

    (
        ModMetrics {
            order,
            rate_name,
            dsd_rate,
            snr_db,
            thd_pct,
            thd_n_db,
            inband_noise_db,
            noise_20k_50k_db,
            noise_50k_100k_db,
            noise_above_100k_db,
            ntf_slope_db_oct,
            max_integrator_state,
            throughput_msps,
            execution_time_ms: elapsed_sec * 1000.0,
            bit_density_pct,
        },
        out_bytes,
    )
}

fn main() {
    println!("==========================================================================");
    println!("  rooPlayer: DSD 5-Order vs 7-Order Delta-Sigma Modulator Measurement Suite");
    println!("==========================================================================");

    let rates = [
        (2822400.0, "DSD64 (2.8224 MHz)"),
        (5644800.0, "DSD128 (5.6448 MHz)"),
        (11289600.0, "DSD256 (11.2896 MHz)"),
    ];

    for (rate, name) in rates {
        println!("\n>>> Testing Rate: {}", name);
        let (m5, bytes5) = measure_modulator(5, rate, name);
        let (m7, bytes7) = measure_modulator(7, rate, name);

        // Verification of bit difference (Hamming distance)
        let min_len = bytes5.len().min(bytes7.len());
        let mut diff_bytes = 0usize;
        let mut diff_bits = 0usize;
        for i in 0..min_len {
            let xor = bytes5[i] ^ bytes7[i];
            if xor != 0 {
                diff_bytes += 1;
                diff_bits += xor.count_ones() as usize;
            }
        }
        let total_bits = min_len * 8;
        let diff_ratio = 100.0 * (diff_bits as f64) / total_bits as f64;

        println!("--- Bitstream Difference Verification ---");
        println!("  Output Bytes Evaluated : {} bytes", min_len);
        println!("  Different Bytes Count  : {} / {} ({:.2}%)", diff_bytes, min_len, 100.0 * diff_bytes as f64 / min_len as f64);
        println!("  Hamming Bit Distance   : {} / {} ({:.2}% bit variance)", diff_bits, total_bits, diff_ratio);
        println!("  Modulator Active Check : {}", if diff_ratio > 30.0 { "CONFIRMED ACTIVE (Distinct Bitstreams Generated)" } else { "FAILED" });

        println!("\n--- Audio Band Metrics (20 Hz - 20,000 Hz, 1 kHz -6 dBFS Test Tone) ---");
        println!("  Parameter                      | DSD 5th-Order          | DSD 7th-Order");
        println!("  -------------------------------+------------------------+------------------------");
        println!("  In-band SNR                    | {:>18.2} dB | {:>18.2} dB", m5.snr_db, m7.snr_db);
        println!("  In-band THD                    | {:>17.5} % | {:>17.5} %", m5.thd_pct, m7.thd_pct);
        println!("  In-band THD+N                  | {:>18.2} dB | {:>18.2} dB", m5.thd_n_db, m7.thd_n_db);
        println!("  In-band Integrated Noise Floor | {:>18.2} dB | {:>18.2} dB", m5.inband_noise_db, m7.inband_noise_db);

        println!("\n--- Noise Shaping & Ultrasonic Behavior (Quantization Noise Transfer) ---");
        println!("  Parameter                      | DSD 5th-Order          | DSD 7th-Order");
        println!("  -------------------------------+------------------------+------------------------");
        println!("  NTF Noise Slope (25k->50kHz)   | {:>15.1} dB/oct | {:>15.1} dB/oct", m5.ntf_slope_db_oct, m7.ntf_slope_db_oct);
        println!("  Ultrasonic Noise (20k - 50kHz) | {:>18.2} dB | {:>18.2} dB", m5.noise_20k_50k_db, m7.noise_20k_50k_db);
        println!("  Ultrasonic Noise (50k - 100kHz)| {:>18.2} dB | {:>18.2} dB", m5.noise_50k_100k_db, m7.noise_50k_100k_db);
        println!("  High-Freq Noise (>100kHz)      | {:>18.2} dB | {:>18.2} dB", m5.noise_above_100k_db, m7.noise_above_100k_db);

        println!("\n--- DSP Computational Complexity & Stability ---");
        println!("  Parameter                      | DSD 5th-Order          | DSD 7th-Order");
        println!("  -------------------------------+------------------------+------------------------");
        println!("  Max Integrator Internal State  | {:>22.4} | {:>22.4}", m5.max_integrator_state, m7.max_integrator_state);
        println!("  Processing Throughput          | {:>17.2} MS/s | {:>17.2} MS/s", m5.throughput_msps, m7.throughput_msps);
        println!("  2.0s Audio Modulation Time     | {:>19.2} ms | {:>19.2} ms", m5.execution_time_ms, m7.execution_time_ms);
        println!("  Bit Density (1s vs 0s)         | {:>19.2} % | {:>19.2} %", m5.bit_density_pct, m7.bit_density_pct);
    }
}
