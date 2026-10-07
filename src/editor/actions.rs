use serde::{Deserialize, Serialize};

use crate::score::{self, clamp_term};

#[derive(Clone, Debug, PartialEq)]
pub struct Moment {
    pub start_s: f64,
    pub end_s: f64,
    pub why: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NormCrop {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn default_center() -> f64 {
    0.5
}

fn default_one() -> f64 {
    1.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundStyle {
    #[default]
    Blur,
    Black,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Framing {
    Fit {
        #[serde(default)]
        background: BackgroundStyle,
    },
    Crop {
        #[serde(default = "default_center")]
        focus_x: f64,
        #[serde(default = "default_center")]
        focus_y: f64,
        #[serde(default = "default_one")]
        zoom: f64,
    },
    CropAndMove {
        #[serde(default)]
        start_x: f64,
        #[serde(default = "default_one")]
        end_x: f64,
        #[serde(default = "default_center")]
        focus_y: f64,
        #[serde(default = "default_one")]
        zoom: f64,
    },
}

impl Default for Framing {
    fn default() -> Self {
        Framing::Crop {
            focus_x: 0.5,
            focus_y: 0.5,
            zoom: 1.0,
        }
    }
}

impl Framing {
    pub fn to_norm_crop(&self, u: f64) -> NormCrop {
        let u = u.clamp(0.0, 1.0);
        match self {
            Framing::Fit { .. } => NormCrop {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            Framing::Crop { focus_x, focus_y, zoom } => {
                let z = zoom.clamp(0.1, 1.0);
                let w = (0.5625 * z).clamp(0.05, 1.0);
                let h = z.clamp(0.05, 1.0);
                let x = (focus_x - w * 0.5).clamp(0.0, 1.0 - w);
                let y = (focus_y - h * 0.5).clamp(0.0, 1.0 - h);
                NormCrop { x, y, w, h }
            }
            Framing::CropAndMove { start_x, end_x, focus_y, zoom } => {
                let current_x = start_x + (end_x - start_x) * u;
                let z = zoom.clamp(0.1, 1.0);
                let w = (0.5625 * z).clamp(0.05, 1.0);
                let h = z.clamp(0.05, 1.0);
                let x = (current_x - w * 0.5).clamp(0.0, 1.0 - w);
                let y = (focus_y - h * 0.5).clamp(0.0, 1.0 - h);
                NormCrop { x, y, w, h }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub t: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StyledSpan {
    pub text: String,
    pub effect: Option<String>,
    pub gradient: Option<String>,
}

impl Cue {
    pub fn new(t: f64, text: impl Into<String>) -> Self {
        Self {
            t,
            text: text.into(),
            effect: None,
            gradient: None,
        }
    }

    pub fn with_effect(mut self, effect: impl Into<String>) -> Self {
        self.effect = Some(effect.into());
        self
    }

    pub fn with_gradient(mut self, gradient: impl Into<String>) -> Self {
        self.gradient = Some(gradient.into());
        self
    }

    pub fn plain_text(&self) -> String {
        strip_tags(&self.text)
    }

    pub fn word_count(&self) -> usize {
        self.plain_text().split_whitespace().count()
    }

    pub fn styled_spans(&self) -> Vec<StyledSpan> {
        parse_styled_spans(&self.text, self.effect.as_deref(), self.gradient.as_deref())
    }
}

pub fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        if ch == '[' {
            in_tag = true;
        } else if ch == ']' && in_tag {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }
    out
}

pub fn normalize_effect_and_gradient(
    effect: Option<String>,
    gradient: Option<String>,
) -> (Option<String>, Option<String>) {
    let Some(eff) = effect else {
        return (None, gradient);
    };
    if let Some((name, grad)) = eff.split_once(':') {
        let canonical_eff = canonical_effect_name(name.trim()).map(|s| s.to_string());
        let grad_name = grad.trim().to_string();
        (canonical_eff.or_else(|| Some(name.trim().to_string())), Some(grad_name))
    } else {
        let canonical_eff = canonical_effect_name(eff.trim()).map(|s| s.to_string());
        (canonical_eff.or_else(|| Some(eff.trim().to_string())), gradient)
    }
}

pub fn canonical_effect_name(name: &str) -> Option<&'static str> {
    match name.to_lowercase().as_str() {
        "shake" | "pause" => Some("shake"),
        "bounce" | "speed_up" | "speedup" => Some("bounce"),
        "wave" | "slow_down" | "slowdown" => Some("wave"),
        "pulse" | "emit_events" | "emitevents" => Some("pulse"),
        "color_shift" | "colorshift" | "one_shot" | "oneshot" => Some("color_shift"),
        "gradient" | "gradient_flow" => Some("gradient"),
        _ => None,
    }
}

pub fn parse_styled_spans(
    text: &str,
    default_effect: Option<&str>,
    default_gradient: Option<&str>,
) -> Vec<StyledSpan> {
    let mut spans = Vec::new();
    let mut cursor = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    let mut current_text = String::new();

    while cursor < len {
        if bytes[cursor] == b'[' {
            if let Some(close_bracket) = text[cursor..].find(']') {
                let tag_content = &text[cursor + 1..cursor + close_bracket];
                if !tag_content.starts_with('/') {
                    let base_tag = tag_content.split_once(':').map(|(n, _)| n).unwrap_or(tag_content);
                    let end_tag = format!("[/{base_tag}]");
                    let tag_end_idx = cursor + close_bracket + 1;
                    if let Some(close_tag_idx) = text[tag_end_idx..].find(&end_tag) {
                        if !current_text.is_empty() {
                            spans.push(StyledSpan {
                                text: std::mem::take(&mut current_text),
                                effect: default_effect.map(String::from),
                                gradient: default_gradient.map(String::from),
                            });
                        }
                        let inner_text = &text[tag_end_idx..tag_end_idx + close_tag_idx];
                        let (effect, gradient) = if let Some((eff, grad)) = tag_content.split_once(':') {
                            (
                                canonical_effect_name(eff).map(String::from).or_else(|| Some(eff.to_string())),
                                Some(grad.to_string()),
                            )
                        } else {
                            (
                                canonical_effect_name(tag_content).map(String::from).or_else(|| Some(tag_content.to_string())),
                                default_gradient.map(String::from),
                            )
                        };
                        spans.push(StyledSpan {
                            text: inner_text.to_string(),
                            effect,
                            gradient,
                        });
                        cursor = tag_end_idx + close_tag_idx + end_tag.len();
                        continue;
                    }
                }
            }
        }
        let ch = text[cursor..].chars().next().unwrap();
        current_text.push(ch);
        cursor += ch.len_utf8();
    }

    if !current_text.is_empty() {
        spans.push(StyledSpan {
            text: current_text,
            effect: default_effect.map(String::from),
            gradient: default_gradient.map(String::from),
        });
    }

    if spans.is_empty() {
        spans.push(StyledSpan {
            text: text.to_string(),
            effect: default_effect.map(String::from),
            gradient: default_gradient.map(String::from),
        });
    }

    spans
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Hold,
    PushIn,
    PullOut,
    PanLeft,
    PanRight,
}

impl Motion {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "hold" => Ok(Self::Hold),
            "push_in" => Ok(Self::PushIn),
            "pull_out" => Ok(Self::PullOut),
            "pan_left" => Ok(Self::PanLeft),
            "pan_right" => Ok(Self::PanRight),
            other => Err(format!("unknown motion {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::PushIn => "push_in",
            Self::PullOut => "pull_out",
            Self::PanLeft => "pan_left",
            Self::PanRight => "pan_right",
        }
    }

    pub fn from_stored(text: &str) -> Self {
        Self::parse(text).unwrap_or(Self::Hold)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipOffer {
    pub start_s: f64,
    pub end_s: f64,
    pub segments: Vec<(f64, f64)>,
    pub crop: NormCrop,
    pub motion: Motion,
    pub framing: Framing,
    pub captions: Vec<Cue>,
    pub hooks: Vec<String>,
    pub scores: Scores,
    pub lesson: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scores {
    pub h: f64,
    pub v: f64,
    pub e: f64,
    pub a: f64,
    pub p: f64,
    pub s: f64,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    SampleFrames {
        start_s: f64,
        end_s: f64,
        every_s: f64,
        max_frames: u32,
    },
    Propose {
        moments: Vec<Moment>,
    },
    SetCrop(NormCrop),
    SetFraming(Framing),
    SetCaptions {
        cues: Vec<Cue>,
    },
    SetHooks {
        hooks: Vec<String>,
    },
    SetScores(Scores),
    Offer { clips: Vec<ClipOffer> },
    Done,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum RawFramingInput {
    Str(String),
    Obj(RawFramingObj),
}

#[derive(Deserialize, Debug, Clone, Default)]
struct RawFramingObj {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    framing: Option<String>,
    #[serde(default)]
    focus_x: Option<f64>,
    #[serde(default)]
    focus_y: Option<f64>,
    #[serde(default)]
    start_x: Option<f64>,
    #[serde(default)]
    end_x: Option<f64>,
    #[serde(default)]
    background: Option<String>,
    #[serde(default)]
    zoom: Option<f64>,
}

#[derive(Deserialize)]
struct RawAction {
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    function: Option<String>,
    start_s: Option<f64>,
    end_s: Option<f64>,
    every_s: Option<f64>,
    max_frames: Option<u32>,
    moments: Option<Vec<RawMoment>>,
    x: Option<f64>,
    y: Option<f64>,
    w: Option<f64>,
    h: Option<f64>,
    mode: Option<String>,
    focus_x: Option<f64>,
    focus_y: Option<f64>,
    start_x: Option<f64>,
    end_x: Option<f64>,
    background: Option<String>,
    zoom: Option<f64>,
    framing: Option<RawFramingInput>,
    cues: Option<Vec<RawCue>>,
    hooks: Option<Vec<String>>,
    v: Option<f64>,
    e: Option<f64>,
    a: Option<f64>,
    p: Option<f64>,
    note: Option<String>,
    clips: Option<Vec<RawClip>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawSegmentInput {
    Obj {
        #[serde(default)]
        start_s: f64,
        #[serde(default)]
        end_s: f64,
    },
    Pair(f64, f64),
}

impl RawSegmentInput {
    fn into_span(self) -> (f64, f64) {
        match self {
            Self::Obj { start_s, end_s } => (start_s, end_s),
            Self::Pair(start_s, end_s) => (start_s, end_s),
        }
    }
}

#[derive(Deserialize)]
struct RawClip {
    #[serde(default)]
    start_s: Option<f64>,
    #[serde(default)]
    end_s: Option<f64>,
    #[serde(default)]
    segments: Vec<RawSegmentInput>,
    #[serde(default)]
    crop: Option<NormCrop>,
    #[serde(default)]
    motion: String,
    #[serde(default)]
    framing: Option<RawFramingInput>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    focus_x: Option<f64>,
    #[serde(default)]
    focus_y: Option<f64>,
    #[serde(default)]
    start_x: Option<f64>,
    #[serde(default)]
    end_x: Option<f64>,
    #[serde(default)]
    background: Option<String>,
    #[serde(default)]
    zoom: Option<f64>,
    #[serde(default)]
    captions: Vec<RawCue>,
    #[serde(default)]
    hooks: Vec<String>,
    #[serde(default)]
    h: f64,
    #[serde(default)]
    v: f64,
    #[serde(default)]
    e: f64,
    #[serde(default)]
    a: f64,
    #[serde(default)]
    p: f64,
    #[serde(default)]
    note: String,
    #[serde(default)]
    lesson: String,
}

#[derive(Deserialize)]
struct RawMoment {
    start_s: f64,
    end_s: f64,
    #[serde(default)]
    why: String,
}

#[derive(Deserialize)]
struct RawCue {
    t: f64,
    text: String,
    #[serde(default)]
    effect: Option<String>,
    #[serde(default)]
    gradient: Option<String>,
    #[serde(default)]
    style: Option<String>,
}

pub fn is_done_reply(text: &str) -> bool {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    if lower == "done"
        || lower == "done."
        || lower == "offer nothing and done"
        || lower == "offer nothing and done."
        || lower == "offer nothing"
        || lower == "offer nothing."
        || lower.contains("offer nothing and done")
        || ((lower.contains("slides") || lower.contains("black frame") || lower.contains("black frames"))
            && (lower.contains("done") || lower.contains("nothing") || lower.contains("offer nothing")))
    {
        return true;
    }
    false
}

/// Strips internal chain-of-thought monologue from model outputs
/// (e.g. Gemma 4 `<|channel>thought ... <channel|>` or `<think> ... </think>`).
/// If the model is mid-thought (unclosed opening tag), returns an empty string
/// to signal that the final action is not yet ready.
pub fn strip_thought_content(text: &str) -> &str {
    let mut s = text;
    // Gemma 4 channel thought format
    if let Some(pos) = s.rfind("<channel|>") {
        s = &s[pos + "<channel|>".len()..];
    } else if s.contains("<|channel>thought") {
        return "";
    }

    // Standard <think> ... </think> tags
    if let Some(pos) = s.rfind("</think>") {
        s = &s[pos + "</think>".len()..];
    } else if s.contains("<think>") {
        return "";
    }

    // Additional <thought> or <|thought|> tags
    if let Some(pos) = s.rfind("</thought>") {
        s = &s[pos + "</thought>".len()..];
    } else if s.contains("<thought>") {
        return "";
    }
    if let Some(pos) = s.rfind("<|/thought|>") {
        s = &s[pos + "<|/thought|>".len()..];
    } else if s.contains("<|thought|>") {
        return "";
    }

    s
}

pub fn parse_action(text: &str) -> Result<Action, String> {
    let text = strip_thought_content(text);
    if text.trim().is_empty() {
        return Err("no json object".to_string());
    }
    let mut candidates: Vec<&str> = Vec::new();
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if start <= end {
            candidates.push(&text[start..=end]);
        }
    }
    for cand in json_candidates(text) {
        if !candidates.contains(&cand) {
            candidates.push(cand);
        }
    }
    if candidates.is_empty() {
        if is_done_reply(text) {
            return Ok(Action::Done);
        }
        return Err("no json object".to_string());
    }

    let mut last_err = String::from("no json object");

    // The widest span first, so an extra } inside an array does not hide the rest.
    for cand in &candidates {
        if let Ok(action) = try_parse_raw_json(cand) {
            return Ok(action);
        }
        let sanitized = sanitize_json(cand);
        match try_parse_raw_json(&sanitized) {
            Ok(action) => return Ok(action),
            Err(e) => last_err = e,
        }
    }

    Err(last_err)
}

fn try_parse_raw_json(json: &str) -> Result<Action, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|err| err.to_string())?;
    let value = normalize_action(value);
    let raw: RawAction = serde_json::from_value(value).map_err(|err| err.to_string())?;
    let act_name = raw
        .action
        .or(raw.name)
        .or(raw.tool)
        .or(raw.function)
        .unwrap_or_else(|| {
            if raw.clips.is_some() {
                "offer".to_string()
            } else if raw.moments.is_some() {
                "propose".to_string()
            } else if raw.start_s.is_some() || raw.max_frames.is_some() {
                "sample_frames".to_string()
            } else if raw.cues.is_some() {
                "set_captions".to_string()
            } else if raw.framing.is_some() || raw.mode.is_some() || raw.focus_x.is_some() || raw.start_x.is_some() {
                "set_framing".to_string()
            } else if raw.x.is_some() || raw.w.is_some() {
                "set_crop".to_string()
            } else if raw.hooks.is_some() {
                "set_hooks".to_string()
            } else if raw.note.is_some() || raw.v.is_some() {
                "set_scores".to_string()
            } else {
                String::new()
            }
        });

    let action = match act_name.as_str() {
        "sample_frames" | "inspect" => Action::SampleFrames {
            start_s: raw.start_s.unwrap_or(0.0),
            end_s: raw.end_s.unwrap_or(0.0),
            every_s: raw.every_s.unwrap_or(1.0),
            max_frames: raw.max_frames.unwrap_or(8).clamp(1, 8),
        },
        "propose" => Action::Propose {
            moments: raw
                .moments
                .unwrap_or_default()
                .into_iter()
                .take(3)
                .map(|m| Moment {
                    start_s: m.start_s,
                    end_s: m.end_s,
                    why: m.why,
                })
                .collect(),
        },
        "set_crop" => Action::SetCrop(NormCrop {
            x: raw.x.unwrap_or(f64::NAN),
            y: raw.y.unwrap_or(f64::NAN),
            w: raw.w.unwrap_or(f64::NAN),
            h: raw.h.unwrap_or(f64::NAN),
        }),
        "set_framing" => {
            let framing = resolve_framing(
                raw.framing.as_ref(),
                raw.mode.as_deref(),
                raw.focus_x,
                raw.focus_y,
                raw.start_x,
                raw.end_x,
                raw.background.as_deref(),
                raw.zoom,
                None,
                None,
            );
            Action::SetFraming(framing)
        }
        "set_captions" => Action::SetCaptions {
            cues: raw
                .cues
                .unwrap_or_default()
                .into_iter()
                .map(|c| {
                    let effect = c.effect.or(c.style);
                    let (effect, gradient) = normalize_effect_and_gradient(effect, c.gradient);
                    Cue {
                        t: c.t,
                        text: c.text,
                        effect,
                        gradient,
                    }
                })
                .collect(),
        },
        "set_hooks" => Action::SetHooks {
            hooks: raw.hooks.unwrap_or_default(),
        },
        "offer" => {
            let clips = raw.clips.unwrap_or_default();
            if clips.len() > 5 {
                return Err("too many clips".into());
            }
            Action::Offer {
                clips: clips.into_iter().map(parse_clip).collect::<Result<Vec<_>, _>>()?,
            }
        }
        "set_scores" => {
            let h = clamp_term(raw.h.unwrap_or(0.0));
            let v = clamp_term(raw.v.unwrap_or(0.0));
            let e = clamp_term(raw.e.unwrap_or(0.0));
            let a = clamp_term(raw.a.unwrap_or(0.0));
            let p = clamp_term(raw.p.unwrap_or(0.0));
            Action::SetScores(Scores {
                h,
                v,
                e,
                a,
                p,
                s: score::combine(h, v, e, a, p),
                note: crate::pet_state::truncate_line(raw.note.as_deref().unwrap_or("")),
            })
        }
        "done" | "offer_nothing" | "none" => Action::Done,
        other => return Err(format!("unknown action {other}")),
    };
    validate(&action)?;
    Ok(action)
}

pub fn validate(action: &Action) -> Result<(), String> {
    match action {
        Action::Propose { moments } => {
            for moment in moments {
                let span = moment.end_s - moment.start_s;
                if span > 60.0 {
                    return Err("window over 60s".into());
                }
                if span <= 0.0 {
                    return Err("empty window".into());
                }
            }
            Ok(())
        }
        Action::SetCrop(crop) => {
            for value in [crop.x, crop.y, crop.w, crop.h] {
                if !(0.0..=1.0).contains(&value) {
                    return Err("crop outside 0-1".into());
                }
            }
            if crop.w <= 0.0 || crop.h <= 0.0 || crop.x + crop.w > 1.0 + 1e-6 || crop.y + crop.h > 1.0 + 1e-6
            {
                return Err("crop outside 0-1".into());
            }
            Ok(())
        }
        Action::SetFraming(framing) => validate_framing(framing),
        Action::SetCaptions { cues } => {
            for cue in cues {
                if cue.word_count() > 4 {
                    return Err("caption over 4 words".into());
                }
            }
            Ok(())
        }
        Action::SampleFrames { end_s, start_s, every_s, .. } => {
            if end_s < start_s || *every_s <= 0.0 {
                return Err("bad sample range".into());
            }
            Ok(())
        }
        Action::SetHooks { hooks } => {
            if hooks.is_empty() || hooks.len() > 2 {
                return Err("need one or two hooks".into());
            }
            Ok(())
        }
        Action::SetScores(_) | Action::Done => Ok(()),
        Action::Offer { clips } => {
            for clip in clips {
                let total_dur: f64 = if !clip.segments.is_empty() {
                    clip.segments.iter().map(|(s, e)| e - s).sum()
                } else {
                    clip.end_s - clip.start_s
                };
                if total_dur > 60.0 {
                    return Err("window over 60s".into());
                }
                if total_dur <= 0.0 {
                    return Err("empty window".into());
                }
                for &(s, e) in &clip.segments {
                    if e <= s {
                        return Err("invalid segment range".into());
                    }
                }
                validate(&Action::SetCrop(clip.crop.clone()))?;
                validate_framing(&clip.framing)?;
                validate(&Action::SetCaptions {
                    cues: clip.captions.clone(),
                })?;
                validate(&Action::SetHooks {
                    hooks: clip.hooks.clone(),
                })?;
            }
            Ok(())
        }
    }
}

pub fn validate_framing(framing: &Framing) -> Result<(), String> {
    match framing {
        Framing::Fit { .. } => Ok(()),
        Framing::Crop { focus_x, focus_y, zoom } => {
            for v in [*focus_x, *focus_y, *zoom] {
                if v.is_nan() || v.is_infinite() {
                    return Err("invalid coordinate in crop framing".into());
                }
            }
            if *zoom <= 0.0 || *zoom > 2.0 {
                return Err("invalid zoom in crop framing".into());
            }
            Ok(())
        }
        Framing::CropAndMove { start_x, end_x, focus_y, zoom } => {
            for v in [*start_x, *end_x, *focus_y, *zoom] {
                if v.is_nan() || v.is_infinite() {
                    return Err("invalid coordinate in crop_and_move framing".into());
                }
            }
            if *zoom <= 0.0 || *zoom > 2.0 {
                return Err("invalid zoom in crop_and_move framing".into());
            }
            Ok(())
        }
    }
}

fn resolve_framing(
    input: Option<&RawFramingInput>,
    direct_mode: Option<&str>,
    focus_x: Option<f64>,
    focus_y: Option<f64>,
    start_x: Option<f64>,
    end_x: Option<f64>,
    background: Option<&str>,
    zoom: Option<f64>,
    fallback_crop: Option<&NormCrop>,
    fallback_motion: Option<Motion>,
) -> Framing {
    let (mode_str, fx, fy, sx, ex, bg_str, z) = match input {
        Some(RawFramingInput::Str(s)) => (Some(s.as_str()), focus_x, focus_y, start_x, end_x, background, zoom),
        Some(RawFramingInput::Obj(obj)) => (
            obj.mode.as_deref().or(obj.framing.as_deref()).or(direct_mode),
            obj.focus_x.or(focus_x),
            obj.focus_y.or(focus_y),
            obj.start_x.or(start_x),
            obj.end_x.or(end_x),
            obj.background.as_deref().or(background),
            obj.zoom.or(zoom),
        ),
        None => (direct_mode, focus_x, focus_y, start_x, end_x, background, zoom),
    };

    if let Some(mode) = mode_str {
        match mode.trim().to_lowercase().as_str() {
            "fit" | "letterbox" | "pad" => {
                let bg = match bg_str.unwrap_or("blur").to_lowercase().as_str() {
                    "black" | "solid" => BackgroundStyle::Black,
                    _ => BackgroundStyle::Blur,
                };
                return Framing::Fit { background: bg };
            }
            "crop" | "static" | "center" => {
                return Framing::Crop {
                    focus_x: fx.unwrap_or(0.5).clamp(0.0, 1.0),
                    focus_y: fy.unwrap_or(0.5).clamp(0.0, 1.0),
                    zoom: z.unwrap_or(1.0).clamp(0.1, 1.0),
                };
            }
            "crop_and_move" | "move" | "pan" | "pan_scan" | "crop_move" => {
                return Framing::CropAndMove {
                    start_x: sx.unwrap_or(0.2).clamp(0.0, 1.0),
                    end_x: ex.unwrap_or(0.8).clamp(0.0, 1.0),
                    focus_y: fy.unwrap_or(0.5).clamp(0.0, 1.0),
                    zoom: z.unwrap_or(1.0).clamp(0.1, 1.0),
                };
            }
            _ => {}
        }
    }

    if let Some(motion) = fallback_motion {
        match motion {
            Motion::PanLeft => {
                return Framing::CropAndMove {
                    start_x: 0.8,
                    end_x: 0.2,
                    focus_y: 0.5,
                    zoom: 1.0,
                };
            }
            Motion::PanRight => {
                return Framing::CropAndMove {
                    start_x: 0.2,
                    end_x: 0.8,
                    focus_y: 0.5,
                    zoom: 1.0,
                };
            }
            Motion::PushIn => {
                return Framing::Crop {
                    focus_x: 0.5,
                    focus_y: 0.5,
                    zoom: 0.85,
                };
            }
            Motion::PullOut => {
                return Framing::Crop {
                    focus_x: 0.5,
                    focus_y: 0.5,
                    zoom: 1.0,
                };
            }
            Motion::Hold => {}
        }
    }

    if let Some(crop) = fallback_crop {
        if crop.w < 0.85 || crop.h < 0.85 {
            return Framing::Crop {
                focus_x: (crop.x + crop.w * 0.5).clamp(0.0, 1.0),
                focus_y: (crop.y + crop.h * 0.5).clamp(0.0, 1.0),
                zoom: crop.h.clamp(0.1, 1.0),
            };
        }
    }

    Framing::Fit {
        background: BackgroundStyle::Blur,
    }
}

pub fn clamp_norm_crop(crop: &mut NormCrop) {
    if crop.x.is_nan() {
        crop.x = 0.0;
    }
    if crop.y.is_nan() {
        crop.y = 0.0;
    }
    if crop.w.is_nan() || crop.w <= 0.0 {
        crop.w = 1.0;
    }
    if crop.h.is_nan() || crop.h <= 0.0 {
        crop.h = 1.0;
    }

    // If coordinates appear to be percentages (e.g. 0..100) or pixels (> 1.0)
    if crop.w > 1.0 || crop.h > 1.0 || crop.x > 1.0 || crop.y > 1.0 {
        if crop.w <= 100.0 && crop.h <= 100.0 && crop.x <= 100.0 && crop.y <= 100.0 {
            crop.x /= 100.0;
            crop.y /= 100.0;
            crop.w /= 100.0;
            crop.h /= 100.0;
        } else {
            let max_dim = crop.x.max(crop.y).max(crop.w).max(crop.h);
            if max_dim > 1.0 {
                crop.x /= max_dim;
                crop.y /= max_dim;
                crop.w /= max_dim;
                crop.h /= max_dim;
            }
        }
    }
    crop.x = crop.x.clamp(0.0, 0.95);
    crop.y = crop.y.clamp(0.0, 0.95);
    crop.w = crop.w.clamp(0.05, 1.0);
    crop.h = crop.h.clamp(0.05, 1.0);
    if crop.x + crop.w > 1.0 {
        crop.w = (1.0 - crop.x).max(0.05);
    }
    if crop.y + crop.h > 1.0 {
        crop.h = (1.0 - crop.y).max(0.05);
    }
}

fn parse_clip(raw: RawClip) -> Result<ClipOffer, String> {
    let motion = Motion::parse(&raw.motion).unwrap_or(Motion::Hold);
    let framing = resolve_framing(
        raw.framing.as_ref(),
        raw.mode.as_deref(),
        raw.focus_x,
        raw.focus_y,
        raw.start_x,
        raw.end_x,
        raw.background.as_deref(),
        raw.zoom,
        raw.crop.as_ref(),
        Some(motion),
    );
    let mut crop = raw.crop.unwrap_or_else(|| framing.to_norm_crop(0.0));
    clamp_norm_crop(&mut crop);
    let h = clamp_term(raw.h);
    let v = clamp_term(raw.v);
    let e = clamp_term(raw.e);
    let a = clamp_term(raw.a);
    let p = clamp_term(raw.p);
    let hooks = if raw.hooks.is_empty() {
        vec!["moment".into()]
    } else {
        raw.hooks
            .into_iter()
            .take(2)
            .map(|hook| hook.chars().take(42).collect())
            .collect()
    };
    let mut segments: Vec<(f64, f64)> = raw.segments.into_iter().map(|s| s.into_span()).collect();
    for seg in &mut segments {
        if seg.1 <= seg.0 {
            seg.1 = seg.0 + 1.0;
        }
    }
    if segments.is_empty() {
        let s = raw.start_s.unwrap_or(0.0);
        let e = raw.end_s.unwrap_or(s + 15.0);
        let e = if e <= s { s + 15.0 } else { e };
        segments.push((s, e));
    }
    let start_s = segments.first().map(|s| s.0).unwrap_or(0.0);
    let end_s = segments.last().map(|s| s.1).unwrap_or(start_s + 15.0);
    let captions = raw
        .captions
        .into_iter()
        .map(|cue| {
            let effect = cue.effect.or(cue.style);
            let (effect, gradient) = normalize_effect_and_gradient(effect, cue.gradient);
            let words: Vec<&str> = cue.text.split_whitespace().collect();
            let text = if words.len() > 4 {
                words[..4].join(" ")
            } else {
                cue.text
            };
            Cue {
                t: cue.t,
                text,
                effect,
                gradient,
            }
        })
        .collect();
    Ok(ClipOffer {
        start_s,
        end_s,
        segments,
        crop,
        motion,
        framing,
        captions,
        hooks,
        scores: Scores {
            h,
            v,
            e,
            a,
            p,
            s: score::combine(h, v, e, a, p),
            note: crate::pet_state::truncate_line(&raw.note),
        },
        lesson: crate::pet_state::truncate_lesson(&raw.lesson),
    })
}

/// How many clips a source of this length may offer.
pub fn offer_count(duration_s: f64) -> usize {
    if duration_s < 120.0 {
        3
    } else if duration_s < 600.0 {
        4
    } else {
        5
    }
}

/// Models often wrap a call, nest `scores`, or put a string field in an object.
fn normalize_action(value: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(mut obj) = value else {
        return value;
    };
    lift_call_object(&mut obj, "function");
    lift_call_object(&mut obj, "tool");
    lift_call_object(&mut obj, "action");
    lift_call_object(&mut obj, "name");
    if let Some(serde_json::Value::Object(args)) = obj.remove("arguments") {
        for (key, val) in args {
            obj.entry(key).or_insert(val);
        }
    }
    if let Some(clips) = obj.get_mut("clips").and_then(|clips| clips.as_array_mut()) {
        for clip in clips.iter_mut() {
            *clip = normalize_clip(std::mem::take(clip));
        }
    }
    if let Some(moments) = obj.get_mut("moments").and_then(|moments| moments.as_array_mut()) {
        for moment in moments.iter_mut() {
            if let serde_json::Value::Object(moment) = moment {
                coerce_string_field(moment, "why");
            }
        }
    }
    if let Some(hooks) = obj.get_mut("hooks") {
        *hooks = normalize_hooks(std::mem::take(hooks));
    }
    if let Some(cues) = obj.get_mut("cues") {
        *cues = normalize_captions(std::mem::take(cues));
    }
    coerce_string_field(&mut obj, "note");
    serde_json::Value::Object(obj)
}

fn lift_call_object(obj: &mut serde_json::Map<String, serde_json::Value>, key: &str) {
    let Some(serde_json::Value::Object(inner)) = obj.get(key).cloned() else {
        return;
    };
    let name = ["name", "action", "tool"]
        .into_iter()
        .find_map(|field| inner.get(field).and_then(as_text));
    let args = inner
        .get("arguments")
        .cloned()
        .or_else(|| inner.get("parameters").cloned())
        .or_else(|| inner.get("args").cloned());
    if let Some(name) = name.clone() {
        obj.insert(key.to_string(), serde_json::Value::String(name.clone()));
        if key != "action" && !obj.get("action").is_some_and(|value| value.is_string()) {
            obj.insert("action".into(), serde_json::Value::String(name));
        }
    } else {
        obj.remove(key);
    }
    let mut extras = inner;
    for drop in ["name", "action", "tool", "arguments", "parameters", "args"] {
        extras.remove(drop);
    }
    for (key, val) in extras {
        obj.entry(key).or_insert(val);
    }
    if let Some(serde_json::Value::Object(args)) = args {
        for (key, val) in args {
            obj.entry(key).or_insert(val);
        }
    }
}

fn normalize_clip(value: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(mut obj) = value else {
        return value;
    };
    if let Some(serde_json::Value::Object(scores)) = obj.remove("scores") {
        for key in ["h", "v", "e", "a", "p", "note", "lesson"] {
            if !obj.contains_key(key) {
                if let Some(val) = scores.get(key).cloned() {
                    obj.insert(key.to_string(), val);
                }
            }
        }
    }
    if let Some(motion) = obj.get("motion").cloned() {
        if let Some(text) = motion_text(&motion) {
            obj.insert("motion".into(), serde_json::Value::String(text));
        }
    }
    coerce_string_field(&mut obj, "note");
    coerce_string_field(&mut obj, "lesson");
    if !obj.contains_key("hooks") {
        if let Some(hook) = obj.get("hook").cloned() {
            obj.insert("hooks".into(), hook);
        }
    }
    if let Some(hooks) = obj.get_mut("hooks") {
        *hooks = normalize_hooks(std::mem::take(hooks));
    }
    if let Some(captions) = obj.get_mut("captions") {
        *captions = normalize_captions(std::mem::take(captions));
    }
    if let Some(segments) = obj.get_mut("segments") {
        *segments = normalize_segments(std::mem::take(segments));
    }
    serde_json::Value::Object(obj)
}

fn normalize_segments(value: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Array(items) = value else {
        return value;
    };
    serde_json::Value::Array(
        items
            .into_iter()
            .map(|item| {
                let serde_json::Value::Object(mut obj) = item else {
                    return item;
                };
                if !obj.contains_key("start_s") {
                    if let Some(start) = obj.get("start").cloned().or_else(|| obj.get("from").cloned()) {
                        obj.insert("start_s".into(), start);
                    }
                }
                if !obj.contains_key("end_s") {
                    if let Some(end) = obj.get("end").cloned().or_else(|| obj.get("to").cloned()) {
                        obj.insert("end_s".into(), end);
                    }
                }
                serde_json::Value::Object(obj)
            })
            .collect(),
    )
}

fn normalize_hooks(value: serde_json::Value) -> serde_json::Value {
    let mut hooks = hook_strings(&value);
    hooks.truncate(2);
    serde_json::Value::Array(hooks.into_iter().map(serde_json::Value::String).collect())
}

fn hook_strings(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                Vec::new()
            } else {
                vec![text.to_string()]
            }
        }
        serde_json::Value::Array(items) => items.iter().flat_map(hook_strings).collect(),
        serde_json::Value::Object(map) => {
            if let Some(text) = ["text", "hook", "line", "value", "content"]
                .into_iter()
                .find_map(|key| map.get(key).and_then(as_text))
            {
                hook_strings(&serde_json::Value::String(text))
            } else {
                map.values().flat_map(hook_strings).collect()
            }
        }
        _ => Vec::new(),
    }
}

fn normalize_captions(value: serde_json::Value) -> serde_json::Value {
    let items = match value {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(_) | serde_json::Value::String(_) => vec![value],
        _ => return serde_json::Value::Array(Vec::new()),
    };
    let cues = items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            serde_json::Value::String(text) => Some(serde_json::json!({ "t": index as f64, "text": text })),
            serde_json::Value::Object(mut obj) => {
                if !obj.contains_key("t") {
                    let t = obj
                        .get("time")
                        .cloned()
                        .or_else(|| obj.get("start").cloned())
                        .or_else(|| obj.get("start_s").cloned())
                        .unwrap_or_else(|| serde_json::json!(index as f64));
                    obj.insert("t".into(), t);
                }
                let text = obj.get("text").cloned().and_then(|value| as_text(&value)).or_else(|| {
                    ["caption", "content", "line", "value"]
                        .into_iter()
                        .find_map(|key| obj.get(key).and_then(as_text))
                });
                let Some(text) = text else {
                    return None;
                };
                obj.insert("text".into(), serde_json::Value::String(text));

                let effect_opt = obj.get("effect").cloned().or_else(|| obj.get("style").cloned()).and_then(|v| as_text(&v));
                let gradient_opt = obj.get("gradient").cloned().and_then(|v| as_text(&v));
                let (norm_effect, norm_gradient) = normalize_effect_and_gradient(effect_opt, gradient_opt);
                if let Some(eff) = norm_effect {
                    obj.insert("effect".into(), serde_json::Value::String(eff));
                }
                if let Some(grad) = norm_gradient {
                    obj.insert("gradient".into(), serde_json::Value::String(grad));
                }

                Some(serde_json::Value::Object(obj))
            }
            _ => None,
        })
        .collect();
    serde_json::Value::Array(cues)
}

fn coerce_string_field(obj: &mut serde_json::Map<String, serde_json::Value>, key: &str) {
    let Some(value) = obj.get(key) else {
        return;
    };
    if value.is_string() || value.is_null() {
        return;
    }
    match as_text(value) {
        Some(text) => {
            obj.insert(key.to_string(), serde_json::Value::String(text));
        }
        None => {
            obj.remove(key);
        }
    }
}

fn motion_text(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = as_text(value) {
        if Motion::parse(&text).is_ok() {
            return Some(text);
        }
    }
    if let serde_json::Value::Object(map) = value {
        for nested in map.values() {
            if let Some(text) = nested.as_str() {
                if Motion::parse(text).is_ok() {
                    return Some(text.to_string());
                }
            }
        }
    }
    as_text(value)
}

fn as_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(as_text).collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        serde_json::Value::Object(map) => {
            for key in [
                "text",
                "hook",
                "note",
                "lesson",
                "sentence",
                "content",
                "value",
                "caption",
                "label",
                "description",
                "motion",
                "name",
                "type",
                "kind",
            ] {
                if let Some(text) = map.get(key).and_then(as_text) {
                    if !text.trim().is_empty() {
                        return Some(text);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

pub fn json_candidates(text: &str) -> Vec<&str> {
    let mut candidates = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut start = None;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;

    for i in 0..len {
        let byte = bytes[i];
        if in_string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        if let Some(s) = start {
                            candidates.push(&text[s..=i]);
                        }
                        start = None;
                    }
                }
            }
            _ => {}
        }
    }
    candidates
}

#[derive(Clone, Copy, PartialEq)]
enum JsonFrame {
    Object,
    Array,
}

#[derive(Clone, Copy, PartialEq)]
enum JsonExpect {
    Key,
    Colon,
    Value,
    CommaOrEnd,
}

pub fn sanitize_json(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut in_str = false;
    let mut escape = false;
    let mut stack: Vec<(JsonFrame, JsonExpect)> = Vec::new();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        let c = chars[i];
        if in_str {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_str = false;
                if let Some((_, expect)) = stack.last_mut() {
                    *expect = if *expect == JsonExpect::Key {
                        JsonExpect::Colon
                    } else {
                        JsonExpect::CommaOrEnd
                    };
                }
            }
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            out.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while j < len && chars[j].is_whitespace() {
                j += 1;
            }
            if j < len && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
            if let Some((frame, expect)) = stack.last_mut() {
                if *expect == JsonExpect::CommaOrEnd {
                    *expect = if *frame == JsonFrame::Object {
                        JsonExpect::Key
                    } else {
                        JsonExpect::Value
                    };
                }
            }
            out.push(c);
            i += 1;
            continue;
        }
        if let Some((frame, expect)) = stack.last().copied() {
            let value_start = matches!(c, '"' | '{' | '[' | '-' | 't' | 'f' | 'n') || c.is_ascii_digit();
            if expect == JsonExpect::CommaOrEnd && value_start {
                out.push(',');
                let next = if frame == JsonFrame::Object {
                    JsonExpect::Key
                } else {
                    JsonExpect::Value
                };
                if let Some((_, slot)) = stack.last_mut() {
                    *slot = next;
                }
                continue;
            }
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '{' => {
                stack.push((JsonFrame::Object, JsonExpect::Key));
                out.push(c);
            }
            '[' => {
                stack.push((JsonFrame::Array, JsonExpect::Value));
                out.push(c);
            }
            '}' => {
                if matches!(stack.last(), Some((JsonFrame::Array, _))) {
                    match after_closer(&chars, i) {
                        AfterCloser::End | AfterCloser::Closer => {
                            close_open_arrays(&mut stack, &mut out);
                            if matches!(stack.last(), Some((JsonFrame::Object, _))) {
                                stack.pop();
                                out.push('}');
                                if let Some((_, expect)) = stack.last_mut() {
                                    *expect = JsonExpect::CommaOrEnd;
                                }
                            }
                        }
                        AfterCloser::ObjectKey => close_open_arrays(&mut stack, &mut out),
                        AfterCloser::MoreValues => {}
                    }
                    i += 1;
                    continue;
                }
                if stack.is_empty() {
                    i += 1;
                    continue;
                }
                stack.pop();
                out.push('}');
                if let Some((_, expect)) = stack.last_mut() {
                    *expect = JsonExpect::CommaOrEnd;
                }
            }
            ']' => {
                if stack.is_empty() {
                    i += 1;
                    continue;
                }
                stack.pop();
                out.push(']');
                if let Some((_, expect)) = stack.last_mut() {
                    *expect = JsonExpect::CommaOrEnd;
                }
            }
            ':' => {
                out.push(c);
                if let Some((JsonFrame::Object, expect)) = stack.last_mut() {
                    *expect = JsonExpect::Value;
                }
            }
            _ if c == '-' || c.is_ascii_digit() => {
                out.push(c);
                i += 1;
                while i < len && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | 'e' | 'E' | '+' | '-')) {
                    out.push(chars[i]);
                    i += 1;
                }
                if let Some((_, expect)) = stack.last_mut() {
                    *expect = JsonExpect::CommaOrEnd;
                }
                continue;
            }
            't' | 'f' | 'n' => {
                let word = if c == 't' {
                    "true"
                } else if c == 'f' {
                    "false"
                } else {
                    "null"
                };
                if chars[i..].starts_with(word.chars().collect::<Vec<_>>().as_slice()) {
                    out.push_str(word);
                    i += word.len();
                    if let Some((_, expect)) = stack.last_mut() {
                        *expect = JsonExpect::CommaOrEnd;
                    }
                    continue;
                }
                out.push(c);
            }
            _ => {
                let between = match stack.last() {
                    None => true,
                    Some((_, expect)) => *expect == JsonExpect::CommaOrEnd,
                };
                if between {
                    i += 1;
                    continue;
                }
                out.push(c);
            }
        }
        i += 1;
    }
    out
}

enum AfterCloser {
    End,
    Closer,
    ObjectKey,
    MoreValues,
}

/// What follows a `}` that landed inside an array.
fn after_closer(chars: &[char], i: usize) -> AfterCloser {
    let mut j = i + 1;
    loop {
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if j >= chars.len() {
            return AfterCloser::End;
        }
        if matches!(chars[j], '.' | ';' | '!' | '?') {
            j += 1;
            continue;
        }
        break;
    }
    match chars[j] {
        '}' | ']' => AfterCloser::Closer,
        ',' => after_comma(chars, j),
        '"' if string_is_object_key(chars, j) => AfterCloser::ObjectKey,
        _ => AfterCloser::MoreValues,
    }
}

fn after_comma(chars: &[char], comma: usize) -> AfterCloser {
    let mut j = comma + 1;
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    if j >= chars.len() {
        return AfterCloser::End;
    }
    if chars[j] == '"' && string_is_object_key(chars, j) {
        AfterCloser::ObjectKey
    } else {
        AfterCloser::MoreValues
    }
}

fn string_is_object_key(chars: &[char], start: usize) -> bool {
    if chars.get(start) != Some(&'"') {
        return false;
    }
    let mut k = start + 1;
    let mut escape = false;
    while k < chars.len() {
        let c = chars[k];
        if escape {
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else if c == '"' {
            k += 1;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            return chars.get(k) == Some(&':');
        }
        k += 1;
    }
    false
}

fn close_open_arrays(stack: &mut Vec<(JsonFrame, JsonExpect)>, out: &mut String) {
    while matches!(stack.last(), Some((JsonFrame::Array, _))) {
        stack.pop();
        out.push(']');
    }
    if let Some((_, expect)) = stack.last_mut() {
        *expect = JsonExpect::CommaOrEnd;
    }
}

pub fn json_object(text: &str) -> Option<&str> {
    let candidates = json_candidates(text);
    if candidates.is_empty() {
        return None;
    }
    for &cand in &candidates {
        if cand.contains("\"action\"") || cand.contains("\"clips\"") || cand.contains("\"moments\"") {
            return Some(cand);
        }
    }
    Some(candidates[0])
}

pub fn window_too_long(start_s: f64, end_s: f64) -> bool {
    end_s - start_s > 60.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_window_over_60s() {
        let raw = r#"{"action":"propose","moments":[{"start_s":0,"end_s":90,"why":"late"}]}"#;
        let err = parse_action(raw).unwrap_err();
        assert!(err.contains("60"));
    }

    #[test]
    fn rejects_a_crop_outside_the_frame() {
        let raw = r#"{"action":"set_crop","x":1.2,"y":0.1,"w":0.4,"h":0.8}"#;
        assert!(parse_action(raw).unwrap_err().contains("crop"));
    }

    #[test]
    fn rejects_a_caption_over_four_words() {
        let raw = r#"{"action":"set_captions","cues":[{"t":0.4,"text":"this is five words now"}]}"#;
        assert!(parse_action(raw).unwrap_err().contains("caption"));
    }

    #[test]
    fn stops_at_the_closing_brace() {
        let raw = "sure {\"action\":\"done\"} and more";
        assert_eq!(parse_action(raw).unwrap(), Action::Done);
    }

    #[test]
    fn offer_count_follows_duration() {
        assert_eq!(offer_count(30.0), 3);
        assert_eq!(offer_count(119.0), 3);
        assert_eq!(offer_count(120.0), 4);
        assert_eq!(offer_count(599.0), 4);
        assert_eq!(offer_count(600.0), 5);
        assert_eq!(offer_count(3_600.0), 5);
    }

    #[test]
    fn rejects_a_sixth_clip_and_a_90s_window() {
        let one = clip_json(0.0, 20.0);
        let six = format!(r#"{{"action":"offer","clips":[{one},{one},{one},{one},{one},{one}]}}"#);
        assert!(parse_action(&six).unwrap_err().contains("too many"));
        let long = format!(r#"{{"action":"offer","clips":[{}]}}"#, clip_json(0.0, 90.0));
        assert!(parse_action(&long).unwrap_err().contains("60"));
    }

    #[test]
    fn parses_action_inside_markdown_block() {
        let text = "Here is the plan:\n```json\n{\"action\":\"sample_frames\",\"start_s\":0.0,\"end_s\":10.0}\n```\nHope this helps!";
        let action = parse_action(text).unwrap();
        match action {
            Action::SampleFrames { start_s, end_s, .. } => {
                assert_eq!(start_s, 0.0);
                assert_eq!(end_s, 10.0);
            }
            _ => panic!("expected sample_frames"),
        }
    }

    #[test]
    fn a_thought_only_reply_has_no_json_object() {
        let err = parse_action("<think>The frames are busy and I should look again.</think>").unwrap_err();
        assert_eq!(err, "no json object");
    }

    #[test]
    fn parses_action_with_thinking_and_internal_braces() {
        let text = "<think>I need to check {start: 0} and {end: 10}</think>\n{\"action\":\"done\"}";
        assert_eq!(parse_action(text).unwrap(), Action::Done);
    }

    #[test]
    fn parses_action_with_trailing_commas() {
        let text = "{\"action\":\"sample_frames\",\"start_s\":1.0,\"end_s\":5.0,}";
        let action = parse_action(text).unwrap();
        match action {
            Action::SampleFrames { start_s, end_s, .. } => {
                assert_eq!(start_s, 1.0);
                assert_eq!(end_s, 5.0);
            }
            _ => panic!("expected sample_frames"),
        }
    }

    #[test]
    fn parses_tool_call_style_action() {
        let text = "{\"name\":\"sample_frames\",\"start_s\":2.0,\"end_s\":8.0}";
        let action = parse_action(text).unwrap();
        match action {
            Action::SampleFrames { start_s, end_s, .. } => {
                assert_eq!(start_s, 2.0);
                assert_eq!(end_s, 8.0);
            }
            _ => panic!("expected sample_frames"),
        }
    }

    #[test]
    fn parses_offer_when_string_fields_arrive_as_objects() {
        let text = r#"{
            "action": "offer",
            "clips": [{
                "segments": [{"start_s": 10.0, "end_s": 14.0}, {"start": 20.0, "end": 24.0}],
                "crop": {"x": 0.1, "y": 0.0, "w": 0.8, "h": 1.0},
                "motion": {"type": "camera", "move": "push_in"},
                "hooks": [{"text": "wait for it"}, {"hook": "look closer"}],
                "captions": [{"t": 0.5, "text": {"content": "look here"}}],
                "scores": {"h": 85, "v": 90, "e": 75, "a": 80, "p": 70, "note": {"sentence": "Opens on a face."}},
                "lesson": {"text": "Three sections combined for fast pacing."}
            }]
        }"#;
        let action = parse_action(text).unwrap();
        match action {
            Action::Offer { clips } => {
                let clip = &clips[0];
                assert_eq!(clip.motion, Motion::PushIn);
                assert_eq!(clip.hooks, vec!["wait for it".to_string(), "look closer".to_string()]);
                assert_eq!(clip.captions[0].text, "look here");
                assert_eq!(clip.scores.h, 85.0);
                assert_eq!(clip.scores.v, 90.0);
                assert!(clip.scores.note.contains("Opens on a face"));
                assert!(clip.lesson.contains("Three sections"));
                assert_eq!(clip.segments[1], (20.0, 24.0));
            }
            other => panic!("expected offer, got {other:?}"),
        }
    }

    #[test]
    fn parses_function_call_envelope() {
        let text = r#"{"function":{"name":"done","arguments":{}}}"#;
        assert_eq!(parse_action(text).unwrap(), Action::Done);
    }

    #[test]
    fn extra_hooks_are_clamped_to_two() {
        let text = r#"{
            "action": "offer",
            "clips": [{
                "start_s": 1.0,
                "end_s": 20.0,
                "crop": {"x": 0.2, "y": 0.1, "w": 0.4, "h": 0.8},
                "motion": "hold",
                "captions": [{"t": 0.4, "text": "the good part"}],
                "hooks": ["first line", "second line", "third line"],
                "h": 80, "v": 80, "e": 70, "a": 70, "p": 70,
                "note": "Opens on a face.",
                "lesson": "The opening is the claim."
            }]
        }"#;
        match parse_action(text).unwrap() {
            Action::Offer { clips } => {
                assert_eq!(clips[0].hooks, vec!["first line".to_string(), "second line".to_string()]);
            }
            other => panic!("expected offer, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_clips_bracket_still_parses() {
        let one = clip_json(1.0, 16.0);
        let two = clip_json(20.0, 36.0);
        let text = format!(r#"{{"action":"offer","clips":[{one},{two}}}"#);
        match parse_action(&text).unwrap() {
            Action::Offer { clips } => assert_eq!(clips.len(), 2),
            other => panic!("expected offer, got {other:?}"),
        }
    }

    #[test]
    fn a_period_after_the_example_clip_does_not_reject_the_offer() {
        let text = r#"{"action":"offer","clips":[{"segments":[{"start_s":1.0,"end_s":5.0}],"crop":{"x":0.2,"y":0.1,"w":0.5,"h":0.9},"motion":"hold","hooks":["the pause lands"],"captions":[{"t":0.4,"text":"look here"}],"h":80,"v":80,"e":70,"a":70,"p":70,"note":"Opens on a face.","lesson":"The opening is the claim. These sections share one beat. Would you keep it?"}.]}"#;
        match parse_action(text).unwrap() {
            Action::Offer { clips } => {
                assert_eq!(clips.len(), 1);
                assert_eq!(clips[0].segments, vec![(1.0, 5.0)]);
            }
            other => panic!("expected offer, got {other:?}"),
        }
    }

    #[test]
    fn a_stray_brace_between_clips_is_dropped() {
        let one = clip_json(1.0, 16.0);
        let two = clip_json(20.0, 36.0);
        let text = format!(r#"{{"action":"offer","clips":[{one}}}, {two}]}}"#);
        match parse_action(&text).unwrap() {
            Action::Offer { clips } => assert_eq!(clips.len(), 2),
            other => panic!("expected offer, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_segment_bracket_keeps_the_crop() {
        let text = r#"{"action":"offer","clips":[{"segments":[{"start_s":1.0,"end_s":5.0}},"crop":{"x":0.2,"y":0.1,"w":0.5,"h":0.9},"motion":"hold","hooks":["the pause lands"],"captions":[{"t":0.4,"text":"look here"}],"h":80,"v":80,"e":70,"a":70,"p":70,"note":"Opens on a face.","lesson":"The opening is the claim."}]}"#;
        match parse_action(text).unwrap() {
            Action::Offer { clips } => {
                assert_eq!(clips[0].segments, vec![(1.0, 5.0)]);
                assert_eq!(clips[0].crop.w, 0.5);
            }
            other => panic!("expected offer, got {other:?}"),
        }
    }

    #[test]
    fn inserts_a_missing_comma_between_clips() {
        let one = clip_json(1.0, 5.0);
        let two = clip_json(8.0, 12.0);
        let text = format!(r#"{{"action":"offer","clips":[{one} {two}]}}"#);
        match parse_action(&text).unwrap() {
            Action::Offer { clips } => assert_eq!(clips.len(), 2),
            other => panic!("expected offer, got {other:?}"),
        }
    }

    #[test]
    fn inserts_a_missing_comma_between_hooks() {
        let text = r#"{
            "action": "offer",
            "clips": [{
                "start_s": 1.0,
                "end_s": 5.0,
                "crop": {"x": 0.2, "y": 0.1, "w": 0.5, "h": 0.9},
                "motion": "hold",
                "hooks": ["the pause lands" "watch the hands"],
                "captions": [{"t": 0.4, "text": "look here"}],
                "h": 80, "v": 80, "e": 70, "a": 70, "p": 70,
                "note": "Opens on a face.",
                "lesson": "The opening is the claim."
            }]
        }"#;
        match parse_action(text).unwrap() {
            Action::Offer { clips } => {
                assert_eq!(
                    clips[0].hooks,
                    vec!["the pause lands".to_string(), "watch the hands".to_string()]
                );
            }
            other => panic!("expected offer, got {other:?}"),
        }
    }

    #[test]
    fn parses_multi_segment_clip_offer() {
        let text = r#"{
            "action": "offer",
            "clips": [
                {
                    "segments": [
                        {"start_s": 10.0, "end_s": 14.0},
                        {"start_s": 25.0, "end_s": 29.5},
                        {"start_s": 40.0, "end_s": 44.0}
                    ],
                    "crop": {"x": 0.1, "y": 0.0, "w": 0.8, "h": 1.0},
                    "motion": "push_in",
                    "captions": [{"t": 0.5, "text": "look here"}],
                    "hooks": ["best moment", "must watch"],
                    "h": 85, "v": 90, "e": 75, "a": 80, "p": 70,
                    "note": "Great montage.",
                    "lesson": "Three sections combined for fast pacing."
                }
            ]
        }"#;
        let action = parse_action(text).unwrap();
        match action {
            Action::Offer { clips } => {
                assert_eq!(clips.len(), 1);
                let c = &clips[0];
                assert_eq!(c.segments.len(), 3);
                assert_eq!(c.segments[0], (10.0, 14.0));
                assert_eq!(c.segments[1], (25.0, 29.5));
                assert_eq!(c.segments[2], (40.0, 44.0));
                assert_eq!(c.start_s, 10.0);
                assert_eq!(c.end_s, 44.0);
            }
            _ => panic!("expected offer"),
        }
    }

    #[test]
    fn clip_offer_clamps_out_of_bounds_crop_gracefully() {
        let text = r#"{
            "action": "offer",
            "clips": [
                {
                    "segments": [{"start_s": 1.0, "end_s": 5.0}],
                    "crop": {"x": 0.2, "y": 0.1, "w": 0.95, "h": 0.95},
                    "motion": "hold",
                    "captions": [{"t": 0.4, "text": "look here"}],
                    "hooks": ["the pause lands"],
                    "h": 80, "v": 80, "e": 70, "a": 70, "p": 70,
                    "note": "Opens on a face.",
                    "lesson": "The opening is the claim."
                }
            ]
        }"#;
        let action = parse_action(text).unwrap();
        match action {
            Action::Offer { clips } => {
                assert!(clips[0].crop.x + clips[0].crop.w <= 1.0 + 1e-6);
                assert!(clips[0].crop.y + clips[0].crop.h <= 1.0 + 1e-6);
            }
            _ => panic!("expected offer"),
        }
    }

    #[test]
    fn clip_offer_truncates_long_captions() {
        let text = r#"{
            "action": "offer",
            "clips": [
                {
                    "segments": [{"start_s": 1.0, "end_s": 5.0}],
                    "crop": {"x": 0.2, "y": 0.1, "w": 0.5, "h": 0.8},
                    "motion": "hold",
                    "captions": [{"t": 0.4, "text": "this caption has way too many words"}],
                    "hooks": ["the pause lands"],
                    "h": 80, "v": 80, "e": 70, "a": 70, "p": 70,
                    "note": "Opens on a face.",
                    "lesson": "The opening is the claim."
                }
            ]
        }"#;
        let action = parse_action(text).unwrap();
        match action {
            Action::Offer { clips } => {
                assert_eq!(clips[0].captions[0].text, "this caption has way");
            }
            _ => panic!("expected offer"),
        }
    }

    #[test]
    fn cue_with_effects_and_gradients_deserializes() {
        let json = r#"{"action":"set_captions","cues":[{"t":0.5,"text":"paused","effect":"shake"},{"t":1.2,"text":"boosted","effect":"gradient:fire"}]}"#;
        let action = parse_action(json).unwrap();
        match action {
            Action::SetCaptions { cues } => {
                assert_eq!(cues.len(), 2);
                assert_eq!(cues[0].effect.as_deref(), Some("shake"));
                assert_eq!(cues[0].gradient, None);
                assert_eq!(cues[1].effect.as_deref(), Some("gradient"));
                assert_eq!(cues[1].gradient.as_deref(), Some("fire"));
            }
            other => panic!("expected SetCaptions, got {other:?}"),
        }
    }

    #[test]
    fn inline_caption_tags_parsed_correctly() {
        let cue = Cue::new(0.4, "I can [shake]pause[/shake] and [gradient:ocean]flow[/gradient]!");
        assert_eq!(cue.word_count(), 5);
        assert_eq!(cue.plain_text(), "I can pause and flow!");
        let spans = cue.styled_spans();
        assert_eq!(spans.len(), 5);
        assert_eq!(spans[0].text, "I can ");
        assert_eq!(spans[0].effect, None);
        assert_eq!(spans[1].text, "pause");
        assert_eq!(spans[1].effect.as_deref(), Some("shake"));
        assert_eq!(spans[2].text, " and ");
        assert_eq!(spans[3].text, "flow");
        assert_eq!(spans[3].effect.as_deref(), Some("gradient"));
        assert_eq!(spans[3].gradient.as_deref(), Some("ocean"));
        assert_eq!(spans[4].text, "!");
    }

    #[test]
    fn done_or_offer_nothing_phrases_parse_as_done() {
        assert_eq!(parse_action("done").unwrap(), Action::Done);
        assert_eq!(parse_action("Done.").unwrap(), Action::Done);
        assert_eq!(parse_action("offer nothing and done").unwrap(), Action::Done);
        assert_eq!(parse_action("The window is slides or black frames, offer nothing and done.").unwrap(), Action::Done);
        assert_eq!(parse_action("These are black frames, done.").unwrap(), Action::Done);
        assert_eq!(parse_action("Slides detected, offer nothing.").unwrap(), Action::Done);
        assert_eq!(parse_action(r#"{"action":"offer_nothing"}"#).unwrap(), Action::Done);
        assert_eq!(parse_action(r#"{"action":"none"}"#).unwrap(), Action::Done);
    }

    #[test]
    fn set_framing_action_parses_fit_crop_and_move() {
        // Crop mode
        let json_crop = r#"{"action":"set_framing","mode":"crop","focus_x":0.7,"focus_y":0.3,"zoom":0.9}"#;
        let act = parse_action(json_crop).unwrap();
        assert_eq!(
            act,
            Action::SetFraming(Framing::Crop {
                focus_x: 0.7,
                focus_y: 0.3,
                zoom: 0.9,
            })
        );

        // Crop and move mode
        let json_move = r#"{"action":"set_framing","mode":"crop_and_move","start_x":0.1,"end_x":0.9,"focus_y":0.5}"#;
        let act = parse_action(json_move).unwrap();
        assert_eq!(
            act,
            Action::SetFraming(Framing::CropAndMove {
                start_x: 0.1,
                end_x: 0.9,
                focus_y: 0.5,
                zoom: 1.0,
            })
        );

        // Fit mode with blur
        let json_fit_blur = r#"{"action":"set_framing","mode":"fit","background":"blur"}"#;
        let act = parse_action(json_fit_blur).unwrap();
        assert_eq!(
            act,
            Action::SetFraming(Framing::Fit {
                background: BackgroundStyle::Blur,
            })
        );

        // Fit mode with black
        let json_fit_black = r#"{"action":"set_framing","mode":"fit","background":"black"}"#;
        let act = parse_action(json_fit_black).unwrap();
        assert_eq!(
            act,
            Action::SetFraming(Framing::Fit {
                background: BackgroundStyle::Black,
            })
        );

        // Nested framing object
        let json_nested = r#"{"action":"set_framing","framing":{"mode":"crop","focus_x":0.25}}"#;
        let act = parse_action(json_nested).unwrap();
        assert_eq!(
            act,
            Action::SetFraming(Framing::Crop {
                focus_x: 0.25,
                focus_y: 0.5,
                zoom: 1.0,
            })
        );
    }

    #[test]
    fn offer_with_explicit_framing_parses_correctly() {
        let text = r#"{
            "action": "offer",
            "clips": [
                {
                    "segments": [{"start_s": 1.0, "end_s": 5.0}],
                    "framing": {"mode": "crop_and_move", "start_x": 0.1, "end_x": 0.9},
                    "captions": [{"t": 0.4, "text": "moving pan"}],
                    "hooks": ["watch this"],
                    "h": 80, "v": 80, "e": 70, "a": 70, "p": 70,
                    "note": "Great pan",
                    "lesson": "Pans across subjects"
                }
            ]
        }"#;
        let action = parse_action(text).unwrap();
        match action {
            Action::Offer { clips } => {
                assert_eq!(clips.len(), 1);
                assert_eq!(
                    clips[0].framing,
                    Framing::CropAndMove {
                        start_x: 0.1,
                        end_x: 0.9,
                        focus_y: 0.5,
                        zoom: 1.0,
                    }
                );
            }
            other => panic!("expected offer, got {other:?}"),
        }
    }

    #[test]
    fn legacy_offer_infers_smart_framing() {
        // Full frame default -> Fit Blur
        let text_fit = r#"{
            "action": "offer",
            "clips": [
                {
                    "segments": [{"start_s": 1.0, "end_s": 5.0}],
                    "crop": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                    "motion": "hold",
                    "captions": [],
                    "hooks": ["hook"],
                    "h": 80, "v": 80, "e": 70, "a": 70, "p": 70,
                    "note": "Note",
                    "lesson": "Lesson"
                }
            ]
        }"#;
        let act = parse_action(text_fit).unwrap();
        match act {
            Action::Offer { clips } => {
                assert_eq!(clips[0].framing, Framing::Fit { background: BackgroundStyle::Blur });
            }
            _ => panic!("expected offer"),
        }

        // PanLeft motion -> CropAndMove
        let text_pan = r#"{
            "action": "offer",
            "clips": [
                {
                    "segments": [{"start_s": 1.0, "end_s": 5.0}],
                    "motion": "pan_left",
                    "captions": [],
                    "hooks": ["hook"],
                    "h": 80, "v": 80, "e": 70, "a": 70, "p": 70,
                    "note": "Note",
                    "lesson": "Lesson"
                }
            ]
        }"#;
        let act_pan = parse_action(text_pan).unwrap();
        match act_pan {
            Action::Offer { clips } => {
                assert_eq!(
                    clips[0].framing,
                    Framing::CropAndMove {
                        start_x: 0.8,
                        end_x: 0.2,
                        focus_y: 0.5,
                        zoom: 1.0,
                    }
                );
            }
            _ => panic!("expected offer"),
        }
    }

    fn clip_json(start: f64, end: f64) -> String {
        format!(
            r#"{{"start_s":{start},"end_s":{end},"crop":{{"x":0.2,"y":0.1,"w":0.4,"h":0.8}},"motion":"push_in","captions":[{{"t":0.4,"text":"the good part"}}],"hooks":["wait for it"],"h":80,"v":80,"e":70,"a":70,"p":70,"note":"Opens on a face.","lesson":"The opening is the claim. The window holds the laugh. Would you keep it?"}}"#
        )
    }

    #[test]
    fn parses_action_with_gemma_channel_thoughts() {
        let text = "<|channel>thought\nCandidate 1 has great motion. What if I do {\"action\":\"done\"}? No, let's offer clips.\n<channel|>\n{\"action\":\"done\"}";
        assert_eq!(parse_action(text).unwrap(), Action::Done);

        let unclosed = "<|channel>thought\nStill thinking through the candidate segments...";
        assert!(parse_action(unclosed).is_err());
    }

    #[test]
    fn parses_action_with_think_tags() {
        let text = "<think>\nPlanning the reels and examining sections: {\"mode\":\"fit\"}\n</think>\n{\"action\":\"done\"}";
        assert_eq!(parse_action(text).unwrap(), Action::Done);

        let unclosed = "<think>\nIn the middle of reasoning...";
        assert!(parse_action(unclosed).is_err());
    }
}

