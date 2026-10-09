//! Small statistics helpers. The arithmetic order follows the prototype so that results are
//! bit-identical where the parity harness compares them.

/// Percentile with linear interpolation between closest ranks (numpy's default). `None` if empty.
pub fn percentile(values: &[u64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let k = (v.len() - 1) as f64 * q;
    let (f, c) = (k.floor(), k.ceil());
    if f == c {
        return Some(v[k as usize] as f64);
    }
    Some(v[f as usize] as f64 * (c - k) + v[c as usize] as f64 * (k - f))
}

/// Median of integer counts; the mean of the two middle values for an even count.
pub fn median(values: &[u32]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2] as f64
    } else {
        (v[n / 2 - 1] + v[n / 2]) as f64 / 2.0
    })
}

/// Shannon entropy of a distribution, normalized by `ln(#values)`. 0 for fewer than two non-zero
/// values.
pub fn entropy_norm(values: &[u64]) -> f64 {
    let total: u64 = values.iter().sum();
    if total == 0 {
        return 0.0;
    }
    let ps: Vec<f64> = values
        .iter()
        .filter(|&&x| x > 0)
        .map(|&x| x as f64 / total as f64)
        .collect();
    if ps.len() <= 1 {
        return 0.0;
    }
    // libm, not the platform's `ln`: identical results natively and in the WASM dashboard.
    let h: f64 = -ps.iter().map(|p| p * libm::log(*p)).sum::<f64>();
    h / libm::log(values.len() as f64)
}

/// `100 · num / den`, or `None` when there is no denominator (spec §1.1: no data ≠ zero).
pub fn pct(num: u64, den: u64) -> Option<f64> {
    (den > 0).then(|| (100 * num) as f64 / den as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles() {
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(percentile(&[7], 0.9), Some(7.0));
        assert_eq!(percentile(&[1, 2, 3, 4], 0.5), Some(2.5));
        assert_eq!(percentile(&[10, 1, 4, 3, 2], 0.5), Some(3.0));
        // k = 0.9 · 9 = 8.1 → 9 · 0.9 + 10 · 0.1
        let p = percentile(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.9).unwrap();
        assert!((p - 9.1).abs() < 1e-9);
    }

    #[test]
    fn medians() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3, 1, 2]), Some(2.0));
        assert_eq!(median(&[4, 1, 2, 3]), Some(2.5));
    }

    #[test]
    fn entropy() {
        assert_eq!(entropy_norm(&[]), 0.0);
        assert_eq!(entropy_norm(&[50]), 0.0);
        assert!((entropy_norm(&[50, 50]) - 1.0).abs() < 1e-12);
        let e = entropy_norm(&[100, 50, 50]);
        assert!(e > 0.9 && e < 1.0);
    }

    #[test]
    fn percentages() {
        assert_eq!(pct(1, 0), None);
        assert_eq!(pct(0, 4), Some(0.0));
        assert_eq!(pct(1, 3), Some(100.0 / 3.0));
    }
}
