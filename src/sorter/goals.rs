//! Needle in a Haystack inspired Daily Quotas, Weekly Bounties, and Metagame Goals.
//!
//! Tracks sand sifted, treasures uncovered, bucket displacements, and awards
//! Amber and Pearls on completion.

use bevy::prelude::*;

#[derive(Clone, Debug)]
pub struct Goal {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub current: u64,
    pub target: u64,
    pub reward_amber: f64,
    pub reward_pearls: u32,
    pub completed: bool,
}

impl Goal {
    pub fn progress_pct(&self) -> f32 {
        ((self.current as f32 / self.target as f32) * 100.0).clamp(0.0, 100.0)
    }

    pub fn record_progress(&mut self, amount: u64) -> bool {
        if self.completed {
            return false;
        }
        self.current = self.current.saturating_add(amount);
        if self.current >= self.target {
            self.completed = true;
            true
        } else {
            false
        }
    }
}

#[derive(Resource)]
pub struct BeachcomberGoals {
    pub total_grains_sifted: u64,
    pub total_treasures_found: u64,
    pub total_pearls: u32,
    pub daily_day_key: u64,
    pub weekly_week_key: u64,
    pub daily_goals: Vec<Goal>,
    pub weekly_goals: Vec<Goal>,
}

impl Default for BeachcomberGoals {
    fn default() -> Self {
        let now = crate::db::now_unix();
        let day_key = now / 86400;
        let week_key = now / (86400 * 7);

        let mut bg = Self {
            total_grains_sifted: 0,
            total_treasures_found: 0,
            total_pearls: 0,
            daily_day_key: (day_key.max(0)) as u64,
            weekly_week_key: (week_key.max(0)) as u64,
            daily_goals: Vec::new(),
            weekly_goals: Vec::new(),
        };
        bg.refresh_daily();
        bg.refresh_weekly();
        bg
    }
}

impl BeachcomberGoals {
    pub fn refresh_daily(&mut self) {
        self.daily_goals = vec![
            Goal {
                id: "daily_sift",
                title: "Beach Patrol",
                description: "Sift 500 grains of footage",
                current: 0,
                target: 500,
                reward_amber: 75.0,
                reward_pearls: 3,
                completed: false,
            },
            Goal {
                id: "daily_treasures",
                title: "Shoreline Scavenger",
                description: "Unearth 2 buried reel treasures",
                current: 0,
                target: 2,
                reward_amber: 100.0,
                reward_pearls: 5,
                completed: false,
            },
            Goal {
                id: "daily_top_cut",
                title: "High Tide Cut",
                description: "Bucket a candidate cut (score 75+)",
                current: 0,
                target: 1,
                reward_amber: 150.0,
                reward_pearls: 8,
                completed: false,
            },
        ];
    }

    pub fn refresh_weekly(&mut self) {
        self.weekly_goals = vec![
            Goal {
                id: "weekly_grains",
                title: "Master Beachcomber",
                description: "Sift 5,000 grains across video hunts",
                current: 0,
                target: 5000,
                reward_amber: 500.0,
                reward_pearls: 20,
                completed: false,
            },
            Goal {
                id: "weekly_displace",
                title: "Shoreline Hegemony",
                description: "Achieve 3 bucket displacements with better cuts",
                current: 0,
                target: 3,
                reward_amber: 750.0,
                reward_pearls: 30,
                completed: false,
            },
            Goal {
                id: "weekly_crown",
                title: "Crown Dredger",
                description: "Unearth an Epic Gold or Kraken relic cut",
                current: 0,
                target: 1,
                reward_amber: 1000.0,
                reward_pearls: 45,
                completed: false,
            },
        ];
    }

    pub fn on_sand_sifted(&mut self, grains: u64, museum_amber: &mut f64) {
        self.total_grains_sifted += grains;
        for g in self.daily_goals.iter_mut().chain(self.weekly_goals.iter_mut()) {
            if g.id.contains("sift") || g.id.contains("grains") {
                if g.record_progress(grains) {
                    *museum_amber += g.reward_amber;
                    self.total_pearls += g.reward_pearls;
                }
            }
        }
    }

    pub fn on_treasure_uncovered(&mut self, tier_score: f64, museum_amber: &mut f64) {
        self.total_treasures_found += 1;
        for g in self.daily_goals.iter_mut().chain(self.weekly_goals.iter_mut()) {
            if g.id.contains("treasure") {
                if g.record_progress(1) {
                    *museum_amber += g.reward_amber;
                    self.total_pearls += g.reward_pearls;
                }
            }
            if g.id == "weekly_crown" && tier_score >= 80.0 {
                if g.record_progress(1) {
                    *museum_amber += g.reward_amber;
                    self.total_pearls += g.reward_pearls;
                }
            }
        }
    }

    pub fn on_bucket_placed(&mut self, score: f64, museum_amber: &mut f64) {
        for g in self.daily_goals.iter_mut() {
            if g.id == "daily_top_cut" && score >= 75.0 {
                if g.record_progress(1) {
                    *museum_amber += g.reward_amber;
                    self.total_pearls += g.reward_pearls;
                }
            }
        }
    }

    pub fn on_bucket_displaced(&mut self, museum_amber: &mut f64) {
        for g in self.weekly_goals.iter_mut() {
            if g.id == "weekly_displace" {
                if g.record_progress(1) {
                    *museum_amber += g.reward_amber;
                    self.total_pearls += g.reward_pearls;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goals_track_and_complete_correctly() {
        let mut goals = BeachcomberGoals::default();
        let mut amber = 10.0;
        goals.on_sand_sifted(550, &mut amber);
        assert!(goals.daily_goals[0].completed);
        assert!(amber > 10.0);
        assert!(goals.total_pearls > 0);
    }
}
