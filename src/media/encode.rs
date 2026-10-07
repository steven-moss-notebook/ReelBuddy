use std::path::Path;

use video_rs::{Encoder, Time};

use crate::editor::{Cue, Framing, Motion, NormCrop};
use crate::media::composite::{active_cue, render_vertical_with_framing};
use crate::media::decode::{frames_between, video_init};

pub fn encode_segments(
    source: &Path,
    dest: &Path,
    segments: &[(f64, f64)],
    framing: &Framing,
    captions: &[Cue],
    hook: &str,
) -> Result<(), String> {
    if segments.is_empty() {
        return Err("no segments to encode".into());
    }
    let total_duration: f64 = segments.iter().map(|(s, e)| (e - s).max(0.0)).sum();
    if total_duration > 60.0 {
        return Err("window over 60s".into());
    }
    video_init();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let settings = video_rs::encode::Settings::preset_h264_yuv420p(1080, 1920, false);
    let mut encoder = Encoder::new(dest, settings).map_err(|err| err.to_string())?;

    let mut running_t = 0.0f64;
    let mut total_frames_encoded = 0;

    for &(start_s, end_s) in segments {
        let seg_frames = frames_between(source, start_s, end_s)?;
        if seg_frames.is_empty() {
            continue;
        }
        let mut prev_frame_t: Option<f64> = None;
        for (t, frame) in seg_frames {
            if let Some(prev) = prev_frame_t {
                let dt = (t - prev).clamp(0.001, 0.1);
                running_t += dt;
            } else if total_frames_encoded > 0 {
                running_t += 1.0 / 30.0;
            }
            prev_frame_t = Some(t);

            let span = total_duration.max(0.001);
            let u = (running_t / span).clamp(0.0, 1.0);
            let cue = active_cue(captions, running_t);
            let hook = if running_t <= 1.5 { Some(hook) } else { None };
            let rendered = render_vertical_with_framing(&frame, framing, u, cue, hook, running_t)?;
            encoder
                .encode(&rendered, Time::from_secs_f64(running_t))
                .map_err(|err| err.to_string())?;
            total_frames_encoded += 1;
        }
    }

    if total_frames_encoded == 0 {
        return Err("no frames in window".into());
    }

    encoder.finish().map_err(|err| err.to_string())?;
    Ok(())
}

pub fn encode_segments_with_crop(
    source: &Path,
    dest: &Path,
    segments: &[(f64, f64)],
    crop: &NormCrop,
    motion: Motion,
    captions: &[Cue],
    hook: &str,
) -> Result<(), String> {
    let framing = match motion {
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
            if crop.w >= 0.95 && crop.h >= 0.95 {
                Framing::Fit {
                    background: crate::editor::BackgroundStyle::Blur,
                }
            } else {
                Framing::Crop {
                    focus_x: (crop.x + crop.w * 0.5).clamp(0.0, 1.0),
                    focus_y: (crop.y + crop.h * 0.5).clamp(0.0, 1.0),
                    zoom: crop.h.clamp(0.1, 1.0),
                }
            }
        }
    };
    encode_segments(source, dest, segments, &framing, captions, hook)
}

pub fn encode_window(
    source: &Path,
    dest: &Path,
    start_s: f64,
    end_s: f64,
    framing: &Framing,
    captions: &[Cue],
    hook: &str,
) -> Result<(), String> {
    encode_segments(source, dest, &[(start_s, end_s)], framing, captions, hook)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array3;

    fn fixture_source() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("reelbuddy-enc-source-{}.mp4", std::process::id()));
        if path.exists() {
            return path;
        }
        video_init();
        let settings = video_rs::encode::Settings::preset_h264_yuv420p(320, 240, false);
        let mut encoder = Encoder::new(path.as_path(), settings).unwrap();
        for i in 0..6 {
            let mut frame = Array3::<u8>::zeros((240, 320, 3));
            frame.slice_mut(ndarray::s![.., .., 0]).fill((i * 40).min(255) as u8);
            encoder
                .encode(&frame, Time::from_secs_f64(i as f64 / 10.0))
                .unwrap();
        }
        encoder.finish().unwrap();
        path
    }

    #[test]
    fn encode_segments_stitches_and_outputs_video() {
        let source = fixture_source();
        let dest = std::env::temp_dir().join(format!("reelbuddy-stitched-{}.mp4", std::process::id()));
        let framing = Framing::Crop { focus_x: 0.5, focus_y: 0.5, zoom: 1.0 };
        let segments = vec![(0.1, 0.2), (0.3, 0.4)];
        let res = encode_segments(
            &source,
            &dest,
            &segments,
            &framing,
            &[],
            "Hook Text",
        );
        assert!(res.is_ok(), "{res:?}");
        assert!(dest.exists());
        let _ = std::fs::remove_file(&dest);
    }
}
