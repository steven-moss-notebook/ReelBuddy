use std::path::Path;
use std::process::Command;

/// Mux source audio onto a silent picture using the ffmpeg binary that links libav.
/// Failure leaves the silent file in place.
pub fn mux_source_audio(
    silent_mp4: &Path,
    source: &Path,
    start_s: f64,
    end_s: f64,
    dest: &Path,
) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            &silent_mp4.to_string_lossy(),
            "-ss",
            &format!("{start_s:.3}"),
            "-to",
            &format!("{end_s:.3}"),
            "-i",
            &source.to_string_lossy(),
            "-map",
            "0:v:0",
            "-map",
            "1:a:0?",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-shortest",
            &dest.to_string_lossy(),
        ])
        .status()
        .map_err(|err| err.to_string())?;
    if status.success() && dest.exists() {
        Ok(())
    } else {
        Err("audio mux failed".into())
    }
}

pub fn mux_source_audio_segments(
    silent_mp4: &Path,
    source: &Path,
    segments: &[(f64, f64)],
    dest: &Path,
) -> Result<(), String> {
    if segments.is_empty() {
        return Err("no segments provided".into());
    }
    if segments.len() == 1 {
        let (start_s, end_s) = segments[0];
        return mux_source_audio(silent_mp4, source, start_s, end_s, dest);
    }

    let mut filter_complex = String::new();
    let mut concat_inputs = String::new();
    for (i, &(start_s, end_s)) in segments.iter().enumerate() {
        filter_complex.push_str(&format!(
            "[1:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS[a{}];",
            start_s, end_s, i
        ));
        concat_inputs.push_str(&format!("[a{}]", i));
    }
    filter_complex.push_str(&format!(
        "{concat_inputs}concat=n={}:v=0:a=1[outa]",
        segments.len()
    ));

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            &silent_mp4.to_string_lossy(),
            "-i",
            &source.to_string_lossy(),
            "-filter_complex",
            &filter_complex,
            "-map",
            "0:v:0",
            "-map",
            "[outa]",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-shortest",
            &dest.to_string_lossy(),
        ])
        .status()
        .map_err(|err| err.to_string())?;

    if status.success() && dest.exists() {
        Ok(())
    } else {
        Err("audio mux failed".into())
    }
}
