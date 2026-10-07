//! Editor thread. Bevy sends commands; this thread owns the pet and the model.

mod actions;
mod brain;
pub mod downloader;
#[path = "loop.rs"]
mod drive;
mod model;
mod prompt;
mod session;
mod view;

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

use bevy::prelude::*;

use crate::pet_state::{self, PetSnapshot};
use session::Desk;

pub use actions::{parse_action, Action, BackgroundStyle, Cue, Framing, Motion, NormCrop, Scores};
#[allow(unused_imports)]
pub use model::{app_data_dir, Config};
pub use session::SectionFind;
pub use view::{ChoreKind, ChoreView, HuntOption};

#[derive(Clone, Debug)]
pub enum EditorCommand {
    BumpHunger,
    Ingest(PathBuf),
    Pick { chore_id: i64, window_id: i64 },
    RejectClip { chore_id: i64, window_id: i64, reason: String },
    AcceptCut { chore_id: i64 },
    Nudge { chore_id: i64, delta: f64 },
    Reject { chore_id: i64, reason: String },
    PickHook { chore_id: i64, hook: String },
    Ship { chore_id: i64 },
    Hold { chore_id: i64 },
    RemixTry { chore_id: i64 },
    RemixSkip { chore_id: i64 },
    Shutdown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModelProgress {
    Idle,
    CheckingCache,
    Downloading {
        file: String,
        pct: f32,
        current_bytes: u64,
        total_bytes: u64,
    },
    Loading {
        pct: f32,
        stage: String,
    },
    Scanning {
        done: u32,
        total: u32,
    },
    Choosing,
    Ready,
    Error(String),
}

impl Default for ModelProgress {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Clone, Debug)]
pub struct EditorUpdate {
    pub pet: PetSnapshot,
    pub chore: Option<ChoreView>,
    pub progress: ModelProgress,
}

enum EditorEvent {
    Update(EditorUpdate),
    Spawns(Vec<crate::field::FieldSpawn>),
    Finds(Vec<SectionFind>),
}

#[derive(Message, Clone, Debug)]
pub struct CrateBatch(pub Vec<crate::field::FieldSpawn>);

#[derive(Message, Clone, Debug)]
pub struct SandFinds(pub Vec<SectionFind>);

pub struct EditorApi {
    pub(crate) commands: Sender<EditorCommand>,
}

impl EditorApi {
    pub fn send(&self, command: EditorCommand) {
        let _ = self.commands.send(command);
    }

    pub fn bump_hunger(&self) {
        self.send(EditorCommand::BumpHunger);
    }

    pub fn ingest(&self, path: PathBuf) {
        self.send(EditorCommand::Ingest(path));
    }

    pub fn shutdown(&self) {
        self.send(EditorCommand::Shutdown);
    }
}

#[derive(Resource)]
pub struct EditorLink {
    pub api: Mutex<EditorApi>,
    events: Mutex<Receiver<EditorEvent>>,
    debug: bool,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl EditorLink {
    pub fn shutdown(&self) {
        if let Ok(api) = self.api.lock() {
            api.shutdown();
        }
        if let Ok(mut guard) = self.thread.lock() {
            if let Some(handle) = guard.take() {
                if let Err(err) = handle.join() {
                    eprintln!("reelbuddy: error joining editor thread: {err:?}");
                }
            }
        }
    }
}

impl Drop for EditorLink {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn shutdown_on_exit(
    mut exit_events: MessageReader<AppExit>,
    link: Res<EditorLink>,
) {
    if exit_events.read().last().is_some() {
        link.shutdown();
    }
}

#[derive(Message, Clone, Debug)]
pub struct PetUpdated(pub EditorUpdate);

pub fn spawn(debug: bool) -> EditorLink {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (evt_tx, evt_rx) = mpsc::channel();
    let handle = std::thread::Builder::new()
        .name("reelbuddy-editor".into())
        .spawn(move || editor_main(cmd_rx, evt_tx, debug))
        .expect("editor thread");
    EditorLink {
        api: Mutex::new(EditorApi { commands: cmd_tx }),
        events: Mutex::new(evt_rx),
        debug,
        thread: Mutex::new(Some(handle)),
    }
}

fn editor_main(commands: Receiver<EditorCommand>, events: Sender<EditorEvent>, debug: bool) {
    let app_data = model::app_data_dir();
    let mut desk = match session::Desk::boot(&app_data) {
        Ok(desk) => desk,
        Err(err) => {
            let _ = events.send(EditorEvent::Update(EditorUpdate {
                pet: PetSnapshot::seed_asleep(pet_state::model_error_line(&err)),
                chore: None,
                progress: ModelProgress::Error(err),
            }));
            while let Ok(cmd) = commands.recv() {
                if matches!(cmd, EditorCommand::Shutdown) {
                    break;
                }
            }
            return;
        }
    };
    desk.pet.line = pet_state::boot_line();
    publish(&events, &desk, debug);

    let mut config = model::load_or_create(&app_data).ok();
    let mut brain = None;

    if let Some(cfg) = config.as_mut() {
        if let Some((model, mmproj, mtp)) = downloader::check_models_available(cfg) {
            cfg.model_path = model;
            cfg.mmproj_path = mmproj;
            if let Some(m) = mtp {
                cfg.mtp_path = m;
            }
            desk.pet.busy = true;
            desk.progress = ModelProgress::Loading {
                pct: 0.25,
                stage: "Cached models found. Settling neural sandscape...".to_string(),
            };
            desk.pet.line = pet_state::boot_line();
            publish(&events, &desk, debug);
            if debug {
                eprintln!("reelbuddy: loading 0");
            }
            match brain::Brain::load(cfg) {
                Ok(b) => {
                    if debug {
                        eprintln!("reelbuddy: loading 1");
                    }
                    desk.pet.model_present = true;
                    desk.pet.busy = false;
                    desk.progress = ModelProgress::Ready;
                    desk.pet.line = pet_state::seed_line();
                    brain = Some(b);
                }
                Err(err) => {
                    desk.pet.model_present = false;
                    desk.pet.busy = false;
                    desk.progress = ModelProgress::Error(err.clone());
                    desk.pet.line = pet_state::model_error_line(&err);
                }
            }
        } else {
            // Models not detected in local cache: initiate auto-download via hf-hub
            desk.pet.model_present = false;
            desk.pet.busy = true;
            desk.progress = ModelProgress::CheckingCache;
            desk.pet.line = format!("Connecting to Hugging Face: {}", cfg.model_repo);
            publish(&events, &desk, debug);

            let (dl_tx, dl_rx) = mpsc::channel();
            let _handle = downloader::spawn_auto_downloader(cfg.clone(), dl_tx);

            // Process download events
            'download: loop {
                match commands.try_recv() {
                    Ok(EditorCommand::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => return,
                    _ => {}
                }
                let progress = match dl_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(p) => p,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break 'download,
                };
                match progress {
                    downloader::DownloadProgress::CheckingCache => {
                        desk.progress = ModelProgress::CheckingCache;
                        desk.pet.line = "Checking local Hugging Face cache...".to_string();
                        publish(&events, &desk, debug);
                    }
                    downloader::DownloadProgress::CachedFound { model, mmproj, mtp } => {
                        cfg.model_path = model;
                        cfg.mmproj_path = mmproj;
                        if let Some(m) = mtp {
                            cfg.mtp_path = m;
                        }
                        desk.progress = ModelProgress::Loading {
                            pct: 0.25,
                            stage: "Found cached weights. Settling sandscape...".to_string(),
                        };
                        desk.pet.line = "Found model in Hugging Face cache!".to_string();
                        publish(&events, &desk, debug);
                    }
                    downloader::DownloadProgress::Downloading { file, pct, current_bytes, total_bytes } => {
                        desk.progress = ModelProgress::Downloading {
                            file: file.clone(),
                            pct,
                            current_bytes,
                            total_bytes,
                        };
                        desk.pet.line = format!("Downloading {file}: {:.0}%", pct);
                        publish(&events, &desk, debug);
                    }
                    downloader::DownloadProgress::Complete { model, mmproj, mtp } => {
                        cfg.model_path = model;
                        cfg.mmproj_path = mmproj;
                        if let Some(m) = mtp {
                            cfg.mtp_path = m;
                        }
                        desk.progress = ModelProgress::Loading {
                            pct: 0.85,
                            stage: "Weights downloaded. Crystallizing neural strata...".to_string(),
                        };
                        desk.pet.line = "Model download complete! Initializing...".to_string();
                        publish(&events, &desk, debug);
                        match brain::Brain::load(cfg) {
                            Ok(b) => {
                                desk.pet.model_present = true;
                                desk.pet.busy = false;
                                desk.progress = ModelProgress::Ready;
                                desk.pet.line = pet_state::seed_line();
                                brain = Some(b);
                            }
                            Err(err) => {
                                desk.pet.model_present = false;
                                desk.pet.busy = false;
                                desk.progress = ModelProgress::Error(err.clone());
                                desk.pet.line = pet_state::model_error_line(&err);
                            }
                        }
                        break 'download;
                    }
                    downloader::DownloadProgress::Error(err) => {
                        desk.pet.model_present = false;
                        desk.pet.busy = false;
                        desk.progress = ModelProgress::Error(err.clone());
                        desk.pet.line = pet_state::model_error_line(&format!("Download failed: {err}"));
                        publish(&events, &desk, debug);
                        break 'download;
                    }
                }
            }
        }
    } else {
        desk.pet.model_present = false;
        desk.progress = ModelProgress::Error("Config not available".into());
        desk.pet.line = pet_state::missing_model_line();
    }
    if desk.chore.is_none() {
        desk.ensure_remix();
    }
    desk.refresh();
    desk.persist();
    publish(&events, &desk, debug);

    while let Ok(command) = commands.recv() {
        if matches!(command, EditorCommand::Shutdown) {
            break;
        }
        if debug {
            eprintln!("reelbuddy: command {command:?}");
        }
        session::handle_live(&mut desk, brain.as_mut(), command, &mut |desk, live| match live {
            session::Live::Pulse => {
                let _ = events.send(EditorEvent::Update(EditorUpdate {
                    pet: desk.pet.clone(),
                    chore: desk.chore.clone(),
                    progress: desk.progress.clone(),
                }));
            }
            session::Live::Spawns(spawns) => {
                if !spawns.is_empty() {
                    let _ = events.send(EditorEvent::Spawns(spawns));
                }
            }
            session::Live::Finds(finds) => {
                if !finds.is_empty() {
                    let _ = events.send(EditorEvent::Finds(finds));
                }
            }
        });
        publish(&events, &desk, debug);
    }
}

fn publish(events: &Sender<EditorEvent>, desk: &Desk, debug: bool) {
    if debug {
        eprintln!(
            "reelbuddy: hunger {} sprite {:?} line {:?}",
            desk.pet.hunger,
            desk.pet.sprite(),
            desk.pet.line
        );
    }
    let _ = events.send(EditorEvent::Update(EditorUpdate {
        pet: desk.pet.clone(),
        chore: desk.chore.clone(),
        progress: desk.progress.clone(),
    }));
}

pub fn pump(
    link: Res<EditorLink>,
    mut out: MessageWriter<PetUpdated>,
    mut spawns: MessageWriter<CrateBatch>,
    mut finds: MessageWriter<SandFinds>,
) {
    let Ok(events) = link.events.lock() else {
        return;
    };
    while let Ok(event) = events.try_recv() {
        match event {
            EditorEvent::Update(update) => {
                out.write(PetUpdated(update));
            }
            EditorEvent::Spawns(batch) => {
                spawns.write(CrateBatch(batch));
            }
            EditorEvent::Finds(batch) => {
                finds.write(SandFinds(batch));
            }
        }
    }
}

#[cfg(test)]
mod channel_tests {
    use super::*;

    #[test]
    fn hunger_command_wakes_the_sprite_before_it_is_hungry() {
        let dir = std::env::temp_dir().join(format!(
            "reelbuddy-hunger-{}-{}",
            std::process::id(),
            crate::db::now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut desk = session::Desk::boot(&dir).unwrap();
        desk.pet = PetSnapshot::seed_asleep(pet_state::missing_model_line());
        session::handle(&mut desk, None::<&mut brain::Brain>, EditorCommand::BumpHunger);
        assert_eq!(desk.pet.hunger, 50);
        assert_eq!(desk.pet.sprite(), pet_state::SpriteKind::Idle);
        session::handle(&mut desk, None::<&mut brain::Brain>, EditorCommand::BumpHunger);
        session::handle(&mut desk, None::<&mut brain::Brain>, EditorCommand::BumpHunger);
        assert_eq!(desk.pet.hunger, 70);
        assert_eq!(desk.pet.sprite(), pet_state::SpriteKind::Hungry);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spawn_and_shutdown_joins_thread() {
        let link = spawn(false);
        assert!(link.thread.lock().unwrap().is_some());
        link.shutdown();
        assert!(link.thread.lock().unwrap().is_none());
    }
}
