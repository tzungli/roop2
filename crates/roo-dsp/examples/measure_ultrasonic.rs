use std::f64::consts::PI;
use roo_dsp::{build_ultrasonic_filter_coeffs, apply_ultrasonic_filter, UltrasonicFilterCoeffs};

fn measure_gain_at_freq(coeffs: &UltrasonicFilterCoeffs, sample_rate: f64, test_freq_hz: f64) -> f64 {
    // Generate 1000 cycles or at least 16384 samples
    let num_samples = 32768usize;
    let mut left: Vec<f64> = (0..num_samples)
        .map(|i| (2.0 * PI * test_freq_hz * i as f64 / sample_rate).sin())
        .collect();
    let mut right = left.clone();

    apply_ultrasonic_filter(coeffs, &mut left, &mut right);

    // Measure steady-state amplitude over last 8192 samples (skip initial transient)
    let steady_start = num_samples - 8192;
    let mut peak = 0.0f64;
    let mut rms_sum = 0.0f64;
    for i in steady_start..num_samples {
        let val = left[i].abs();
        if val > peak {
            peak = val;
        }
        rms_sum += left[i].powi(2);
    }
    let rms = (rms_sum / 8192.0).sqrt();
    let amplitude = rms * std::f64::consts::SQRT_2; // Effective sine amplitude
    20.0 * amplitude.max(1e-12).log10()
}

fn main() {
    println!("==========================================================================");
    println!("  rooPlayer: Ultrasonic Protection Filter Verification & Measurement Suite");
    println!("==========================================================================");

    let sample_rate = 192000.0;
    println!("\n>>> 1. Frequency Response Sweep at Sample Rate = {} Hz", sample_rate);

    let modes = ["20k", "30k", "40k", "50k"];
    let test_frequencies = [
        1000.0,
        5000.0,
        10000.0,
        15000.0,
        20000.0,
        25000.0,
        30000.0,
        35000.0,
        40000.0,
        45000.0,
        50000.0,
        60000.0,
        70000.0,
        80000.0,
        90000.0,
    ];

    println!("\n  Frequency (Hz) |    20k Cutoff    |    30k Cutoff    |    40k Cutoff    |    50k Cutoff");
    println!("  ---------------+------------------+------------------+------------------+------------------");

    for &freq in &test_frequencies {
        print!("  {:>14.0} |", freq);
        for &mode in &modes {
            if let Some(coeffs) = build_ultrasonic_filter_coeffs(sample_rate, mode) {
                let gain_db = measure_gain_at_freq(&coeffs, sample_rate, freq);
                print!(" {:>14.2} dB |", gain_db);
            } else {
                print!(" {:>17} |", "N/A");
            }
        }
        println!();
    }

    println!("\n>>> 2. Cutoff Accuracy & -3.01 dB Verification");
    for &mode in &modes {
        let target_fc = match mode {
            "20k" => 20000.0,
            "30k" => 30000.0,
            "40k" => 40000.0,
            "50k" => 50000.0,
            _ => 30000.0,
        };
        let coeffs = build_ultrasonic_filter_coeffs(sample_rate, mode).unwrap();
        let gain_at_fc = measure_gain_at_freq(&coeffs, sample_rate, target_fc);
        let error = (gain_at_fc - (-3.0103)).abs();
        println!("  - Mode {:<4} | Target fc = {:>5.0} Hz | Measured Gain = {:>6.2} dB | Error from -3.01 dB = {:.4} dB -> {}",
            mode, target_fc, gain_at_fc, error,
            if error < 0.15 { "PASS (Butterworth Q=0.7071 verified)" } else { "FAIL" }
        );
    }

    println!("\n>>> 3. Stopband Roll-off Slope Verification (Attenuation per Octave)");
    for &mode in &modes {
        let (f1, f2) = match mode {
            "20k" => (30000.0, 60000.0),
            "30k" => (40000.0, 80000.0),
            "40k" => (45000.0, 90000.0),
            "50k" => (45000.0, 90000.0),
            _ => (40000.0, 80000.0),
        };
        let coeffs = build_ultrasonic_filter_coeffs(sample_rate, mode).unwrap();
        let g1 = measure_gain_at_freq(&coeffs, sample_rate, f1);
        let g2 = measure_gain_at_freq(&coeffs, sample_rate, f2);
        let slope = (g2 - g1) / (f2 / f1).log2();
        println!("  - Mode {:<4} | Measured Stopband Slope = {:>6.1} dB/octave (Theoretical: -12.0 dB/oct) -> {}",
            mode, slope,
            if slope < -10.0 && slope > -14.0 { "PASS (2nd-Order Standard Roll-off)" } else { "CHECK" }
        );
    }

    println!("\n>>> 4. Nyquist Safety Guard Checks Across Sample Rates");
    let test_rates = [44100.0, 48000.0, 88200.0, 96000.0, 176400.0, 192000.0, 352800.0, 384000.0];
    println!("  Sample Rate |    20k Mode    |    30k Mode    |    40k Mode    |    50k Mode");
    println!("  ------------+----------------+----------------+----------------+----------------");
    for &sr in &test_rates {
        print!("  {:>11.0} |", sr);
        for &mode in &modes {
            let res = build_ultrasonic_filter_coeffs(sr, mode);
            let status = if res.is_some() { "ACTIVE" } else { "BYPASS (Guard)" };
            print!(" {:>14} |", status);
        }
        println!();
    }

    println!("\n>>> 5. Ultrasonic Noise Attenuation Test on Mixed Signal (1kHz Audio + 60kHz RF Noise)");
    let n = 32768usize;
    let sr = 192000.0;
    // Input: 1kHz signal (0 dBFS) + 60kHz ultrasonic noise (-10 dBFS, 0.316)
    let orig_sig: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / sr;
            (2.0 * PI * 1000.0 * t).sin() + 0.316 * (2.0 * PI * 60000.0 * t).sin()
        })
        .collect();

    for &mode in &["none", "20k", "30k", "40k", "50k"] {
        let mut test_sig = orig_sig.clone();
        let mut right = orig_sig.clone();
        if mode != "none" {
            let coeffs = build_ultrasonic_filter_coeffs(sr, mode).unwrap();
            apply_ultrasonic_filter(&coeffs, &mut test_sig, &mut right);
        }

        // Measure residual 60kHz component by correlation/DFT at 60kHz
        let mut sum_re = 0.0;
        let mut sum_im = 0.0;
        let steady_len = 16384;
        let steady_offset = n - steady_len;
        for i in 0..steady_len {
            let t = (steady_offset + i) as f64 / sr;
            let val = test_sig[steady_offset + i];
            sum_re += val * (2.0 * PI * 60000.0 * t).sin();
            sum_im += val * (2.0 * PI * 60000.0 * t).cos();
        }
        let amp_60k = sum_re.hypot(sum_im) * 2.0 / steady_len as f64;
        let level_60k_db = 20.0 * amp_60k.max(1e-12).log10();

        // Measure 1kHz component
        let mut sum_1k = 0.0;
        for i in 0..steady_len {
            let t = (steady_offset + i) as f64 / sr;
            sum_1k += test_sig[steady_offset + i] * (2.0 * PI * 1000.0 * t).sin();
        }
        let amp_1k = sum_1k * 2.0 / steady_len as f64;
        let level_1k_db = 20.0 * amp_1k.max(1e-12).log10();

        println!("  - Mode {:<4} | 1 kHz Audio Level = {:>6.2} dB | 60 kHz Noise Level = {:>7.2} dB | Noise Suppressed by: {:>5.1} dB",
            mode, level_1k_db, level_60k_db,
            (-10.0 - level_60k_db).max(0.0)
        );
    }
}
