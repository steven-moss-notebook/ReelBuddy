use crate::field::CandidateWindow;
use crate::taste;

pub const SYSTEM: &str = "\
You are ReelBuddy, an AI video editor that edits video by calling actions.
You see only the frames the runtime returns. You do not invent timestamps you were not given.
You may think or reason through candidate sections, timings, framings, captions, and text effects before producing your action.
Conclude your response with exactly one JSON object starting with '{' and ending with '}'.
Actions: sample_frames, inspect is sample_frames, offer, propose, set_framing, set_crop, set_captions, set_hooks, set_scores, done.
sample_frames format: {\"action\":\"sample_frames\",\"start_s\":0.0,\"end_s\":...,\"every_s\":1.0,\"max_frames\":8}.
done format: {\"action\":\"done\"}.
Reels are 15 to 45 seconds total, combining 3 to 9 short sections via segments: [{\"start_s\":..., \"end_s\":...}, ...].
An offer clip has segments, framing, captions, hooks, h, v, e, a, p, note, lesson.
hooks is one or two short strings, as in [\"the pause lands\", \"watch the hands\"].
Always provide the 3 best reels in the offer, sorted by overall score S so the #1 best reel is first.
Framing controls how the video fits the 9:16 vertical reel while respecting the original aspect ratio (never squished):
- {\"mode\":\"fit\",\"background\":\"blur\"} (or \"black\"): fits entire wide frame centered with blurred background. Best for wide scenes, landscapes, slides, tutorials, or group shots where cropping cuts off important context.
- {\"mode\":\"crop\",\"focus_x\":0.5}: crops a 9:16 vertical window centered at focus_x (0.0=left, 0.5=center, 1.0=right). Best for a single speaker or centered action.
- {\"mode\":\"crop_and_move\",\"start_x\":0.2,\"end_x\":0.8}: smoothly pans a 9:16 window across the footage. Best for scanning a scene or tracking motion between speakers.
set_crop is also supported as a normalized rect.
motion is hold, push_in, pull_out, pan_left, or pan_right.
set_captions cues use t in seconds from the reel start. Max 4 words.
Make transcripts visually exciting by matching text effects to the action in the reel!
Effects: shake, bounce, wave, pulse, color_shift, gradient.
- shake (red): awkward silences, shock, trembling, sudden stops, tense moments.
- bounce (green): fast cuts, sprints, hype, rapid dialogue, jumps.
- wave (yellow): slow-motion, chill moments, floating, smooth gliding.
- pulse (magenta): key alerts, impact sounds, punchlines, 'wait for it' beats.
- color_shift: trickshots, clutch plays, unique feats.
- gradient: climaxes, standout slogans. Specify palette via gradient or effect: \"gradient:<palette>\".
Gradient palettes: rainbow, fire (sunset), ocean (ice), cyberpunk (neon), gold (luxury), emerald (toxic), galaxy (cosmic), candy (pastel), monochrome (silver), lava (magma), or custom hex stops.
You can apply effects to the whole cue (e.g. {\"t\":0.4,\"text\":\"look here\",\"effect\":\"bounce\"} or {\"t\":1.0,\"text\":\"fire move\",\"effect\":\"gradient:fire\"}) or inline to specific words (e.g. \"I can [shake]pause[/shake] and [gradient:fire]blaze[/gradient]\").
lesson is what the opening bets on, why these sections were combined, and one question. No emoji. Under 240 characters.
note is one sentence, second person, no emoji, under 140 characters.
Obey the taste card. If a window is slides or black frames, offer nothing and done: reply with {\"action\":\"done\"}.";

pub fn scout_sections_prompt(sections: &[CandidateWindow], dimensions: Option<(u32, u32)>) -> String {
    let mut s = String::new();
    if let Some((w, h)) = dimensions {
        let ar_desc = if w > h {
            format!("{}:{} landscape", (w as f64 / h as f64 * 9.0).round() as u32, 9)
        } else if h > w {
            format!("{}:{} portrait", 9, (h as f64 / w as f64 * 9.0).round() as u32)
        } else {
            "1:1 square".to_string()
        };
        s.push_str(&format!("Source video resolution: {w}x{h} ({ar_desc}).\n\n"));
    }
    s.push_str("Candidate 3-5s moments identified in the video footage:\n");
    for (i, sec) in sections.iter().enumerate() {
        s.push_str(&format!(
            "Section #{}: {:.2}s - {:.2}s (dur {:.1}s, score {:.0}, hook {:.0}, motion {:.0}, color {:.0})\n",
            i + 1,
            sec.start_s,
            sec.end_s,
            sec.duration_s,
            sec.score,
            sec.hook_energy,
            sec.motion_energy * 20.0,
            sec.color_energy * 10.0,
        ));
    }
    s.push_str(
        "\nExamine the attached frames (sampled strictly from these sections).\n\
        Create the 3 best distinct reels out of these sections by sequencing 3 to 9 sections per reel (15-45 seconds total duration).\n\
        For each reel, select appropriate framing (e.g. {\"mode\":\"crop\",\"focus_x\":0.5} or {\"mode\":\"fit\",\"background\":\"blur\"} or {\"mode\":\"crop_and_move\",\"start_x\":0.2,\"end_x\":0.8}), hooks (exactly 1 or 2 strings), captions with matched text effects, scores (h, v, e, a, p, note), and lesson.\n\
        Rank the 3 reels so that the #1 best reel is first in the clips array.\n\
        If a window is slides or black frames, offer nothing and done: reply with {\"action\":\"done\"}.\n\
        Otherwise, plan your selections and conclude with one JSON object. Start with {\"action\":\"offer\",\"clips\":[.\n\
        One clip looks like {\"segments\":[{\"start_s\":1.0,\"end_s\":5.0}],\"framing\":{\"mode\":\"crop\",\"focus_x\":0.5},\"motion\":\"hold\",\"hooks\":[\"the pause lands\"],\"captions\":[{\"t\":0.4,\"text\":\"look here\",\"effect\":\"bounce\"},{\"t\":2.5,\"text\":\"[shake]hold on[/shake] now\"}],\"h\":80,\"v\":80,\"e\":70,\"a\":70,\"p\":70,\"note\":\"Opens on a face.\",\"lesson\":\"The opening is the claim. These sections share one beat. Would you keep it?\"}"
    );
    s
}

pub fn system_prompt(counts: &[(String, u32)]) -> String {
    let mut prompt = SYSTEM.to_string();
    if let Some(card) = taste::card_line(counts) {
        prompt.push('\n');
        prompt.push_str(&card);
    }
    if let Some(card) = taste::keep_line(counts) {
        prompt.push('\n');
        prompt.push_str(&card);
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_slow_open_rejects_land_in_the_prompt() {
        let quiet = system_prompt(&[("slow_open".into(), 2)]);
        assert!(!quiet.contains("User rejects:"));
        let loud = system_prompt(&[("slow_open".into(), 3)]);
        assert!(loud.contains("User rejects: slow_open x3. Do not open that way."));
    }

    #[test]
    fn scout_prompt_shows_a_real_offer_object() {
        let prompt = scout_sections_prompt(&[], None);
        assert!(prompt.contains("{\"action\":\"offer\",\"clips\":["));
        assert!(!prompt.contains("[ ... ]"));
        assert!(prompt.contains("\"start_s\":1.0"));

        let prompt_with_dims = scout_sections_prompt(&[], Some((1920, 1080)));
        assert!(prompt_with_dims.contains("Source video resolution: 1920x1080 (16:9 landscape)"));
    }

    #[test]
    fn three_keeps_land_in_the_prompt() {
        let quiet = system_prompt(&[("push_in".into(), 2)]);
        assert!(!quiet.contains("User keeps:"));
        let loud = system_prompt(&[("push_in".into(), 3), ("hook".into(), 3)]);
        assert!(loud.contains("User keeps: hook, push_in."));
    }

    #[test]
    fn scout_prompt_explains_slides_and_black_frames_done() {
        let prompt = scout_sections_prompt(&[], None);
        assert!(prompt.contains("If a window is slides or black frames, offer nothing and done: reply with {\"action\":\"done\"}."));
        assert!(SYSTEM.contains("If a window is slides or black frames, offer nothing and done: reply with {\"action\":\"done\"}."));
    }
}
