use crate::editor::actions::{self, Action, ClipOffer, Cue, Framing, Moment, NormCrop, Scores};
use crate::pet_state;
use crate::score::is_scrap;
use crate::taste;

pub trait Talk {
    fn generate(&mut self, prompt: &str, jpegs: &[Vec<u8>]) -> Result<String, String>;
}

#[derive(Clone, Debug)]
pub struct SeenFrame {
    pub t: f64,
    pub jpeg: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub moments: Vec<Moment>,
    pub offers: Vec<ClipOffer>,
    pub seen: Vec<SeenFrame>,
    pub crop: Option<NormCrop>,
    pub framing: Option<Framing>,
    pub captions: Vec<Cue>,
    pub hooks: Vec<String>,
    pub scores: Option<Scores>,
    pub disagrees: bool,
}

pub fn rejection_line(err: &str) -> String {
    if err == "no json object" {
        "Tool result: rejected (no json object). You may reason through the clips, then reply with one JSON object: if slides or black frames, {\"action\":\"done\"}, otherwise start with {\"action\":\"offer\",\"clips\":[".to_string()
    } else {
        format!("Tool result: rejected ({err}). Reply with one JSON object.")
    }
}

pub fn drive<G, S>(
    generate: G,
    sample: S,
    system: &str,
    task: &str,
    banned: &[String],
    stage: u8,
    offer_cap: usize,
) -> Result<Draft, String>
where
    G: FnMut(&str, &[Vec<u8>]) -> Result<String, String>,
    S: FnMut(f64, f64, f64, u32) -> Result<Vec<SeenFrame>, String>,
{
    drive_with_frames(generate, sample, system, task, Vec::new(), banned, stage, offer_cap)
}

pub fn drive_with_frames<G, S>(
    mut generate: G,
    mut sample: S,
    system: &str,
    task: &str,
    initial_frames: Vec<SeenFrame>,
    banned: &[String],
    stage: u8,
    offer_cap: usize,
) -> Result<Draft, String>
where
    G: FnMut(&str, &[Vec<u8>]) -> Result<String, String>,
    S: FnMut(f64, f64, f64, u32) -> Result<Vec<SeenFrame>, String>,
{
    let mut images: Vec<Vec<u8>> = initial_frames.iter().map(|f| f.jpeg.clone()).collect();
    let mut draft = Draft::default();
    let mut transcript = if !initial_frames.is_empty() {
        let stamps: Vec<String> = initial_frames.iter().map(|f| format!("{:.3}", f.t)).collect();
        draft.seen = initial_frames;
        format!("{system}\n\nTool result: frames at {} attached\n\n{task}", stamps.join(", "))
    } else {
        format!("{system}\n\n{task}")
    };
    let mut garbage = 0u8;
    for _ in 0..12 {
        let raw = match generate(&transcript, &images) {
            Ok(raw) => raw,
            Err(err) => {
                garbage += 1;
                if garbage >= 2 {
                    return Err(pet_state::cant_cut_line());
                }
                transcript = format!("{transcript}\n\nTool result: error {err}. Reply with one JSON object.");
                continue;
            }
        };
        let action = match actions::parse_action(&raw) {
            Ok(action) => {
                garbage = 0;
                action
            }
            Err(err) => {
                garbage += 1;
                if garbage >= 2 {
                    return Err(pet_state::cant_cut_line());
                }
                transcript = format!("{transcript}\n\n{}", rejection_line(&err));
                continue;
            }
        };
        images.clear();
        match action {
            Action::SampleFrames {
                start_s,
                end_s,
                every_s,
                max_frames,
            } => {
                let frames = sample(start_s, end_s, every_s.max(0.5), max_frames)?;
                let stamps: Vec<String> = frames.iter().map(|f| format!("{:.3}", f.t)).collect();
                draft.seen.extend(frames.iter().cloned());
                images = frames.into_iter().map(|f| f.jpeg).collect();
                transcript = format!(
                    "{system}\n\nTool result: frames at {} attached",
                    stamps.join(", ")
                );
            }
            Action::Propose { moments } => {
                let mut kept = Vec::new();
                for moment in moments.into_iter().take(3) {
                    let blocked = banned.iter().any(|reason| taste::shape_rejected(&moment.why, reason));
                    if blocked && stage == 3 {
                        continue;
                    }
                    if blocked && stage >= 5 {
                        draft.disagrees = true;
                    }
                    kept.push(moment);
                }
                draft.moments = kept;
            }
            Action::SetCrop(crop) => draft.crop = Some(crop),
            Action::SetFraming(framing) => draft.framing = Some(framing),
            Action::SetCaptions { cues } => draft.captions = cues,
            Action::SetHooks { hooks } => {
                draft.hooks = hooks
                    .into_iter()
                    .take(2)
                    .map(|hook| hook.chars().take(42).collect())
                    .collect();
            }
            Action::SetScores(scores) => draft.scores = Some(scores),
            Action::Offer { clips } => {
                let mut kept = Vec::new();
                for clip in clips {
                    if is_scrap(clip.scores.s) {
                        continue;
                    }
                    let blocked = banned.iter().any(|reason| {
                        taste::shape_rejected(&clip.lesson, reason)
                            || taste::shape_rejected(&clip.scores.note, reason)
                    });
                    if blocked && stage == 3 {
                        continue;
                    }
                    if blocked && stage >= 5 {
                        draft.disagrees = true;
                    }
                    kept.push(clip);
                    if kept.len() >= offer_cap.max(1) {
                        break;
                    }
                }
                draft.offers = kept;
                break;
            }
            Action::Done => break,
        }
    }
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thought_only_reply_is_told_to_start_the_offer() {
        let mut calls = 0;
        let draft = drive(
            |prompt, _| {
                calls += 1;
                if calls == 1 {
                    Ok("I looked at the frames.".into())
                } else {
                    assert!(prompt.contains("{\"action\":\"offer\",\"clips\":["));
                    assert!(prompt.contains("{\"action\":\"done\"}"));
                    Ok(r#"{"action":"done"}"#.into())
                }
            },
            |_start, _end, _every, _max| Ok(Vec::new()),
            "sys",
            "task",
            &[],
            1,
            3,
        )
        .unwrap();
        assert!(draft.offers.is_empty());
        assert_eq!(calls, 2);
    }

    #[test]
    fn slides_or_black_frames_text_reply_is_accepted_as_done() {
        let draft = drive(
            |_prompt, _| Ok("The window is slides or black frames, offer nothing and done.".into()),
            |_start, _end, _every, _max| Ok(Vec::new()),
            "sys",
            "task",
            &[],
            1,
            3,
        )
        .unwrap();
        assert!(draft.offers.is_empty());
    }

    #[test]
    fn offer_reply_breaks_loop_in_one_call() {
        let mut calls = 0;
        let draft = drive(
            |_prompt, _| {
                calls += 1;
                Ok(r#"{"action":"offer","clips":[{"segments":[{"start_s":1.0,"end_s":5.0}],"framing":{"mode":"crop","focus_x":0.5},"motion":"hold","hooks":["the pause lands"],"captions":[{"t":0.4,"text":"look here","effect":"bounce"}],"h":80,"v":80,"e":70,"a":70,"p":70,"note":"Opens on a face.","lesson":"The opening is the claim."}]}"#.into())
            },
            |_start, _end, _every, _max| Ok(Vec::new()),
            "sys",
            "task",
            &[],
            1,
            3,
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(draft.offers.len(), 1);
    }
}
