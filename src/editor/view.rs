use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChoreView {
    pub id: i64,
    #[serde(flatten)]
    pub kind: ChoreKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChoreKind {
    Hunt { options: Vec<HuntOption> },
    Cut {
        window_id: i64,
        hooks: Vec<String>,
        note: String,
    },
    Hook {
        window_id: i64,
        a: String,
        b: String,
    },
    Remix { window_id: i64, clip_id: i64 },
    Ship {
        window_id: i64,
        clip_id: i64,
        note: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HuntOption {
    pub window_id: i64,
    pub label: String,
    #[serde(default)]
    pub start_s: f64,
    #[serde(default)]
    pub end_s: f64,
    #[serde(default)]
    pub segments: Vec<(f64, f64)>,
    #[serde(default)]
    pub motion: String,
    #[serde(default)]
    pub caption: String,
    #[serde(default)]
    pub hook: String,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub h: f64,
    #[serde(default)]
    pub v: f64,
    #[serde(default)]
    pub e: f64,
    #[serde(default)]
    pub a: f64,
    #[serde(default)]
    pub p: f64,
    #[serde(default)]
    pub s: f64,
    #[serde(default)]
    pub lesson: String,
    #[serde(default)]
    pub thumb: String,
    #[serde(default)]
    pub thumbs: Vec<String>,
}

impl ChoreKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Hunt { .. } => "hunt",
            Self::Cut { .. } => "cut",
            Self::Hook { .. } => "hook_duel",
            Self::Remix { .. } => "remix",
            Self::Ship { .. } => "ship",
        }
    }
}
