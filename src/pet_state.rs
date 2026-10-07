//! Pet singleton and the only spoken lines. UI systems render these; they do not invent copy.

pub const HUNGER_MAX: u8 = 100;
pub const HUNGER_STEP: u8 = 10;
pub const HUNGRY_ABOVE: u8 = 60;
pub const LINE_MAX: usize = 140;
pub const LESSON_MAX: usize = 240;

pub const SEED_HUNGER: u8 = 40;
pub const SEED_ENERGY: u8 = 100;
pub const SEED_MOOD: u8 = 50;
pub const SEED_BOND: u8 = 50;
pub const SEED_STAGE: u8 = 1;
pub const SEED_SHIPPED: u32 = 0;

pub const BOOT_LINE: &str = "Waking beach buddy...";
pub const HUNGRY_LINE: &str = "Drop a video to start sifting!";
pub const CHAMBER_STILL: &str = "Shoreline Quiet";
pub const CHAMBER_STILL_DETAIL: &str = "Drop a video to comb for cuts.";
pub const CHAMBER_CACHE: &str = "Checking AI Weights";
pub const CHAMBER_CACHE_DETAIL: &str = "Checking local model files...";
pub const CHAMBER_SHORE: &str = "Incoming Tide";
pub const CHAMBER_WAKING: &str = "Awakening AI Eye";
pub const CHAMBER_SCAN: &str = "Sifting Footage";
pub const CHAMBER_CHOOSE: &str = "Combing for Gems";
pub const CHAMBER_CHOOSE_DETAIL: &str = "Curating top 3 reel treasures...";
pub const CHAMBER_PAINTING: &str = "Polishing Reel";
pub const CHAMBER_PAINT_DETAIL: &str = "Burning captions & audio...";
pub const CHAMBER_SET: &str = "Treasures Unearthed!";
pub const CHAMBER_SET_DETAIL: &str = "3 candidate reels ready to inspect";
pub const CHAMBER_BROKEN: &str = "Notice";
pub const CANT_CUT: &str = "No clean cuts found here.";
pub const NOTHING_TO_CUT: &str = "Only driftwood found in clip.";
pub const LOCAL_ONLY_LINE: &str = "Please drop a local video file (.mp4, .mov).";
pub const SORTING_LINE: &str = "Combing through the waves...";
pub const LEVEL_READY_LINE: &str = "Reel treasures ready to inspect!";
pub const KEPT_LINE: &str = "Pocketed Reel:";
pub const MODEL_REPO: &str = "unsloth/gemma-4-26B-A4B-it-qat-GGUF";
pub const MTP_REPO: &str = "unsloth/gemma-4-26B-A4B-it-GGUF";
pub const MTP_FILE: &str = "mtp-gemma-4-26B-A4B-it.gguf";

/// Unsloth / Google recommended sampling for Gemma 4 instruct.
pub const EDIT_TEMPERATURE: f32 = 1.0;
pub const LINE_TEMPERATURE: f32 = 1.0;
pub const SAMPLE_TOP_K: i32 = 64;
pub const SAMPLE_TOP_P: f32 = 0.95;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteKind {
    Asleep,
    Idle,
    Hungry,
    Judging,
    Eating,
    Sulk,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PetSnapshot {
    pub hunger: u8,
    pub energy: u8,
    pub mood: u8,
    pub bond: u8,
    pub stage: u8,
    pub shipped: u32,
    pub line: String,
    pub model_present: bool,
    /// Set after the editor thread has applied a hunger bump. Wakes the sprite off Asleep.
    pub woke: bool,
    pub busy: bool,
    pub eating: bool,
    pub sulk: bool,
    pub collar: Option<u8>,
}

impl PetSnapshot {
    pub fn seed_asleep(line: String) -> Self {
        Self {
            hunger: SEED_HUNGER,
            energy: SEED_ENERGY,
            mood: SEED_MOOD,
            bond: SEED_BOND,
            stage: SEED_STAGE,
            shipped: SEED_SHIPPED,
            line,
            model_present: false,
            woke: false,
            busy: false,
            eating: false,
            sulk: false,
            collar: None,
        }
    }

    pub fn sprite(&self) -> SpriteKind {
        sprite_for(self)
    }
}

pub fn seed_line() -> String {
    HUNGRY_LINE.to_string()
}

pub fn boot_line() -> String {
    BOOT_LINE.to_string()
}

pub fn cant_cut_line() -> String {
    CANT_CUT.to_string()
}

pub fn nothing_to_cut_line() -> String {
    NOTHING_TO_CUT.to_string()
}

pub fn sorting_line() -> String {
    SORTING_LINE.to_string()
}

pub fn level_ready_line() -> String {
    LEVEL_READY_LINE.to_string()
}

pub fn kept_line(file: &str) -> String {
    truncate_line(&format!("{KEPT_LINE} {file}"))
}

pub fn chamber_shore_line(file: &str, pct: f32) -> String {
    truncate_line(&format!("{CHAMBER_SHORE} {file} {pct:.0}%"))
}

pub fn chamber_waking_line(stage: &str, pct: f32) -> String {
    truncate_line(&format!("{CHAMBER_WAKING} {stage} {:.0}%", pct.clamp(0.0, 1.0) * 100.0))
}

pub fn chamber_scan_line(done: u32, total: u32) -> String {
    truncate_line(&format!("Combing: {done} of {total} frames"))
}

pub fn missing_model_line() -> String {
    truncate_line(&format!(
        "Need model.gguf & mmproj.gguf from {MODEL_REPO}."
    ))
}

pub fn model_error_line(detail: &str) -> String {
    truncate_line(detail)
}

pub fn stage_five_line(note: &str) -> String {
    truncate_line(&format!("Buddy pick: {note}"))
}

pub fn spoken_line(note: &str, stage: u8, disagrees: bool) -> String {
    if stage >= 5 && disagrees {
        stage_five_line(note)
    } else {
        truncate_line(note)
    }
}

pub fn truncate_line(text: &str) -> String {
    truncate_chars(text, LINE_MAX)
}

pub fn truncate_lesson(text: &str) -> String {
    truncate_chars(text, LESSON_MAX)
}

fn truncate_chars(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

pub fn bump_hunger(hunger: u8) -> u8 {
    hunger.saturating_add(HUNGER_STEP).min(HUNGER_MAX)
}

pub fn hunger_after_hunt(hunger: u8) -> u8 {
    hunger.saturating_add(15).min(HUNGER_MAX)
}

pub fn hunger_after_pick(hunger: u8) -> u8 {
    hunger.saturating_sub(20)
}

pub fn stage_for_shipped(shipped: u32) -> u8 {
    if shipped >= 40 {
        5
    } else if shipped >= 10 {
        3
    } else {
        1
    }
}

pub fn bond_from_counts(accepts: u32, rejects: u32) -> u8 {
    let total = accepts + rejects;
    if total == 0 {
        SEED_BOND
    } else {
        ((accepts * 100) / total) as u8
    }
}

pub fn mood_from_scores(scores: &[f64]) -> u8 {
    if scores.is_empty() {
        return SEED_MOOD;
    }
    let mean = scores.iter().sum::<f64>() / scores.len() as f64;
    mean.round().clamp(0.0, 100.0) as u8
}

pub fn sprite_kind(model_present: bool, woke: bool, hunger: u8) -> SpriteKind {
    if hunger > HUNGRY_ABOVE {
        SpriteKind::Hungry
    } else if model_present || woke {
        SpriteKind::Idle
    } else {
        SpriteKind::Asleep
    }
}

pub fn sprite_for(pet: &PetSnapshot) -> SpriteKind {
    if pet.eating {
        SpriteKind::Eating
    } else if pet.busy {
        SpriteKind::Judging
    } else if pet.sulk {
        SpriteKind::Sulk
    } else {
        sprite_kind(pet.model_present, pet.woke, pet.hunger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_is_asleep_until_the_channel_wakes_it() {
        let pet = PetSnapshot::seed_asleep(missing_model_line());
        assert_eq!(pet.hunger, 40);
        assert_eq!(pet.sprite(), SpriteKind::Asleep);
        assert!(pet.line.contains(MODEL_REPO));
        assert!(pet.line.chars().count() <= LINE_MAX);
    }

    #[test]
    fn hunger_bump_swaps_off_asleep_then_follows_the_threshold() {
        let mut hunger = SEED_HUNGER;
        hunger = bump_hunger(hunger);
        assert_eq!(hunger, 50);
        assert_eq!(sprite_kind(false, true, hunger), SpriteKind::Idle);

        hunger = bump_hunger(bump_hunger(hunger));
        assert_eq!(hunger, 70);
        assert_eq!(sprite_kind(false, true, hunger), SpriteKind::Hungry);
        assert_eq!(sprite_kind(true, false, 60), SpriteKind::Idle);
        assert_eq!(sprite_kind(true, false, 61), SpriteKind::Hungry);
    }

    #[test]
    fn present_model_starts_idle_with_the_hungry_line() {
        assert_eq!(sprite_kind(true, false, SEED_HUNGER), SpriteKind::Idle);
        assert_eq!(seed_line(), HUNGRY_LINE);
    }

    #[test]
    fn scan_line_counts_frames_and_is_not_the_title() {
        let line = chamber_scan_line(12, 40);
        assert!(line.contains("12 of 40"));
        assert_ne!(line, CHAMBER_SCAN);
        assert!(line.chars().count() <= LINE_MAX);
    }

    #[test]
    fn lines_never_pass_140_chars() {
        let long = "x".repeat(200);
        assert_eq!(truncate_line(&long).chars().count(), LINE_MAX);
    }

    #[test]
    fn shipped_counts_map_to_stages() {
        assert_eq!(stage_for_shipped(0), 1);
        assert_eq!(stage_for_shipped(10), 3);
        assert_eq!(stage_for_shipped(40), 5);
        assert_eq!(stage_for_shipped(100), 5);
    }

    #[test]
    fn stage_five_disagrees_in_the_line_only() {
        let line = spoken_line("Opens on a face.", 5, true);
        assert!(line.starts_with("Buddy pick:"));
        assert_eq!(spoken_line("Opens on a face.", 3, true), "Opens on a face.");
    }
}
