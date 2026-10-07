//! Procedural specimen blueprints, anatomical sockets, and LLM-driven lore generation.

use crate::editor::Scores;
use crate::field::Recipe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Archetype {
    PaleoFossil,
    Clockwork,
    Astral,
}

impl Archetype {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::PaleoFossil => "Paleo Fossil",
            Self::Clockwork => "Clockwork Automaton",
            Self::Astral => "Astral Reliquary",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::PaleoFossil => "🦴",
            Self::Clockwork => "⚙️",
            Self::Astral => "🔮",
        }
    }

    pub fn socket_names(&self) -> [&'static str; 4] {
        match self {
            Self::PaleoFossil => ["Cranium", "Spine", "Ribcage", "Apex Talon"],
            Self::Clockwork => ["Escapement Core", "Chassis Spine", "Gearbox", "Piston Strut"],
            Self::Astral => ["Obsidian Eye", "Resonant Core", "Astrolabe Ring", "Prismatic Shard"],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Blueprint {
    pub id: u64,
    pub name: String,
    pub archetype: Archetype,
    pub slots: [Option<Recipe>; 4],
    pub lore_hint: String,
    pub llm_tag: Option<String>,
}

impl Blueprint {
    pub fn is_complete(&self) -> bool {
        self.slots.iter().all(|s| s.is_some())
    }

    pub fn filled_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn needs_slot(&self, slot: usize) -> bool {
        slot < 4 && self.slots[slot].is_none()
    }

    pub fn insert_part(&mut self, slot: usize, recipe: Recipe) -> bool {
        if self.needs_slot(slot) {
            self.slots[slot] = Some(recipe);
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug)]
pub struct Exhibit {
    pub id: u64,
    pub name: String,
    pub archetype: Archetype,
    pub parts: [Recipe; 4],
    pub renown_reward: u64,
    pub ticket_rate: f64,
    pub plaque: String,
    pub llm_echo: Option<String>,
}

/// Determine which anatomical socket a recipe/fragment belongs to.
pub fn socket_for_recipe(recipe: &Recipe) -> usize {
    if recipe.strokes > 2 || (recipe.seed & 3) == 0 {
        0 // Cranium / Core
    } else if recipe.theta.abs() < 0.6 {
        1 // Spine / Axial
    } else if recipe.freq > 0.22 {
        2 // Ribs / Gearbox
    } else {
        3 // Talon / Limb / Shard
    }
}

/// Generate the next procedural blueprint, optionally informed by LLM insights.
pub fn generate_blueprint(
    id: u64,
    llm_scores: Option<&Scores>,
    llm_hook: Option<&str>,
    llm_lesson: Option<&str>,
) -> Blueprint {
    let archetype = match (id % 3) as usize {
        0 => Archetype::PaleoFossil,
        1 => Archetype::Clockwork,
        _ => Archetype::Astral,
    };

    let name = generate_name(id, archetype, llm_scores, llm_hook);
    let lore_hint = generate_hint(archetype, llm_lesson);
    let llm_tag = llm_hook.map(|h| h.chars().take(48).collect());

    Blueprint {
        id,
        name,
        archetype,
        slots: [None, None, None, None],
        lore_hint,
        llm_tag,
    }
}

fn generate_name(
    id: u64,
    archetype: Archetype,
    scores: Option<&Scores>,
    hook: Option<&str>,
) -> String {
    let prefix = match archetype {
        Archetype::PaleoFossil => {
            let prefixes = ["Titan", "Archae", "Dromaeo", "Cryo", "Spino", "Rex"];
            prefixes[(id as usize) % prefixes.len()]
        }
        Archetype::Clockwork => {
            let prefixes = ["Brass", "Aether", "Piston", "Chrono", "Galvanic", "Gilded"];
            prefixes[(id as usize) % prefixes.len()]
        }
        Archetype::Astral => {
            let prefixes = ["Void", "Stellar", "Obsidian", "Hyperborean", "Solar", "Eclipse"];
            prefixes[(id as usize) % prefixes.len()]
        }
    };

    let suffix = match archetype {
        Archetype::PaleoFossil => {
            let suffixes = ["spondylus", "raptor", "tyrannus", "gnathus", "saurus"];
            suffixes[((id >> 2) as usize) % suffixes.len()]
        }
        Archetype::Clockwork => {
            let suffixes = ["mancer", "golem", "automaton", "engine", "colossus"];
            suffixes[((id >> 2) as usize) % suffixes.len()]
        }
        Archetype::Astral => {
            let suffixes = ["astrolabe", "prism", "monolith", "crucible", "beacon"];
            suffixes[((id >> 2) as usize) % suffixes.len()]
        }
    };

    // If the LLM provided a hook and high energy or visual scores, inject an epithet
    if let (Some(h), Some(s)) = (hook, scores) {
        if s.e > 65.0 {
            let short_hook: String = h.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
            return format!("{prefix}{suffix} \"{short_hook}\"");
        }
    }

    format!("{prefix}{suffix} #{id:02}")
}

fn generate_hint(archetype: Archetype, lesson: Option<&str>) -> String {
    if let Some(l) = lesson {
        if !l.is_empty() {
            return format!("Field note: {}", l.chars().take(80).collect::<String>());
        }
    }
    match archetype {
        Archetype::PaleoFossil => "Unearthed from deep strata. Requires 4 articulated fossils.",
        Archetype::Clockwork => "Preserved in clock-oil silt. Requires 4 precision mechanical parts.",
        Archetype::Astral => "Resonates with cosmic frequency. Requires 4 attuned astral relics.",
    }
    .to_string()
}

/// Convert a completed blueprint into an exhibit with full museum plaque.
pub fn complete_blueprint(blueprint: Blueprint, llm_lesson: Option<&str>) -> Exhibit {
    let parts = [
        blueprint.slots[0].unwrap_or_else(dummy_recipe),
        blueprint.slots[1].unwrap_or_else(dummy_recipe),
        blueprint.slots[2].unwrap_or_else(dummy_recipe),
        blueprint.slots[3].unwrap_or_else(dummy_recipe),
    ];

    let base_renown = 35 + (blueprint.id * 5).min(100);
    let base_ticket = 0.8 + (blueprint.id as f64 * 0.25).min(8.0);

    let plaque = format!(
        "{} — Assembled in the Conservation Bay.\n{}\nArchival Class: {} | Restoration Rating: Pristine",
        blueprint.name,
        blueprint.lore_hint,
        blueprint.archetype.display_name(),
    );

    Exhibit {
        id: blueprint.id,
        name: blueprint.name,
        archetype: blueprint.archetype,
        parts,
        renown_reward: base_renown,
        ticket_rate: base_ticket,
        plaque,
        llm_echo: llm_lesson.map(|l| l.to_string()),
    }
}

#[cfg(test)]
pub(crate) fn dummy_recipe_for_test() -> Recipe {
    dummy_recipe()
}

fn dummy_recipe() -> Recipe {
    Recipe {
        color: [180, 160, 140],
        theta: 0.0,
        freq: 0.2,
        seed: 42,
        lumps: 2,
        strokes: 2,
        is_flat: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_assignment_is_deterministic() {
        let r1 = Recipe {
            color: [200, 100, 50],
            theta: 0.1,
            freq: 0.18,
            seed: 16,
            lumps: 2,
            strokes: 4,
            is_flat: false,
        };
        assert_eq!(socket_for_recipe(&r1), 0); // strokes > 2 -> Cranium

        let r2 = Recipe {
            color: [50, 100, 200],
            theta: 0.2,
            freq: 0.18,
            seed: 17,
            lumps: 2,
            strokes: 2,
            is_flat: false,
        };
        assert_eq!(socket_for_recipe(&r2), 1); // theta < 0.6 -> Spine
    }

    #[test]
    fn blueprint_completion_flow() {
        let mut bp = generate_blueprint(1, None, None, None);
        assert!(!bp.is_complete());
        assert_eq!(bp.filled_count(), 0);

        for slot in 0..4 {
            assert!(bp.needs_slot(slot));
            bp.insert_part(slot, dummy_recipe());
        }

        assert!(bp.is_complete());
        let exhibit = complete_blueprint(bp, Some("Rare specimen reconstructed."));
        assert_eq!(exhibit.id, 1);
        assert!(exhibit.renown_reward >= 35);
        assert!(exhibit.plaque.contains("Assembled in the Conservation Bay"));
    }
}
