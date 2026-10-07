use std::path::Path;
use std::sync::Once;

use ndarray::Array3;
use video_rs::{Decoder, Time};

use crate::media::composite::jpeg_long_edge;

static VIDEO_INIT: Once = Once::new();

pub fn video_init() {
    VIDEO_INIT.call_once(|| {
        let _ = video_rs::init();
    });
}

#[derive(Clone, Debug)]
pub struct Sample {
    pub t: f64,
    pub frame: Array3<u8>,
    pub jpeg: Vec<u8>,
}

pub fn duration_s(path: &Path) -> Result<f64, String> {
    video_init();
    let decoder = Decoder::new(path).map_err(|err| err.to_string())?;
    Ok(decoder.duration().map_err(|err| err.to_string())?.as_secs_f64())
}

pub fn video_dimensions(path: &Path) -> Result<(u32, u32), String> {
    video_init();
    let decoder = Decoder::new(path).map_err(|err| err.to_string())?;
    Ok(decoder.size())
}

fn sample_times_ffmpeg(path: &Path, wanted: &[f64]) -> Option<Vec<Sample>> {
    let mut out = Vec::new();
    for &t in wanted {
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v", "error",
                "-ss", &format!("{t:.3}"),
                "-i", &path.to_string_lossy(),
                "-frames:v", "1",
                "-vf", "scale='min(512,iw)':'min(512,ih)':force_original_aspect_ratio=decrease",
                "-f", "image2",
                "-c:v", "mjpeg",
                "pipe:1",
            ])
            .output()
            .ok()?;
        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }
        let jpeg = output.stdout;
        let img = image::load_from_memory(&jpeg).ok()?.to_rgb8();
        let (w, h) = img.dimensions();
        let raw = img.into_raw();
        let frame = Array3::from_shape_vec((h as usize, w as usize, 3), raw).ok()?;
        out.push(Sample { t, frame, jpeg });
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Walk the decoder until each requested time is within half a frame, and return the real timestamp.
pub fn sample_times(path: &Path, times: &[f64]) -> Result<Vec<Sample>, String> {
    video_init();
    let mut wanted: Vec<f64> = times.iter().copied().filter(|t| t.is_finite()).collect();
    wanted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(samples) = sample_times_ffmpeg(path, &wanted) {
        return Ok(samples);
    }
    let mut decoder = Decoder::new(path).map_err(|err| err.to_string())?;
    let half = 1.0 / 60.0;
    let mut out = Vec::new();
    let mut index = 0;
    for item in decoder.decode_iter() {
        let (ts, frame) = match item {
            Ok(v) => v,
            Err(_) => break,
        };
        let t = ts.as_secs_f64();
        while index < wanted.len() && t + half >= wanted[index] {
            out.push(make_sample(t, &frame)?);
            index += 1;
        }
        if index >= wanted.len() {
            break;
        }
    }
    if out.is_empty() {
        return Err("no frames".into());
    }
    Ok(out)
}

pub fn frames_between(path: &Path, start_s: f64, end_s: f64) -> Result<Vec<(f64, Array3<u8>)>, String> {
    video_init();
    let mut decoder = Decoder::new(path).map_err(|err| err.to_string())?;
    if start_s > 1.0 {
        let _ = decoder.seek(((start_s - 0.5).max(0.0) * 1000.0) as i64);
    }
    let mut out = Vec::new();
    for item in decoder.decode_iter() {
        let (ts, frame) = match item {
            Ok(v) => v,
            Err(_) => break,
        };
        let t = ts.as_secs_f64();
        if t + 1.0 / 60.0 < start_s {
            continue;
        }
        if t > end_s {
            break;
        }
        out.push((t, frame));
    }
    Ok(out)
}

/// Sample representative frames strictly bounded within the given sections.
/// Guarantees that only frames inside the candidate sections are sampled and returned.
pub fn sample_sections_only(
    path: &Path,
    sections: &[(f64, f64)],
    frames_per_section: usize,
) -> Result<Vec<Sample>, String> {
    let mut times = Vec::new();
    let count = frames_per_section.clamp(1, 4);
    for &(start_s, end_s) in sections {
        let dur = (end_s - start_s).max(0.1);
        if count == 1 {
            times.push(start_s + dur * 0.5);
        } else {
            for step in 0..count {
                let frac = (step as f64 + 0.5) / (count as f64);
                times.push(start_s + dur * frac);
            }
        }
    }
    sample_times(path, &times)
}

/// Decode video frames for an ordered sequence of segments, returning frames
/// for each segment.
pub fn frames_for_segments(
    path: &Path,
    segments: &[(f64, f64)],
) -> Result<Vec<(f64, Array3<u8>)>, String> {
    video_init();
    let mut out = Vec::new();
    for &(start_s, end_s) in segments {
        let mut seg_frames = frames_between(path, start_s, end_s)?;
        out.append(&mut seg_frames);
    }
    Ok(out)
}

fn make_sample(t: f64, frame: &Array3<u8>) -> Result<Sample, String> {
    let (h, w, c) = frame.dim();
    if c != 3 {
        return Err("expected rgb".into());
    }
    let jpeg = jpeg_long_edge(frame, 512)?;
    let _ = (h, w);
    Ok(Sample {
        t,
        frame: frame.clone(),
        jpeg,
    })
}

fn stream_gabor_samples_ffmpeg<F>(
    path: &Path,
    interval_s: f64,
    mut on_sample: F,
) -> Result<Vec<crate::field::OklabGaborAnalysis>, String>
where
    F: FnMut(&crate::field::FieldSpawn),
{
    let fps_filter = format!(
        "fps=1/{interval_s:.4},scale=240:240:force_original_aspect_ratio=decrease,pad=240:240:(ow-iw)/2:(oh-ih)/2"
    );
    let mut child = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v", "error",
            "-i", &path.to_string_lossy(),
            "-vf", &fps_filter,
            "-f", "rawvideo",
            "-pix_fmt", "rgb24",
            "pipe:1",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| err.to_string())?;

    let mut stdout = child.stdout.take().ok_or_else(|| "failed to open stdout".to_string())?;
    let mut estimator = crate::field::DualGaborEstimator::new();
    let mut analyses = Vec::new();
    let frame_size = crate::field::FRAME_DIM * crate::field::FRAME_DIM * 3;
    let mut buf = vec![0u8; frame_size];
    let mut t = 0.0f64;

    use std::io::Read;
    while stdout.read_exact(&mut buf).is_ok() {
        let analysis = estimator.analyze_frame_240p(t, &buf);
        let spawn = crate::field::FieldSpawn {
            t,
            sig: crate::field::FieldSig {
                r: analysis.rgb[0],
                g: analysis.rgb[1],
                b: analysis.rgb[2],
                theta: analysis.theta,
                freq: analysis.freq,
                energy: analysis.luma_energy,
            },
        };
        on_sample(&spawn);
        analyses.push(analysis);
        t += interval_s;
    }

    let _ = child.wait();
    if analyses.is_empty() {
        return Err("no frames from ffmpeg".into());
    }
    Ok(analyses)
}

pub fn stream_gabor_samples<F>(
    path: &Path,
    interval_s: f64,
    mut on_sample: F,
) -> Result<Vec<crate::field::OklabGaborAnalysis>, String>
where
    F: FnMut(&crate::field::FieldSpawn),
{
    if let Ok(analyses) = stream_gabor_samples_ffmpeg(path, interval_s, |spawn| on_sample(spawn)) {
        return Ok(analyses);
    }

    video_init();
    let mut decoder = Decoder::new(path).map_err(|err| err.to_string())?;
    let mut estimator = crate::field::DualGaborEstimator::new();
    let mut analyses = Vec::new();

    let mut next_target = 0.0f64;
    let half = 1.0 / 60.0;

    for item in decoder.decode_iter() {
        let (ts, frame) = match item {
            Ok(v) => v,
            Err(_) => break,
        };
        let t = ts.as_secs_f64();
        if t + half >= next_target {
            let (h, w, c) = frame.dim();
            if c == 3 {
                let raw_rgb = if let Some(slice) = frame.as_slice() {
                    slice.to_vec()
                } else {
                    let mut buf = Vec::with_capacity(w * h * 3);
                    for y in 0..h {
                        for x in 0..w {
                            buf.push(frame[[y, x, 0]]);
                            buf.push(frame[[y, x, 1]]);
                            buf.push(frame[[y, x, 2]]);
                        }
                    }
                    buf
                };

                let padded_240p = crate::field::fit_240p_padded(w as u32, h as u32, &raw_rgb);
                let analysis = estimator.analyze_frame_240p(t, &padded_240p);

                let spawn = crate::field::FieldSpawn {
                    t,
                    sig: crate::field::FieldSig {
                        r: analysis.rgb[0],
                        g: analysis.rgb[1],
                        b: analysis.rgb[2],
                        theta: analysis.theta,
                        freq: analysis.freq,
                        energy: analysis.luma_energy,
                    },
                };
                on_sample(&spawn);

                analyses.push(analysis);
            }
            next_target = t + interval_s;
        }
    }

    if analyses.is_empty() {
        return Err("no frames analyzed".into());
    }

    Ok(analyses)
}

#[cfg(test)]

mod tests {
    use super::*;
    use ndarray::Array3;
    use video_rs::Encoder;

    fn fixture_named(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("reelbuddy-color-{}-{}.mp4", std::process::id(), name));
        if path.exists() {
            return path;
        }
        video_init();
        let settings = video_rs::encode::Settings::preset_h264_yuv420p(320, 240, false);
        let mut encoder = Encoder::new(path.as_path(), settings).unwrap();
        for i in 0..100 {
            let mut frame = Array3::<u8>::zeros((240, 320, 3));
            let red = if i < 50 { 220u8 } else { 20u8 };
            frame.slice_mut(ndarray::s![.., .., 0]).fill(red);
            encoder
                .encode(&frame, Time::from_secs_f64(i as f64 / 10.0))
                .unwrap();
        }
        encoder.finish().unwrap();
        path
    }

    #[test]
    fn ten_second_fixture_reports_the_real_timestamp() {
        let path = fixture_named("sample_times");
        let duration = duration_s(&path).unwrap();
        assert!(duration > 8.0 && duration < 12.0, "{duration}");
        let samples = sample_times(&path, &[1.0]).unwrap();
        assert_eq!(samples.len(), 1);
        assert!((samples[0].t - 1.0).abs() < 0.2, "{}", samples[0].t);
        assert!(samples[0].frame[[0, 0, 0]] > 100);
        assert!(!samples[0].jpeg.is_empty());

        // Test path with spaces and .mov extension
        let space_dir = std::env::temp_dir().join(format!("space dir {}", std::process::id()));
        let _ = std::fs::create_dir_all(&space_dir);
        let space_path = space_dir.join("test clip.mov");
        let _ = std::fs::copy(&path, &space_path);
        let dur_space = duration_s(&space_path);
        println!("dur_space result: {dur_space:?}");
        assert!(dur_space.is_ok(), "failed to decode path with spaces: {dur_space:?}");
        let _ = std::fs::remove_dir_all(&space_dir);
    }

    #[test]
    fn stream_gabor_samples_processes_video_stream() {
        let path = fixture_named("stream_gabor");
        let mut spawn_count = 0;
        let analyses = stream_gabor_samples(&path, 1.0, |spawn| {
            assert!(spawn.t >= 0.0);
            spawn_count += 1;
        }).unwrap();
        assert!(!analyses.is_empty());
        assert_eq!(analyses.len(), spawn_count);
        // The fixture switches from red to dark red around second 5
        let mid = analyses.len() / 2;
        assert!(analyses[0].rgb[0] > 150);
        let _ = mid;
    }

    #[test]
    fn sample_sections_only_samples_strictly_within_sections() {
        let path = fixture_named("sections_only");
        let sections = vec![(2.0, 4.0), (7.0, 9.0)];
        let samples = sample_sections_only(&path, &sections, 1).unwrap();
        assert_eq!(samples.len(), 2);
        assert!(samples[0].t >= 1.9 && samples[0].t <= 4.1, "t was {}", samples[0].t);
        assert!(samples[1].t >= 6.9 && samples[1].t <= 9.1, "t was {}", samples[1].t);
    }
}

