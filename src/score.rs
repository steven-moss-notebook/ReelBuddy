pub fn combine(h: f64, v: f64, e: f64, a: f64, p: f64) -> f64 {
    0.30 * h + 0.25 * v + 0.20 * e + 0.15 * a + 0.10 * p
}

pub fn is_scrap(s: f64) -> bool {
    s < 60.0
}

pub fn clamp_term(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

/// The name of the largest term. Ties keep the earlier name.
pub fn strongest_tag(h: f64, v: f64, e: f64, a: f64, p: f64) -> &'static str {
    let mut best = ("hook", h);
    for term in [("virality", v), ("energy", e), ("speech", a), ("payoff", p)] {
        if term.1 > best.1 {
            best = term;
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_and_scrap_threshold() {
        let s = combine(70.0, 80.0, 60.0, 75.0, 66.0);
        let expect = 0.30 * 70.0 + 0.25 * 80.0 + 0.20 * 60.0 + 0.15 * 75.0 + 0.10 * 66.0;
        assert!((s - expect).abs() < 1e-9);
        assert!(is_scrap(59.9));
        assert!(!is_scrap(60.0));
    }
}
