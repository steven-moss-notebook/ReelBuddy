use std::path::{Path, PathBuf};

use crate::db::{self, Db};
use crate::field::FieldSpawn;
use crate::editor::actions::{Cue, Framing, Motion, NormCrop};
use crate::editor::drive::{self, Draft, SeenFrame, Talk};
use crate::editor::prompt;
use crate::editor::view::{ChoreKind, ChoreView, HuntOption};
use crate::media;
use crate::pet_state::{self, PetSnapshot};
use crate::taste;

use super::EditorCommand;

pub struct Desk {
    pub db: Db,
    pub pet: PetSnapshot,
    pub chore: Option<ChoreView>,
    pub app_data: PathBuf,
    pub progress: super::ModelProgress,
}

impl Desk {
    pub fn boot(app_data: &Path) -> Result<Self, String> {
        let db = Db::open(&db::db_path(app_data))?;
        let mut pet = db.load_pet()?;
        pet.model_present = false;
        pet.woke = false;
        pet.busy = false;
        pet.eating = false;
        let chore = match db.open_chore()? {
            Some((id, payload, _)) => Db::restore_chore(&payload, id),
            None => None,
        };
        let mut desk = Self {
            db,
            pet,
            chore,
            app_data: app_data.to_path_buf(),
            progress: super::ModelProgress::Idle,
        };
        desk.refresh();
        Ok(desk)
    }

    pub fn refresh(&mut self) {
        if let Ok((accepts, rejects)) = self.db.accept_reject_counts() {
            self.pet.bond = pet_state::bond_from_counts(accepts, rejects);
        }
        if let Ok(scores) = self.db.recent_shipped_scores() {
            self.pet.mood = pet_state::mood_from_scores(&scores);
        }
        self.pet.stage = pet_state::stage_for_shipped(self.pet.shipped);
        self.pet.energy = if self.pet.busy { 25 } else { 100 };
        self.pet.sulk = false;
        if let Ok(Some((_, _, created))) = self.db.open_chore() {
            if db::now_unix() - created > 86_400 {
                self.pet.sulk = true;
            }
        }
    }

    pub fn persist(&self) {
        let _ = self.db.save_pet(&self.pet);
    }

    pub fn ensure_remix(&mut self) {
        let open = self.db.open_count().unwrap_or(1);
        let day = taste::utc_date(db::now_unix());
        let already = self.db.remix_on_day(&day).unwrap_or(true);
        let clip = self.db.latest_ready_clip().ok().flatten();
        if !taste::remix_due(open, already, clip.is_some()) {
            return;
        }
        let Some(clip) = clip else { return };
        let kind = ChoreKind::Remix {
            window_id: clip.window_id,
            clip_id: clip.id,
        };
        if let Ok(id) = self.insert_view(kind, None, Some(clip.window_id), Some(clip.id)) {
            self.chore = self.db.open_chore().ok().flatten().and_then(|(oid, payload, _)| {
                let _ = id;
                Db::restore_chore(&payload, oid)
            });
        }
    }

    fn insert_view(
        &self,
        kind: ChoreKind,
        source_id: Option<i64>,
        window_id: Option<i64>,
        clip_id: Option<i64>,
    ) -> Result<i64, String> {
        let view = ChoreView { id: 0, kind: kind.clone() };
        let payload = serde_json::to_string(&view).map_err(|err| err.to_string())?;
        self.db
            .insert_chore(kind.type_name(), source_id, window_id, clip_id, &payload)
    }

    fn set_chore(&mut self, id: i64) {
        self.chore = self
            .db
            .open_chore()
            .ok()
            .flatten()
            .filter(|(oid, _, _)| *oid == id)
            .and_then(|(oid, payload, _)| Db::restore_chore(&payload, oid));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SectionFind {
    pub start_s: f64,
    pub end_s: f64,
    pub score: f64,
}

pub enum Live {
    Pulse,
    Spawns(Vec<crate::field::FieldSpawn>),
    Finds(Vec<SectionFind>),
}

pub fn handle<T: Talk + ?Sized>(desk: &mut Desk, brain: Option<&mut T>, command: EditorCommand) {
    let mut quiet = |_: &Desk, _: Live| {};
    handle_live(desk, brain, command, &mut quiet);
}

pub fn handle_live<T, L>(desk: &mut Desk, brain: Option<&mut T>, command: EditorCommand, live: &mut L)
where
    T: Talk + ?Sized,
    L: FnMut(&Desk, Live),
{
    desk.pet.eating = false;
    match command {
        EditorCommand::BumpHunger => {
            desk.pet.hunger = pet_state::bump_hunger(desk.pet.hunger);
            desk.pet.woke = true;
        }
        EditorCommand::Ingest(path) => ingest(desk, brain, &path, live),
        EditorCommand::Pick { chore_id, window_id } => {
            if desk.pet.busy || !matches_open(desk, chore_id) {
                return;
            }
            let options = hunt_options(desk);
            if !options.iter().any(|option| option.window_id == window_id) {
                return;
            }
            record_pick(desk, &options, window_id);
            let _ = desk.db.resolve_chore(chore_id, "resolved", r#"{"pick":true}"#);
            desk.pet.hunger = pet_state::hunger_after_pick(desk.pet.hunger);
            desk.chore = None;
            desk.pet.busy = true;
            desk.pet.line = pet_state::CHAMBER_PAINT_DETAIL.to_string();
            desk.progress = super::ModelProgress::Ready;
            live(desk, Live::Pulse);
            let hook = first_hook(desk, window_id);
            encode_picked(desk, window_id, &hook, None, true);
        }
        EditorCommand::RejectClip { chore_id, window_id, reason } => {
            reject_card(desk, chore_id, window_id, &reason);
        }
        EditorCommand::AcceptCut { chore_id } => accept_cut(desk, chore_id),
        EditorCommand::Nudge { chore_id, delta } => nudge(desk, chore_id, delta),
        EditorCommand::Reject { chore_id, reason } => reject(desk, chore_id, &reason),
        EditorCommand::PickHook { chore_id, hook } => pick_hook(desk, chore_id, &hook),
        EditorCommand::Ship { chore_id } => ship(desk, chore_id),
        EditorCommand::Hold { chore_id } => {
            if matches_open(desk, chore_id) {
                let _ = desk.db.resolve_chore(chore_id, "skipped", r#"{"hold":true}"#);
                desk.chore = None;
                desk.pet.collar = None;
                desk.ensure_remix();
            }
        }
        EditorCommand::RemixTry { chore_id } => {
            let clip_id = match &desk.chore {
                Some(view) if view.id == chore_id => match &view.kind {
                    ChoreKind::Remix { clip_id, .. } => Some(*clip_id),
                    _ => None,
                },
                _ => None,
            };
            if let Some(clip_id) = clip_id {
                let path = desk.db.clip(clip_id).ok().and_then(|row| row.path);
                let _ = desk.db.resolve_chore(chore_id, "resolved", r#"{"accept":true}"#);
                desk.chore = None;
                if let Some(path) = path {
                    reveal_saved(&path);
                    let name = std::path::Path::new(&path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or(path);
                    desk.pet.line = pet_state::kept_line(&name);
                }
            }
        }
        EditorCommand::RemixSkip { chore_id } => {
            if matches_open(desk, chore_id) {
                let _ = desk.db.resolve_chore(chore_id, "skipped", r#"{"skip":true}"#);
                desk.chore = None;
                desk.ensure_remix();
            }
        }
        EditorCommand::Shutdown => {}
    }
    desk.refresh();
    desk.persist();
}

fn matches_open(desk: &Desk, chore_id: i64) -> bool {
    desk.chore.as_ref().is_some_and(|view| view.id == chore_id)
}

fn ingest<T, L>(desk: &mut Desk, brain: Option<&mut T>, path: &Path, live: &mut L)
where
    T: Talk + ?Sized,
    L: FnMut(&Desk, Live),
{
    let raw_path = normalize_local_path(path);
    let origin = std::fs::canonicalize(&raw_path).unwrap_or(raw_path);
    if !origin.is_file() {
        desk.pet.line = pet_state::LOCAL_ONLY_LINE.to_string();
        return;
    }
    let duration = match media::duration_s(&origin) {
        Ok(dur) => dur,
        Err(err) => {
            desk.pet.line = pet_state::model_error_line(&format!("Cannot open video: {err}"));
            return;
        }
    };
    if duration <= 0.0 {
        desk.pet.line = pet_state::model_error_line("Source video duration is 0.0s.");
        return;
    }
    // Strictly requires the LLM - no fallback per user instructions
    let Some(brain) = brain else {
        desk.pet.line = pet_state::missing_model_line();
        return;
    };

    let source_id = match desk.db.insert_source(&origin.display().to_string(), Some(duration), "ready", None) {
        Ok(id) => id,
        Err(err) => {
            desk.pet.line = pet_state::model_error_line(&err);
            return;
        }
    };
    let dur = duration;
    desk.pet.busy = true;
    desk.pet.energy = 25;
    desk.pet.line = pet_state::sorting_line();
    live(desk, Live::Pulse);

    // Stream video frames through the Oklab Dual Gabor Field Estimator
    let interval_s = if dur > 1200.0 {
        2.5
    } else if dur > 600.0 {
        2.0
    } else if dur > 60.0 {
        1.0
    } else {
        0.5
    };
    let expected = ((dur / interval_s).ceil() as u32).max(1);
    let mut seen = 0u32;
    desk.progress = super::ModelProgress::Scanning {
        done: 0,
        total: expected,
    };
    live(desk, Live::Pulse);
    let mut batch_spawns = Vec::new();
    let analyses = match media::stream_gabor_samples(&origin, interval_s, |spawn| {
        seen += 1;
        batch_spawns.push(spawn.clone());
        if batch_spawns.len() >= 4 {
            desk.progress = super::ModelProgress::Scanning {
                done: seen,
                total: expected.max(seen),
            };
            live(desk, Live::Pulse);
            live(desk, Live::Spawns(std::mem::take(&mut batch_spawns)));
        }
    }) {
        Ok(res) => {
            let total = expected.max(res.len() as u32);
            desk.progress = super::ModelProgress::Scanning {
                done: res.len() as u32,
                total,
            };
            live(desk, Live::Pulse);
            if !batch_spawns.is_empty() {
                live(desk, Live::Spawns(batch_spawns));
            }
            res
        }
        Err(err) => {
            desk.pet.busy = false;
            desk.progress = super::ModelProgress::Ready;
            desk.pet.line = pet_state::model_error_line(&err);
            return;
        }
    };

    // Find the best 10 to 20 short sections (3-5 seconds each)
    let target_count = if dur >= 300.0 {
        20
    } else if dur >= 120.0 {
        16
    } else if dur >= 60.0 {
        14
    } else {
        10
    };
    let candidate_sections = crate::field::select_best_sections(&analyses, 3.0, 5.0, target_count);

    if candidate_sections.is_empty() {
        desk.pet.busy = false;
        desk.progress = super::ModelProgress::Ready;
        desk.pet.line = pet_state::nothing_to_cut_line();
        return;
    }

    let finds: Vec<SectionFind> = candidate_sections
        .iter()
        .map(|section| SectionFind {
            start_s: section.start_s,
            end_s: section.end_s,
            score: section.score as f64,
        })
        .collect();
    live(desk, Live::Finds(finds));

    // Sample frames ONLY inside the candidate sections (1 frame per section, midpoint)
    // The LLM does NOT see the entire clip.
    let section_spans: Vec<(f64, f64)> = candidate_sections
        .iter()
        .map(|s| (s.start_s, s.end_s))
        .collect();
    let section_samples = match media::sample_sections_only(&origin, &section_spans, 1) {
        Ok(samples) => samples,
        Err(err) => {
            desk.pet.busy = false;
            desk.progress = super::ModelProgress::Ready;
            desk.pet.line = pet_state::model_error_line(&err);
            return;
        }
    };

    let seen_frames: Vec<SeenFrame> = section_samples
        .into_iter()
        .map(|s| SeenFrame { t: s.t, jpeg: s.jpeg })
        .collect();

    let spawns = seen_frames_to_spawns(&seen_frames);
    live(desk, Live::Spawns(spawns));

    let dims = media::video_dimensions(&origin).ok();
    let scout_task = prompt::scout_sections_prompt(&candidate_sections, dims);
    desk.progress = super::ModelProgress::Choosing;
    live(desk, Live::Pulse);
    let draft = match run_scout_model(desk, brain, &origin, &scout_task, seen_frames, live) {
        Ok(draft) => draft,
        Err(line) => {
            desk.pet.busy = false;
            desk.progress = super::ModelProgress::Ready;
            desk.pet.line = line;
            return;
        }
    };

    desk.pet.busy = false;
    desk.progress = super::ModelProgress::Ready;
    if draft.offers.is_empty() {
        desk.pet.line = pet_state::nothing_to_cut_line();
        return;
    }

    store_offers(desk, source_id, &draft);
}


fn accept_cut(desk: &mut Desk, chore_id: i64) {
    let Some(view) = desk.chore.clone() else { return };
    if view.id != chore_id {
        return;
    }
    let ChoreKind::Cut { window_id, hooks, note } = view.kind else {
        return;
    };
    let _ = desk.db.resolve_chore(chore_id, "resolved", r#"{"accept":true}"#);
    let (a, b) = match hooks.as_slice() {
        [a, b, ..] => (a.clone(), b.clone()),
        [a] => (a.clone(), a.clone()),
        _ => {
            desk.pet.line = pet_state::cant_cut_line();
            desk.chore = None;
            return;
        }
    };
    let _ = note;
    let kind = ChoreKind::Hook { window_id, a, b };
    match desk.insert_view(kind, None, Some(window_id), None) {
        Ok(id) => desk.set_chore(id),
        Err(err) => desk.pet.line = pet_state::model_error_line(&err),
    }
}

fn nudge(desk: &mut Desk, chore_id: i64, delta: f64) {
    if !matches_open(desk, chore_id) {
        return;
    }
    let Some(window_id) = desk.chore.as_ref().and_then(|view| match &view.kind {
        ChoreKind::Cut { window_id, .. } => Some(*window_id),
        _ => None,
    }) else {
        return;
    };
    let Ok(row) = desk.db.window(window_id) else { return };
    let mut start = row.start_s + delta;
    let mut end = row.end_s + delta;
    if start < 0.0 {
        end -= start;
        start = 0.0;
    }
    if end - start > 60.0 {
        end = start + 60.0;
    }
    if end - start < 0.5 {
        end = start + 0.5;
    }
    let _ = desk.db.nudge_window(window_id, start, end);
}

fn reject(desk: &mut Desk, chore_id: i64, reason: &str) {
    if !taste::REASONS.contains(&reason) || !matches_open(desk, chore_id) {
        return;
    }
    let window_id = desk.chore.as_ref().and_then(|view| match &view.kind {
        ChoreKind::Cut { window_id, .. } => Some(*window_id),
        _ => None,
    });
    let _ = desk.db.insert_taste(reason, window_id);
    let _ = desk.db.resolve_chore(chore_id, "resolved", &format!(r#"{{"reject":"{reason}"}}"#));
    desk.chore = None;
    desk.pet.collar = None;
    desk.ensure_remix();
}

fn pick_hook(desk: &mut Desk, chore_id: i64, hook: &str) {
    let Some(view) = desk.chore.clone() else { return };
    if view.id != chore_id {
        return;
    }
    let ChoreKind::Hook { window_id, .. } = view.kind else { return };
    encode_picked(desk, window_id, hook, Some(chore_id), false);
}

fn encode_picked(desk: &mut Desk, window_id: i64, hook: &str, chore_id: Option<i64>, keep: bool) {
    let row = match desk.db.window(window_id) {
        Ok(row) => row,
        Err(err) => {
            desk.pet.busy = false;
            desk.pet.line = pet_state::model_error_line(&err);
            return;
        }
    };
    let segments = row.segments();
    let total_duration = row.total_duration();
    if total_duration > 60.0 {
        desk.pet.busy = false;
        desk.pet.line = pet_state::cant_cut_line();
        return;
    }
    let origin = match desk.db.source_origin(row.source_id) {
        Ok(origin) => origin,
        Err(err) => {
            desk.pet.busy = false;
            desk.pet.line = pet_state::model_error_line(&err);
            return;
        }
    };
    let motion = Motion::from_stored(row.motion.as_deref().unwrap_or("hold"));
    let framing: Framing = row
        .framing_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_else(|| {
            let legacy_crop: Option<NormCrop> = row.crop_json.as_deref().and_then(|json| serde_json::from_str(json).ok());
            match motion {
                Motion::PanLeft => Framing::CropAndMove {
                    start_x: 0.8,
                    end_x: 0.2,
                    focus_y: 0.5,
                    zoom: 1.0,
                },
                Motion::PanRight => Framing::CropAndMove {
                    start_x: 0.2,
                    end_x: 0.8,
                    focus_y: 0.5,
                    zoom: 1.0,
                },
                Motion::PushIn => Framing::Crop {
                    focus_x: 0.5,
                    focus_y: 0.5,
                    zoom: 0.85,
                },
                _ => {
                    if let Some(c) = legacy_crop {
                        if c.w < 0.85 || c.h < 0.85 {
                            Framing::Crop {
                                focus_x: (c.x + c.w * 0.5).clamp(0.0, 1.0),
                                focus_y: (c.y + c.h * 0.5).clamp(0.0, 1.0),
                                zoom: c.h.clamp(0.1, 1.0),
                            }
                        } else {
                            Framing::Fit {
                                background: crate::editor::BackgroundStyle::Blur,
                            }
                        }
                    } else {
                        Framing::Fit {
                            background: crate::editor::BackgroundStyle::Blur,
                        }
                    }
                }
            }
        });
    let captions: Vec<Cue> = row
        .captions_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    let clips = desk.app_data.join("clips");
    let silent = clips.join(format!("window-{window_id}-silent.mp4"));
    let final_path = clips.join(format!("window-{window_id}.mp4"));
    desk.pet.busy = true;
    let encoded = media::encode_segments(
        Path::new(&origin),
        &silent,
        &segments,
        &framing,
        &captions,
        hook,
    );
    desk.pet.busy = false;
    if let Err(err) = encoded {
        desk.pet.line = pet_state::model_error_line(&err);
        if keep {
            desk.progress = super::ModelProgress::Ready;
            desk.chore = None;
        }
        return;
    }
    let output = match media::mux_source_audio_segments(&silent, Path::new(&origin), &segments, &final_path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&silent);
            final_path
        }
        Err(_) => {
            let _ = std::fs::rename(&silent, &final_path);
            final_path
        }
    };
    let clip_id = match desk.db.insert_clip(window_id, &output.display().to_string(), hook, "ready") {
        Ok(id) => id,
        Err(err) => {
            desk.pet.line = pet_state::model_error_line(&err);
            if keep {
                desk.progress = super::ModelProgress::Ready;
                desk.chore = None;
            }
            return;
        }
    };
    if keep {
        let name = output
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| output.display().to_string());
        
        // Copy to app-local ./saved_reels folder for easy access
        let local_dir = std::env::current_dir()
            .unwrap_or_else(|_| desk.app_data.clone())
            .join("saved_reels");
        if std::fs::create_dir_all(&local_dir).is_ok() {
            let local_dest = local_dir.join(&name);
            if std::fs::copy(&output, &local_dest).is_ok() {
                reveal_saved(&local_dest.display().to_string());
            } else {
                reveal_saved(&output.display().to_string());
            }
        } else {
            reveal_saved(&output.display().to_string());
        }

        desk.pet.line = format!("{} ./saved_reels/{}", pet_state::KEPT_LINE, name);
        desk.pet.collar = None;
        desk.progress = super::ModelProgress::Ready;
        desk.chore = None;
        return;
    }
    if let Some(chore_id) = chore_id {
        let _ = desk.db.resolve_chore(chore_id, "resolved", r#"{"hook":true}"#);
    }
    let note = row.note.unwrap_or_default();
    desk.pet.line = pet_state::truncate_line(&note);
    let kind = ChoreKind::Ship {
        window_id,
        clip_id,
        note,
    };
    match desk.insert_view(kind, None, Some(window_id), Some(clip_id)) {
        Ok(id) => desk.set_chore(id),
        Err(err) => desk.pet.line = pet_state::model_error_line(&err),
    }
}

fn ship(desk: &mut Desk, chore_id: i64) {
    let Some(view) = desk.chore.clone() else { return };
    if view.id != chore_id {
        return;
    }
    let ChoreKind::Ship { window_id, clip_id, note } = view.kind else { return };
    let _ = desk.db.update_clip(clip_id, "shipped", None, None);
    let _ = desk.db.write_window_edit(window_id, "rendered", None, None, None, Some(&note));
    let _ = desk.db.resolve_chore(chore_id, "resolved", r#"{"accept":true}"#);
    desk.pet.shipped = desk.pet.shipped.saturating_add(1);
    desk.pet.stage = pet_state::stage_for_shipped(desk.pet.shipped);
    desk.pet.eating = true;
    desk.pet.line = pet_state::truncate_line(&note);
    desk.pet.collar = None;
    desk.chore = None;
    desk.ensure_remix();
}

fn seen_frames_to_spawns(frames: &[SeenFrame]) -> Vec<FieldSpawn> {
    frames
        .iter()
        .map(|frame| FieldSpawn {
            t: frame.t,
            sig: crate::field::signature_jpeg(&frame.jpeg),
        })
        .collect()
}

struct ModelDriveContext {
    system: String,
    banned: Vec<String>,
    stage: u8,
}

impl ModelDriveContext {
    fn from_desk(desk: &Desk) -> Self {
        let counts = desk.db.taste_counts().unwrap_or_default();
        let banned = desk.db.banned_reasons().unwrap_or_default();
        let system = prompt::system_prompt(&counts);
        Self {
            system,
            banned,
            stage: desk.pet.stage,
        }
    }
}

fn run_scout_model<T, L>(
    desk: &Desk,
    brain: &mut T,
    source: &Path,
    task: &str,
    initial_frames: Vec<SeenFrame>,
    live: &mut L,
) -> Result<Draft, String>
where
    T: Talk + ?Sized,
    L: FnMut(&Desk, Live),
{
    let ctx = ModelDriveContext::from_desk(desk);
    let mut sample_frames = |start: f64, end: f64, every: f64, max: u32| {
        let frames = sample_source(source, start, end, every, max)?;
        live(desk, Live::Spawns(seen_frames_to_spawns(&frames)));
        Ok(frames)
    };
    drive::drive_with_frames(
        |prompt, jpegs| brain.generate(prompt, jpegs),
        &mut sample_frames,
        &ctx.system,
        task,
        initial_frames,
        &ctx.banned,
        ctx.stage,
        3,
    )
}

fn sample_source(path: &Path, start: f64, end: f64, every: f64, max: u32) -> Result<Vec<SeenFrame>, String> {
    let mut times = Vec::new();
    let mut t = start;
    while t <= end && times.len() < max as usize {
        times.push(t);
        t += every;
    }
    if times.is_empty() {
        times.push(start);
    }
    let samples = media::sample_times(path, &times)?;
    Ok(samples
        .into_iter()
        .map(|sample| SeenFrame {
            t: sample.t,
            jpeg: sample.jpeg,
        })
        .collect())
}

fn bare_option(window_id: i64, label: String, start_s: f64, end_s: f64) -> HuntOption {
    HuntOption {
        window_id,
        label,
        start_s,
        end_s,
        segments: vec![(start_s, end_s)],
        motion: String::new(),
        caption: String::new(),
        hook: String::new(),
        hooks: Vec::new(),
        h: 0.0,
        v: 0.0,
        e: 0.0,
        a: 0.0,
        p: 0.0,
        s: 0.0,
        lesson: String::new(),
        thumb: String::new(),
        thumbs: Vec::new(),
    }
}

fn store_offers(desk: &mut Desk, source_id: i64, draft: &Draft) {
    let origin = desk.db.source_origin(source_id).ok();
    let mut options = Vec::new();
    let mut best = 0u8;
    for clip in &draft.offers {
        let Ok(id) = desk.db.insert_window(source_id, clip.start_s, clip.end_s, "candidate") else {
            continue;
        };
        let crop_json = serde_json::to_string(&clip.crop).unwrap_or_else(|_| "{}".into());
        let framing_json = serde_json::to_string(&clip.framing).unwrap_or_else(|_| "{}".into());
        let captions_json = serde_json::to_string(&clip.captions).unwrap_or_else(|_| "[]".into());
        let hooks_json = serde_json::to_string(&clip.hooks).unwrap_or_else(|_| "[]".into());
        let segments_json = serde_json::to_string(&clip.segments).unwrap_or_else(|_| "[]".into());
        let (thumb, thumbs) = write_thumbs(&desk.app_data, id, origin.as_deref(), &draft.seen, clip);
        let _ = desk.db.write_window_offer(
            id,
            "candidate",
            &clip.scores,
            &crop_json,
            &captions_json,
            &clip.scores.note,
            clip.motion.as_str(),
            &hooks_json,
            &clip.lesson,
            &thumb,
            &segments_json,
            &framing_json,
        );
        let score = clip.scores.s.round().clamp(0.0, 100.0) as u8;
        if score > best {
            best = score;
        }
        let label = if !clip.segments.is_empty() {
            let total_dur: f64 = clip.segments.iter().map(|(s, e)| (e - s).max(0.0)).sum();
            format!("Reel ({:.0}s, {} cuts)", total_dur, clip.segments.len())
        } else {
            clip.lesson.chars().take(48).collect()
        };
        options.push(HuntOption {
            window_id: id,
            label,
            start_s: clip.start_s,
            end_s: clip.end_s,
            segments: clip.segments.clone(),
            motion: clip.motion.as_str().to_string(),
            caption: clip.captions.first().map(|cue| cue.text.clone()).unwrap_or_default(),
            hook: clip.hooks.first().cloned().unwrap_or_default(),
            hooks: clip.hooks.clone(),
            h: clip.scores.h,
            v: clip.scores.v,
            e: clip.scores.e,
            a: clip.scores.a,
            p: clip.scores.p,
            s: clip.scores.s,
            lesson: clip.lesson.clone(),
            thumb,
            thumbs,
        });
    }
    if options.is_empty() {
        desk.pet.line = pet_state::nothing_to_cut_line();
        return;
    }
    // Rank options by score S descending so #1 best reel is first
    options.sort_by(|a, b| b.s.partial_cmp(&a.s).unwrap_or(std::cmp::Ordering::Equal));
    desk.pet.hunger = pet_state::hunger_after_hunt(desk.pet.hunger);
    desk.pet.collar = Some(best);
    desk.pet.line = if draft.disagrees {
        pet_state::stage_five_line(pet_state::LEVEL_READY_LINE)
    } else {
        pet_state::level_ready_line()
    };
    let kind = ChoreKind::Hunt { options };
    match desk.insert_view(kind, Some(source_id), None, None) {
        Ok(id) => desk.set_chore(id),
        Err(err) => desk.pet.line = pet_state::model_error_line(&err),
    }
}

fn write_thumbs(
    app_data: &Path,
    id: i64,
    origin: Option<&str>,
    seen: &[SeenFrame],
    clip: &crate::editor::actions::ClipOffer,
) -> (String, Vec<String>) {
    let in_clip = |t: f64| -> bool {
        if !clip.segments.is_empty() {
            clip.segments.iter().any(|(s, e)| t >= (*s - 0.5) && t <= (*e + 0.5))
        } else {
            t >= (clip.start_s - 0.5) && t <= (clip.end_s + 0.5)
        }
    };
    let mut matching: Vec<SeenFrame> = seen
        .iter()
        .filter(|f| in_clip(f.t))
        .cloned()
        .collect();

    matching.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    matching.dedup_by(|a, b| (a.t - b.t).abs() < 0.2);

    // If we have fewer than 3 frames and origin is available, sample additional frames
    if matching.len() < 3 {
        if let Some(origin_path) = origin {
            let mut target_times = Vec::new();
            if !clip.segments.is_empty() {
                for &(s, e) in &clip.segments {
                    let dur = (e - s).max(0.1);
                    let count = if dur >= 4.0 { 3 } else if dur >= 1.5 { 2 } else { 1 };
                    for step in 0..count {
                        let frac = (step as f64 + 0.5) / (count as f64);
                        target_times.push(s + dur * frac);
                    }
                }
            } else {
                let dur = (clip.end_s - clip.start_s).max(0.1);
                for step in 0..4 {
                    let frac = (step as f64 + 0.5) / 4.0;
                    target_times.push(clip.start_s + dur * frac);
                }
            }
            if let Ok(samples) = media::sample_times(Path::new(origin_path), &target_times) {
                for s in samples {
                    matching.push(SeenFrame { t: s.t, jpeg: s.jpeg });
                }
                matching.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
                matching.dedup_by(|a, b| (a.t - b.t).abs() < 0.2);
            }
        }
    }

    // Fallback if still empty: pick closest frame from seen
    if matching.is_empty() {
        if let Some(frame) = seen.iter().min_by(|a, b| {
            (a.t - clip.start_s)
                .abs()
                .partial_cmp(&(b.t - clip.start_s).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            matching.push(frame.clone());
        }
    }

    let dir = app_data.join("thumbs");
    if std::fs::create_dir_all(&dir).is_err() || matching.is_empty() {
        return (String::new(), Vec::new());
    }

    let mut paths = Vec::new();
    for (i, frame) in matching.iter().enumerate() {
        let p = dir.join(format!("{id}_{i}.jpg"));
        if std::fs::write(&p, &frame.jpeg).is_ok() {
            paths.push(p.display().to_string());
        }
    }

    let primary_path = dir.join(format!("{id}.jpg"));
    if let Some(first) = matching.first() {
        let _ = std::fs::write(&primary_path, &first.jpeg);
    }
    let primary_str = primary_path.display().to_string();
    if paths.is_empty() && primary_path.is_file() {
        paths.push(primary_str.clone());
    }

    (primary_str, paths)
}

fn hunt_options(desk: &Desk) -> Vec<HuntOption> {
    match &desk.chore {
        Some(view) => match &view.kind {
            ChoreKind::Hunt { options } => options.clone(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn record_pick(desk: &Desk, options: &[HuntOption], window_id: i64) {
    let Some(picked) = options.iter().find(|option| option.window_id == window_id) else {
        return;
    };
    if !picked.motion.is_empty() {
        let _ = desk.db.insert_taste(&picked.motion, Some(window_id));
        let tag = crate::score::strongest_tag(picked.h, picked.v, picked.e, picked.a, picked.p);
        let _ = desk.db.insert_taste(tag, Some(window_id));
    }
    for other in options.iter().filter(|option| option.window_id != window_id) {
        if other.motion.is_empty() {
            continue;
        }
        let _ = desk.db.insert_taste(&taste::passed_tag(&other.motion), Some(other.window_id));
    }
}

fn first_hook(desk: &Desk, window_id: i64) -> String {
    desk.db
        .window(window_id)
        .ok()
        .and_then(|row| row.hooks_json)
        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .and_then(|hooks| hooks.into_iter().find(|hook| !hook.is_empty()))
        .unwrap_or_default()
}

fn reveal_saved(path: &str) {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut cmd = std::process::Command::new("open");
        cmd.arg("-R").arg(path);
        cmd
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut cmd = std::process::Command::new("explorer");
        cmd.arg(format!("/select,{path}"));
        cmd
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut cmd = {
        let mut cmd = std::process::Command::new("xdg-open");
        cmd.arg(path);
        cmd
    };
    let _ = cmd.spawn();
}

fn reject_card(desk: &mut Desk, chore_id: i64, window_id: i64, reason: &str) {
    if !taste::REASONS.contains(&reason) || !matches_open(desk, chore_id) {
        return;
    }
    let Some(view) = desk.chore.clone() else { return };
    let ChoreKind::Hunt { mut options } = view.kind else { return };
    if !options.iter().any(|option| option.window_id == window_id) {
        return;
    }
    let _ = desk.db.insert_taste(reason, Some(window_id));
    options.retain(|option| option.window_id != window_id);
    if options.is_empty() {
        let _ = desk.db.resolve_chore(chore_id, "resolved", &format!(r#"{{"reject":"{reason}"}}"#));
        desk.chore = None;
        desk.pet.collar = None;
        desk.ensure_remix();
        return;
    }
    let next = ChoreView {
        id: chore_id,
        kind: ChoreKind::Hunt { options },
    };
    if let Ok(payload) = serde_json::to_string(&next) {
        let _ = desk.db.rewrite_chore(chore_id, &payload);
        desk.set_chore(chore_id);
    }
}

pub fn normalize_local_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let trimmed = s.trim().trim_matches('"').trim_matches('\'');
    if trimmed.starts_with("file://") {
        let stripped = trimmed.trim_start_matches("file://");
        let decoded = percent_decode(stripped);
        #[cfg(target_os = "windows")]
        let decoded = decoded.trim_start_matches('/');
        return PathBuf::from(decoded);
    }
    PathBuf::from(trimmed)
}

fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                let hex = [h1, h2];
                if let Ok(hex_str) = std::str::from_utf8(&hex) {
                    if let Ok(byte) = u8::from_str_radix(hex_str, 16) {
                        bytes.push(byte);
                        continue;
                    }
                }
                bytes.push(b'%');
                bytes.push(h1);
                bytes.push(h2);
            } else {
                bytes.push(b'%');
            }
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::drive::Talk;

    struct Script {
        lines: Vec<String>,
        i: usize,
    }

    impl Talk for Script {
        fn generate(&mut self, _prompt: &str, _jpegs: &[Vec<u8>]) -> Result<String, String> {
            let line = self
                .lines
                .get(self.i)
                .cloned()
                .unwrap_or_else(|| r#"{"action":"done"}"#.into());
            self.i += 1;
            Ok(line)
        }
    }

    #[test]
    fn stage_three_drops_a_rejected_shape() {
        let mut script = Script {
            lines: vec![
                r#"{"action":"propose","moments":[{"start_s":12,"end_s":31,"why":"slow_open again"}]}"#.into(),
                r#"{"action":"done"}"#.into(),
            ],
            i: 0,
        };
        let draft = drive::drive(
            |prompt, jpegs| script.generate(prompt, jpegs),
            |_, _, _, _| Ok(vec![]),
            prompt::SYSTEM,
            "scout",
            &["slow_open".into()],
            3,
            3,
        )
        .unwrap();
        assert!(draft.moments.is_empty());
    }

    #[test]
    fn empty_inbox_with_a_clip_opens_one_remix() {
        let dir = std::env::temp_dir().join(format!(
            "reelbuddy-remix-{}-{}",
            std::process::id(),
            db::now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut desk = Desk::boot(&dir).unwrap();
        let source = desk.db.insert_source("old.mp4", Some(30.0), "ready", None).unwrap();
        let window = desk.db.insert_window(source, 1.0, 20.0, "rendered").unwrap();
        desk.db.insert_clip(window, "old.mp4", "hook", "ready").unwrap();
        desk.ensure_remix();
        assert!(matches!(desk.chore.as_ref().map(|c| &c.kind), Some(ChoreKind::Remix { .. })));
        let id = desk.chore.as_ref().unwrap().id;
        desk.ensure_remix();
        assert_eq!(desk.chore.as_ref().unwrap().id, id);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_local_path_decodes_spaces_and_file_uri() {
        let uri = Path::new("file:///Users/test/My%20Folder/clip%20one.mov");
        let decoded = normalize_local_path(uri);
        assert_eq!(decoded, PathBuf::from("/Users/test/My Folder/clip one.mov"));

        let plain = Path::new("/Users/test/regular_clip.mp4");
        assert_eq!(normalize_local_path(plain), PathBuf::from("/Users/test/regular_clip.mp4"));
    }

    #[test]
    fn ingest_without_brain_sets_missing_model_line() {
        use crate::media::video_init;
        use video_rs::{Encoder, Time};
        use ndarray::Array3;

        let dir = std::env::temp_dir().join(format!(
            "reelbuddy-nobrain-{}-{}",
            std::process::id(),
            db::now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut desk = Desk::boot(&dir).unwrap();

        let video_path = dir.join("dummy.mp4");
        video_init();
        let settings = video_rs::encode::Settings::preset_h264_yuv420p(320, 240, false);
        let mut encoder = Encoder::new(video_path.as_path(), settings).unwrap();
        for i in 0..5 {
            let frame = Array3::<u8>::zeros((240, 320, 3));
            encoder.encode(&frame, Time::from_secs_f64(i as f64 / 10.0)).unwrap();
        }
        encoder.finish().unwrap();

        let mut live = |_desk: &Desk, _event: Live| {};
        ingest::<Script, _>(&mut desk, None, &video_path, &mut live);

        assert_eq!(desk.pet.line, pet_state::missing_model_line());
        assert!(!desk.pet.busy);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ingest_with_brain_creates_hunt_options_and_collar_tag() {
        use crate::media::video_init;
        use video_rs::{Encoder, Time};
        use ndarray::Array3;

        let dir = std::env::temp_dir().join(format!(
            "reelbuddy-ingest-{}-{}",
            std::process::id(),
            db::now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut desk = Desk::boot(&dir).unwrap();

        let video_path = dir.join("test_reel.mp4");
        video_init();
        let settings = video_rs::encode::Settings::preset_h264_yuv420p(320, 240, false);
        let mut encoder = Encoder::new(video_path.as_path(), settings).unwrap();
        for i in 0..60 {
            let mut frame = Array3::<u8>::zeros((240, 320, 3));
            frame.slice_mut(ndarray::s![.., .., 0]).fill(((i * 7) % 255) as u8);
            frame.slice_mut(ndarray::s![.., .., 1]).fill(((i * 13) % 255) as u8);
            encoder.encode(&frame, Time::from_secs_f64(i as f64 / 10.0)).unwrap();
        }
        encoder.finish().unwrap();

        let offer_json = r#"{
            "action": "offer",
            "clips": [
                {
                    "segments": [
                        [0.0, 3.0],
                        [3.5, 6.0]
                    ],
                    "crop": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                    "motion": "hold",
                    "captions": [],
                    "hooks": ["Best Hook"],
                    "h": 85, "v": 90, "e": 75, "a": 80, "p": 70,
                    "note": "Great",
                    "lesson": "Epic multi-segment reel"
                }
            ]
        }"#;

        let mut script = Script {
            lines: vec![
                offer_json.to_string(),
                r#"{"action":"done"}"#.into(),
            ],
            i: 0,
        };

        let mut live = |_desk: &Desk, _event: Live| {};
        ingest(&mut desk, Some(&mut script), &video_path, &mut live);

        assert!(!desk.pet.busy);
        assert!(matches!(desk.chore.as_ref().map(|c| &c.kind), Some(ChoreKind::Hunt { .. })));
        let (chore_id, window_id) = match desk.chore.as_ref().map(|c| &c.kind) {
            Some(ChoreKind::Hunt { options, .. }) => {
                assert_eq!(options.len(), 1);
                assert_eq!(options[0].segments, vec![(0.0, 3.0), (3.5, 6.0)]);
                assert_eq!(desk.pet.collar, Some(82));
                (desk.chore.as_ref().unwrap().id, options[0].window_id)
            }
            other => panic!("expected a hunt, got {other:?}"),
        };

        struct RefuseModel;
        impl Talk for RefuseModel {
            fn generate(&mut self, _prompt: &str, _jpegs: &[Vec<u8>]) -> Result<String, String> {
                Err("model should not run".into())
            }
        }
        let mut refuse = RefuseModel;
        let mut live = |_desk: &Desk, _event: Live| {};
        handle_live(
            &mut desk,
            Some(&mut refuse),
            EditorCommand::Pick { chore_id, window_id },
            &mut live,
        );
        assert!(desk.chore.is_none());
        assert!(!desk.pet.busy);
        assert!(desk.pet.line.starts_with(pet_state::KEPT_LINE));
        assert!(!desk.pet.line.contains("model should not run"));
        let clip = desk.app_data.join("clips").join(format!("window-{window_id}.mp4"));
        assert!(clip.is_file(), "missing {}", clip.display());
        let tastes = desk.db.taste_counts().unwrap();
        assert!(tastes.iter().any(|(reason, _)| reason == "hold"));
        assert!(tastes.iter().any(|(reason, _)| reason == "virality"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
