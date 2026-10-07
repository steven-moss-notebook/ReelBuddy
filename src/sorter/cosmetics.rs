//! Beachcombing Upgrades, Tool Progression, and Cosmetic Boutique.
//!
//! Inspired by Needle in a Haystack: metal detector pings, gold sieves,
//! and dynamic sand color themes.

use bevy::prelude::*;
use crate::sorter::sand_hanabi::SandTheme;

#[derive(Resource)]
pub struct BeachcomberBoutique {
    // Upgrades
    pub detector_level: u32,
    pub sieve_level: u32,
    pub magnetic_scoop_level: u32,

    // Active Cosmetics
    pub unlocked_themes: Vec<SandTheme>,
}

impl Default for BeachcomberBoutique {
    fn default() -> Self {
        Self {
            detector_level: 1,
            sieve_level: 1,
            magnetic_scoop_level: 0,
            unlocked_themes: vec![SandTheme::GoldenSunrise],
        }
    }
}

impl BeachcomberBoutique {
    pub fn detector_ping_radius(&self) -> f32 {
        32.0 + (self.detector_level as f32) * 12.0
    }

    pub fn detector_cost(&self) -> f64 {
        40.0 * 1.8f64.powi(self.detector_level as i32)
    }

    pub fn sieve_cost(&self) -> f64 {
        30.0 * 1.7f64.powi(self.sieve_level as i32)
    }

    pub fn buy_detector(&mut self, amber: &mut f64) -> bool {
        let cost = self.detector_cost();
        if *amber >= cost {
            *amber -= cost;
            self.detector_level += 1;
            true
        } else {
            false
        }
    }

    pub fn buy_sieve(&mut self, amber: &mut f64) -> bool {
        let cost = self.sieve_cost();
        if *amber >= cost {
            *amber -= cost;
            self.sieve_level += 1;
            true
        } else {
            false
        }
    }

    pub fn owns_theme(&self, theme: SandTheme) -> bool {
        self.unlocked_themes.contains(&theme)
    }

    pub fn buy_theme(&mut self, theme: SandTheme, amber: &mut f64) -> bool {
        if self.owns_theme(theme) {
            return true;
        }
        let cost = theme.amber_cost();
        if *amber < cost {
            return false;
        }
        *amber -= cost;
        self.unlocked_themes.push(theme);
        true
    }

    pub fn cycle_theme(&self, current: SandTheme) -> SandTheme {
        let catalog = SandTheme::catalog();
        let Some(start) = catalog.iter().position(|theme| *theme == current) else {
            return SandTheme::GoldenSunrise;
        };
        for step in 1..=catalog.len() {
            let theme = catalog[(start + step) % catalog.len()];
            if self.owns_theme(theme) {
                return theme;
            }
        }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sorter::sand_hanabi::SandTheme;

    #[test]
    fn snow_and_halloween_need_amber_and_cycle_skips_locked() {
        let mut boutique = BeachcomberBoutique::default();
        let mut amber = 0.0;
        assert!(!boutique.buy_theme(SandTheme::Snow, &mut amber));
        assert!(!boutique.buy_theme(SandTheme::Halloween, &mut amber));
        assert_eq!(boutique.cycle_theme(SandTheme::GoldenSunrise), SandTheme::GoldenSunrise);

        amber = 500.0;
        assert!(boutique.buy_theme(SandTheme::Snow, &mut amber));
        assert!(boutique.buy_theme(SandTheme::Halloween, &mut amber));
        assert!(amber < 500.0);

        let mut theme = SandTheme::GoldenSunrise;
        for _ in 0..8 {
            theme = boutique.cycle_theme(theme);
            assert!(boutique.owns_theme(theme));
            assert_ne!(theme, SandTheme::Bioluminescent);
            assert_ne!(theme, SandTheme::VolcanicObsidian);
            assert_ne!(theme, SandTheme::CoralPink);
            assert_ne!(theme, SandTheme::Space);
        }
        assert!(boutique.owns_theme(SandTheme::Snow));
        assert!(boutique.owns_theme(SandTheme::Halloween));
        assert!(!boutique.owns_theme(SandTheme::Space));

        amber = 200.0;
        assert!(boutique.buy_theme(SandTheme::Space, &mut amber));
        assert!(boutique.owns_theme(SandTheme::Space));
        assert!(amber < 200.0);
        let mut seen_space = false;
        for _ in 0..8 {
            theme = boutique.cycle_theme(theme);
            assert!(boutique.owns_theme(theme));
            if theme == SandTheme::Space {
                seen_space = true;
            }
        }
        assert!(seen_space);
    }
}
