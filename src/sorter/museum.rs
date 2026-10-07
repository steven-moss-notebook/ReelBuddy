//! Museum economy, exhibit gallery, upgrades, and LLM insight caching.

use bevy::prelude::*;

use crate::editor::{ChoreKind, ChoreView, Scores};
use crate::sorter::specimen::{self, Blueprint, Exhibit};

#[derive(Resource)]
pub struct Museum {
    pub amber: f64,
    pub renown: u64,
    pub serial: u64,
    pub active_blueprint: Blueprint,
    pub completed_exhibits: Vec<Exhibit>,

    // Upgrades
    pub bellows_level: u32,
    pub magnet_level: u32,
    pub apprentice_level: u32,
    pub wing_level: u32,

    // LLM Insights from recent video analysis
    pub cached_scores: Option<Scores>,
    pub cached_hook: Option<String>,
    pub cached_lesson: Option<String>,

    // UI state
    pub show_gallery: bool,
    pub show_menu: bool,
    pub show_store: bool,
}

impl Default for Museum {
    fn default() -> Self {
        let first_blueprint = specimen::generate_blueprint(1, None, None, None);
        Self {
            amber: 0.0,
            renown: 0,
            serial: 1,
            active_blueprint: first_blueprint,
            completed_exhibits: Vec::new(),
            bellows_level: 0,
            magnet_level: 0,
            apprentice_level: 0,
            wing_level: 1,
            cached_scores: None,
            cached_hook: None,
            cached_lesson: None,
            show_gallery: false,
            show_menu: false,
            show_store: false,
        }
    }
}

impl Museum {
    pub fn wing_multiplier(&self) -> f64 {
        match self.wing_level {
            1 => 1.0,
            2 => 1.75,
            3 => 3.0,
            _ => 1.0 + (self.wing_level as f64) * 0.75,
        }
    }

    pub fn passive_ticket_rate(&self) -> f64 {
        let base: f64 = self.completed_exhibits.iter().map(|e| e.ticket_rate).sum();
        base * self.wing_multiplier()
    }

    pub fn tick_economy(&mut self, dt: f64) {
        let earned = self.passive_ticket_rate() * dt;
        self.amber += earned;
    }

    pub fn complete_current_blueprint(&mut self) -> Exhibit {
        let lesson = self.cached_lesson.as_deref();
        let exhibit = specimen::complete_blueprint(self.active_blueprint.clone(), lesson);
        self.renown += exhibit.renown_reward;
        self.amber += 50.0;
        self.completed_exhibits.push(exhibit.clone());

        // Roll next blueprint
        self.serial += 1;
        self.active_blueprint = specimen::generate_blueprint(
            self.serial,
            self.cached_scores.as_ref(),
            self.cached_hook.as_deref(),
            self.cached_lesson.as_deref(),
        );

        exhibit
    }

    /// Extract hooks, lessons, and scores from LLM chore updates.
    pub fn ingest_chore_insight(&mut self, chore: &ChoreView) {
        match &chore.kind {
            ChoreKind::Hunt { options } => {
                if let Some(best) = options.iter().max_by(|a, b| a.s.total_cmp(&b.s)) {
                    self.cached_scores = Some(Scores {
                        h: best.h,
                        v: best.v,
                        e: best.e,
                        a: best.a,
                        p: best.p,
                        s: best.s,
                        note: best.caption.clone(),
                    });
                    if !best.hook.is_empty() {
                        self.cached_hook = Some(best.hook.clone());
                    } else if let Some(first_hook) = best.hooks.first() {
                        self.cached_hook = Some(first_hook.clone());
                    }
                    if !best.lesson.is_empty() {
                        self.cached_lesson = Some(best.lesson.clone());
                    }
                }
            }
            ChoreKind::Cut { hooks, note, .. } => {
                if let Some(h) = hooks.first() {
                    self.cached_hook = Some(h.clone());
                }
                if !note.is_empty() {
                    self.cached_lesson = Some(note.clone());
                }
            }
            _ => {}
        }
    }

    // Upgrade costs
    pub fn bellows_cost(&self) -> f64 {
        40.0 * (1.8f64).powi(self.bellows_level as i32)
    }

    pub fn magnet_cost(&self) -> f64 {
        60.0 * (2.0f64).powi(self.magnet_level as i32)
    }

    pub fn apprentice_cost(&self) -> f64 {
        80.0 * (2.2f64).powi(self.apprentice_level as i32)
    }

    pub fn wing_cost(&self) -> f64 {
        200.0 * (3.0f64).powi((self.wing_level - 1) as i32)
    }

    pub fn buy_bellows(&mut self) -> bool {
        let cost = self.bellows_cost();
        if self.amber >= cost {
            self.amber -= cost;
            self.bellows_level += 1;
            true
        } else {
            false
        }
    }

    pub fn buy_magnet(&mut self) -> bool {
        let cost = self.magnet_cost();
        if self.amber >= cost {
            self.amber -= cost;
            self.magnet_level += 1;
            true
        } else {
            false
        }
    }

    pub fn buy_apprentice(&mut self) -> bool {
        let cost = self.apprentice_cost();
        if self.amber >= cost {
            self.amber -= cost;
            self.apprentice_level += 1;
            true
        } else {
            false
        }
    }

    pub fn buy_wing(&mut self) -> bool {
        let cost = self.wing_cost();
        if self.amber >= cost && self.wing_level < 3 {
            self.amber -= cost;
            self.wing_level += 1;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn museum_passive_income_scales_with_exhibits() {
        let mut m = Museum::default();
        assert_eq!(m.passive_ticket_rate(), 0.0);

        // Fill all 4 slots
        for slot in 0..4 {
            m.active_blueprint.insert_part(slot, crate::sorter::specimen::dummy_recipe_for_test());
        }
        let ex = m.complete_current_blueprint();
        assert!(ex.ticket_rate > 0.0);
        assert!(m.passive_ticket_rate() > 0.0);

        m.tick_economy(10.0);
        assert!(m.amber > 0.0);
    }
}
