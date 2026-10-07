//! "Needle In A Haystack" hidden relic discovery system.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedleKind {
    GoldenPin,
    ChronoAmber,
    ApexFang,
    StarlightCore,
}

impl NeedleKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::GoldenPin => "Golden Pin of Babel",
            Self::ChronoAmber => "Chrono-Amber Shard",
            Self::ApexFang => "Apex Fossil Fang",
            Self::StarlightCore => "Starlight Monolith Core",
        }
    }

    pub fn amber_yield(&self) -> f64 {
        match self {
            Self::GoldenPin => 25.0,
            Self::ChronoAmber => 60.0,
            Self::ApexFang => 150.0,
            Self::StarlightCore => 350.0,
        }
    }

    pub fn renown_yield(&self) -> u64 {
        match self {
            Self::GoldenPin => 2,
            Self::ChronoAmber => 8,
            Self::ApexFang => 20,
            Self::StarlightCore => 50,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::GoldenPin => Color::srgb(1.0, 0.85, 0.25),
            Self::ChronoAmber => Color::srgb(1.0, 0.55, 0.15),
            Self::ApexFang => Color::srgb(0.95, 0.95, 0.85),
            Self::StarlightCore => Color::srgb(0.4, 0.85, 1.0),
        }
    }
}



/// Rolls for a hidden needle treasure based on magnet level and serial seed.
pub fn roll_needle(seed: u64, magnet_level: u32) -> Option<NeedleKind> {
    let base_chance_pct = 6 + (magnet_level * 4).min(20);
    let roll = (seed.wrapping_mul(0x517cc1b727220a95) >> 48) % 100;
    if roll >= base_chance_pct as u64 {
        return None;
    }

    let tier_roll = (seed.wrapping_mul(0x9e3779b97f4a7c15) >> 48) % 100;
    if tier_roll < 55 {
        Some(NeedleKind::GoldenPin)
    } else if tier_roll < 82 {
        Some(NeedleKind::ChronoAmber)
    } else if tier_roll < 95 {
        Some(NeedleKind::ApexFang)
    } else {
        Some(NeedleKind::StarlightCore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_needle_produces_valid_tiers() {
        let mut found = 0;
        for s in 0..1000 {
            if let Some(needle) = roll_needle(s, 2) {
                found += 1;
                assert!(needle.amber_yield() > 0.0);
            }
        }
        assert!(found > 40, "expected at least 40 needles in 1000 rolls, found {found}");
    }
}
