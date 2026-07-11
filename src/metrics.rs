use num_complex::Complex64;

#[derive(Debug, Clone)]
pub struct DecodedMetrics {
    pub slot_count: usize,
    pub max_abs_error: f64,
    pub mean_abs_error: f64,
    pub rms_error: f64,
    pub relative_l2_error: f64,
    pub snr_db: f64,
}

pub fn decoded_metrics(correct: &[Complex64], faulty: &[Complex64]) -> DecodedMetrics {
    assert_eq!(correct.len(), faulty.len());
    let mut max_abs = 0.0f64;
    let mut sum_abs = 0.0f64;
    let mut err2 = 0.0f64;
    let mut sig2 = 0.0f64;

    for (c, f) in correct.iter().zip(faulty.iter()) {
        let e = *f - *c;
        let ea = e.norm();
        max_abs = max_abs.max(ea);
        sum_abs += ea;
        err2 += e.norm_sqr();
        sig2 += c.norm_sqr();
    }

    let n = correct.len().max(1) as f64;
    let rms = (err2 / n).sqrt();
    let rel = if sig2 > 0.0 {
        err2.sqrt() / sig2.sqrt()
    } else {
        f64::INFINITY
    };
    let snr = if err2 > 0.0 {
        10.0 * (sig2 / err2).log10()
    } else {
        f64::INFINITY
    };

    DecodedMetrics {
        slot_count: correct.len(),
        max_abs_error: max_abs,
        mean_abs_error: sum_abs / n,
        rms_error: rms,
        relative_l2_error: rel,
        snr_db: snr,
    }
}
