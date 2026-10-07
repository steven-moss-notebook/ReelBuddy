use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use image::ImageEncoder;
use ndarray::Array3;

use crate::editor::{BackgroundStyle, Framing, Motion, NormCrop};

pub const REEL_WIDTH: u32 = 1080;
pub const REEL_HEIGHT: u32 = 1920;

pub fn compute_crop_rect_9_16(
    src_w: u32,
    src_h: u32,
    focus_x: f64,
    focus_y: f64,
    zoom: f64,
) -> (u32, u32, u32, u32) {
    let target_ar = 9.0 / 16.0;
    let src_ar = src_w as f64 / src_h.max(1) as f64;
    let zoom = zoom.clamp(0.1, 1.0);

    let (cw, ch) = if src_ar >= target_ar {
        // Wider than 9:16 (landscape, square, 4:3)
        let ch = (src_h as f64 * zoom).round() as u32;
        let ch = ch.clamp(2, src_h) & !1;
        let cw = ((ch as f64 * target_ar).round() as u32).clamp(2, src_w) & !1;
        (cw, ch)
    } else {
        // Taller than 9:16 (ultra-tall phone screen)
        let cw = (src_w as f64 * zoom).round() as u32;
        let cw = cw.clamp(2, src_w) & !1;
        let ch = ((cw as f64 / target_ar).round() as u32).clamp(2, src_h) & !1;
        (cw, ch)
    };

    let avail_x = src_w.saturating_sub(cw);
    let avail_y = src_h.saturating_sub(ch);
    let x = ((avail_x as f64 * focus_x.clamp(0.0, 1.0)).round() as u32).min(avail_x) & !1;
    let y = ((avail_y as f64 * focus_y.clamp(0.0, 1.0)).round() as u32).min(avail_y) & !1;

    (x, y, cw.max(2), ch.max(2))
}

pub fn crop_rect(frame_w: u32, frame_h: u32, crop: &NormCrop) -> (u32, u32, u32, u32) {
    let x = crop.x.clamp(0.0, 1.0);
    let y = crop.y.clamp(0.0, 1.0);
    let mut w = crop.w.clamp(0.0, 1.0);
    let mut h = crop.h.clamp(0.0, 1.0);
    if x + w > 1.0 {
        w = 1.0 - x;
    }
    if y + h > 1.0 {
        h = 1.0 - y;
    }
    let mut px = (x * frame_w as f64).floor() as u32;
    let mut py = (y * frame_h as f64).floor() as u32;
    let mut pw = (w * frame_w as f64).round() as u32;
    let mut ph = (h * frame_h as f64).round() as u32;
    px -= px % 2;
    py -= py % 2;
    pw = pw.max(2);
    ph = ph.max(2);
    pw -= pw % 2;
    ph -= ph % 2;
    if px >= frame_w {
        px = frame_w.saturating_sub(2);
    }
    if py >= frame_h {
        py = frame_h.saturating_sub(2);
    }
    if px + pw > frame_w {
        pw = frame_w - px;
        pw -= pw % 2;
    }
    if py + ph > frame_h {
        ph = frame_h - py;
        ph -= ph % 2;
    }
    (px, py, pw.max(2), ph.max(2))
}

/// Slide the model's crop. `u` is 0 at the window start and 1 at the end.
pub fn motion_crop(base: &NormCrop, motion: Motion, u: f64) -> NormCrop {
    let u = u.clamp(0.0, 1.0);
    let mut crop = match motion {
        Motion::Hold => base.clone(),
        Motion::PushIn => zoom(base, 1.0 - 0.18 * u),
        Motion::PullOut => zoom(base, 1.0 - 0.18 * (1.0 - u)),
        Motion::PanLeft => shift(base, -0.16 * u, 0.0),
        Motion::PanRight => shift(base, 0.16 * u, 0.0),
    };
    crop.w = crop.w.clamp(0.05, 1.0);
    crop.h = crop.h.clamp(0.05, 1.0);
    crop.x = crop.x.clamp(0.0, 1.0 - crop.w);
    crop.y = crop.y.clamp(0.0, 1.0 - crop.h);
    crop
}

fn zoom(crop: &NormCrop, scale: f64) -> NormCrop {
    let scale = scale.clamp(0.55, 1.0);
    let w = (crop.w * scale).clamp(0.05, 1.0);
    let h = (crop.h * scale).clamp(0.05, 1.0);
    let cx = crop.x + crop.w * 0.5;
    let cy = crop.y + crop.h * 0.5;
    NormCrop {
        x: cx - w * 0.5,
        y: cy - h * 0.5,
        w,
        h,
    }
}

fn shift(crop: &NormCrop, dx: f64, dy: f64) -> NormCrop {
    NormCrop {
        x: crop.x + dx,
        y: crop.y + dy,
        w: crop.w,
        h: crop.h,
    }
}

pub fn render_crop_9_16(
    frame: &Array3<u8>,
    focus_x: f64,
    focus_y: f64,
    zoom: f64,
) -> Result<Array3<u8>, String> {
    let (h, w, _) = frame.dim();
    let (x, y, cw, ch) = compute_crop_rect_9_16(w as u32, h as u32, focus_x, focus_y, zoom);
    let mut cropped = Array3::<u8>::zeros((ch as usize, cw as usize, 3));
    for row in 0..ch as usize {
        for col in 0..cw as usize {
            for channel in 0..3 {
                cropped[[row, col, channel]] = frame[[y as usize + row, x as usize + col, channel]];
            }
        }
    }
    let rgb = array_to_rgb(&cropped)?;
    let scaled = image::imageops::resize(
        &rgb,
        REEL_WIDTH,
        REEL_HEIGHT,
        image::imageops::FilterType::Triangle,
    );
    Ok(rgb_to_array(&scaled))
}

pub fn render_crop_and_move_9_16(
    frame: &Array3<u8>,
    start_x: f64,
    end_x: f64,
    focus_y: f64,
    zoom: f64,
    u: f64,
) -> Result<Array3<u8>, String> {
    let u_clamped = u.clamp(0.0, 1.0);
    let t_ease = u_clamped * u_clamped * (3.0 - 2.0 * u_clamped);
    let current_x = start_x + (end_x - start_x) * t_ease;
    render_crop_9_16(frame, current_x, focus_y, zoom)
}

pub fn render_fit_9_16(
    frame: &Array3<u8>,
    background: BackgroundStyle,
) -> Result<Array3<u8>, String> {
    let (h, w, _) = frame.dim();
    let src_w = w as u32;
    let src_h = h as u32;
    let src_ar = src_w as f64 / src_h.max(1) as f64;
    let target_ar = REEL_WIDTH as f64 / REEL_HEIGHT as f64;

    let (fit_w, fit_h) = if src_ar >= target_ar {
        // Wider than 9:16 - fit width to 1080
        let fit_w = REEL_WIDTH;
        let fit_h = ((REEL_WIDTH as f64 / src_ar).round() as u32).min(REEL_HEIGHT);
        (fit_w, fit_h)
    } else {
        // Taller than 9:16 - fit height to 1920
        let fit_h = REEL_HEIGHT;
        let fit_w = ((REEL_HEIGHT as f64 * src_ar).round() as u32).min(REEL_WIDTH);
        (fit_w, fit_h)
    };

    let offset_x = (REEL_WIDTH.saturating_sub(fit_w)) / 2;
    let offset_y = (REEL_HEIGHT.saturating_sub(fit_h)) / 2;

    let rgb = array_to_rgb(frame)?;
    let fg_scaled = image::imageops::resize(
        &rgb,
        fit_w,
        fit_h,
        image::imageops::FilterType::Triangle,
    );

    let canvas = match background {
        BackgroundStyle::Black => {
            let mut canvas = image::RgbImage::new(REEL_WIDTH, REEL_HEIGHT);
            image::imageops::overlay(&mut canvas, &fg_scaled, offset_x as i64, offset_y as i64);
            canvas
        }
        BackgroundStyle::Blur => {
            // Fast bokeh: downsample to 135x240, blur with sigma 8, upscale to 1080x1920
            let bg_tiny = image::imageops::resize(&rgb, 135, 240, image::imageops::FilterType::Triangle);
            let bg_blurred = image::imageops::blur(&bg_tiny, 8.0);
            let mut bg_canvas = image::imageops::resize(&bg_blurred, REEL_WIDTH, REEL_HEIGHT, image::imageops::FilterType::Triangle);
            for pixel in bg_canvas.pixels_mut() {
                pixel[0] = (pixel[0] as f32 * 0.55) as u8;
                pixel[1] = (pixel[1] as f32 * 0.55) as u8;
                pixel[2] = (pixel[2] as f32 * 0.55) as u8;
            }
            image::imageops::overlay(&mut bg_canvas, &fg_scaled, offset_x as i64, offset_y as i64);
            bg_canvas
        }
    };

    Ok(rgb_to_array(&canvas))
}

pub fn render_vertical_with_framing(
    frame: &Array3<u8>,
    framing: &Framing,
    u: f64,
    cue: Option<&crate::editor::Cue>,
    hook: Option<&str>,
    t: f64,
) -> Result<Array3<u8>, String> {
    let mut out = match framing {
        Framing::Fit { background } => render_fit_9_16(frame, *background)?,
        Framing::Crop { focus_x, focus_y, zoom } => render_crop_9_16(frame, *focus_x, *focus_y, *zoom)?,
        Framing::CropAndMove { start_x, end_x, focus_y, zoom } => {
            render_crop_and_move_9_16(frame, *start_x, *end_x, *focus_y, *zoom, u)?
        }
    };
    if let Some(hook) = hook {
        burn_hook(&mut out, hook);
    }
    if let Some(cue) = cue {
        burn_styled_caption(&mut out, cue, t);
    }
    Ok(out)
}

pub fn render_vertical(
    frame: &Array3<u8>,
    crop: &NormCrop,
    caption: Option<&str>,
    hook: Option<&str>,
) -> Result<Array3<u8>, String> {
    let cue = caption.map(|text| crate::editor::Cue::new(0.0, text));
    render_vertical_with_cue(frame, crop, cue.as_ref(), hook, 0.0)
}

pub fn render_vertical_with_cue(
    frame: &Array3<u8>,
    crop: &NormCrop,
    cue: Option<&crate::editor::Cue>,
    hook: Option<&str>,
    t: f64,
) -> Result<Array3<u8>, String> {
    let framing = if crop.w >= 0.95 && crop.h >= 0.95 {
        Framing::Fit {
            background: BackgroundStyle::Blur,
        }
    } else {
        Framing::Crop {
            focus_x: (crop.x + crop.w * 0.5).clamp(0.0, 1.0),
            focus_y: (crop.y + crop.h * 0.5).clamp(0.0, 1.0),
            zoom: crop.h.clamp(0.1, 1.0),
        }
    };
    render_vertical_with_framing(frame, &framing, 0.0, cue, hook, t)
}

pub fn burn_caption(frame: &mut Array3<u8>, text: &str) {
    let cue = crate::editor::Cue::new(0.0, text);
    burn_styled_caption(frame, &cue, 0.0);
}

pub fn burn_styled_caption(frame: &mut Array3<u8>, cue: &crate::editor::Cue, t: f64) {
    let plain = cue.plain_text();
    let four = four_words(&plain);
    if four.is_empty() {
        return;
    }
    let Ok(font) = FontRef::try_from_slice(bevy::text::DEFAULT_FONT_DATA) else {
        return;
    };
    let (_, _, h) = dims(frame);
    let px = (h as f32 * 0.045).clamp(18.0, 72.0);
    let baseline = h as i32 - (h as i32 / 8);
    let dt = (t - cue.t).max(0.0);

    let spans = cue.styled_spans();
    let scale = PxScale::from(px);
    let mut total_width: f32 = 0.0;
    let mut total_chars: usize = 0;
    for span in &spans {
        for ch in span.text.chars() {
            total_width += font.as_scaled(scale).h_advance(font.glyph_id(ch));
            total_chars += 1;
        }
    }

    let (_, w, _) = (frame.dim().0, frame.dim().1, frame.dim().2);
    let mut cursor = (w as f32 - total_width) / 2.0;
    let mut global_char_idx = 0;

    for span in &spans {
        let effect_name = span.effect.as_deref().unwrap_or("");
        let gradient_name = span.gradient.as_deref().unwrap_or("rainbow");

        for ch in span.text.chars() {
            let (dx, dy, color, scale_mod) = match effect_name {
                "shake" => {
                    let phase = (dt * 50.0).floor() as u64 + global_char_idx as u64;
                    let jx = ((phase * 17 + 5) % 9) as f32 - 4.0;
                    let jy = ((phase * 31 + 7) % 7) as f32 - 3.0;
                    (jx, jy, [255, 59, 48], 1.0f32)
                }
                "bounce" => {
                    let bounce_val = (dt * 14.0).sin().abs() as f32;
                    let jy = -bounce_val * (px * 0.35);
                    (0.0, jy, [52, 199, 89], 1.0f32)
                }
                "wave" => {
                    let wave_phase = (dt * 4.0) + (global_char_idx as f64 * 0.7);
                    let jy = (wave_phase.sin() * (px as f64 * 0.22)) as f32;
                    (0.0, jy, [255, 204, 0], 1.0f32)
                }
                "pulse" => {
                    let pulse_factor = 1.0 + (dt * 10.0).sin().abs() as f32 * 0.22;
                    (0.0, 0.0, [255, 45, 85], pulse_factor)
                }
                "color_shift" => {
                    let hue = ((dt * 1.5) as f32 + global_char_idx as f32 * 0.1).fract().abs();
                    (0.0, 0.0, hsv_to_rgb(hue, 0.9, 1.0), 1.0f32)
                }
                "gradient" => {
                    let u = ((global_char_idx as f32 / total_chars.max(1) as f32) + (dt as f32 * 0.4)).fract().abs();
                    (0.0, 0.0, sample_gradient(gradient_name, u), 1.0f32)
                }
                _ => (0.0, 0.0, [255, 255, 255], 1.0f32),
            };

            let char_scale = PxScale::from(px * scale_mod);
            paint_glyph(frame, &font, ch, char_scale, cursor + dx, baseline as f32 + dy, [0, 0, 0], 2);
            paint_glyph(frame, &font, ch, char_scale, cursor + dx, baseline as f32 + dy, color, 0);
            cursor += font.as_scaled(scale).h_advance(font.glyph_id(ch));
            global_char_idx += 1;
        }
    }
}

pub fn burn_hook(frame: &mut Array3<u8>, text: &str) {
    let text: String = text.chars().take(42).collect();
    if text.is_empty() {
        return;
    }
    let (_, _, h) = dims(frame);
    draw_centered(frame, &text, (h as i32 / 10).max(24), (h as f32 * 0.04).clamp(16.0, 64.0));
}

pub fn active_cue<'a>(cues: &'a [crate::editor::Cue], t: f64) -> Option<&'a crate::editor::Cue> {
    cues.iter()
        .filter(|cue| cue.t <= t + 0.05)
        .max_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal))
}

pub fn active_caption<'a>(cues: &'a [crate::editor::Cue], t: f64) -> Option<&'a str> {
    active_cue(cues, t).map(|cue| cue.text.as_str())
}

fn sample_gradient(palette: &str, u: f32) -> [u8; 3] {
    let u = u.fract().abs();
    if palette.contains('#') {
        let custom_stops: Vec<[u8; 3]> = palette
            .split(|c| c == ',' || c == '[' || c == ']' || c == ' ' || c == '-')
            .filter_map(parse_hex_color)
            .collect();
        if custom_stops.len() >= 2 {
            return interpolate_stops(&custom_stops, u);
        } else if let Some(only) = custom_stops.first() {
            return *only;
        }
    }

    let stops: &[[u8; 3]] = match palette.to_lowercase().as_str() {
        "fire" | "sunset" => &[[255, 30, 39], [255, 122, 0], [255, 214, 0], [255, 30, 39]],
        "ocean" | "ice" => &[[0, 56, 255], [0, 210, 255], [112, 255, 243], [0, 56, 255]],
        "cyberpunk" | "neon" => &[[255, 0, 122], [122, 0, 255], [0, 240, 255], [255, 0, 122]],
        "gold" | "luxury" => &[[200, 138, 30], [255, 215, 0], [255, 243, 168], [200, 138, 30]],
        "emerald" | "toxic" => &[[15, 125, 66], [16, 229, 67], [164, 255, 0], [15, 125, 66]],
        "galaxy" | "cosmic" => &[[11, 14, 42], [108, 28, 209], [224, 57, 253], [11, 14, 42]],
        "candy" | "pastel" => &[[255, 133, 179], [199, 133, 255], [133, 227, 255], [255, 133, 179]],
        "monochrome" | "silver" => &[[34, 34, 34], [160, 160, 160], [255, 255, 255], [34, 34, 34]],
        "lava" | "magma" => &[[26, 11, 11], [230, 25, 25], [255, 107, 8], [26, 11, 11]],
        _ => &[
            [255, 0, 0],
            [255, 127, 0],
            [255, 255, 0],
            [0, 255, 0],
            [0, 127, 255],
            [139, 0, 255],
            [255, 0, 0],
        ],
    };
    interpolate_stops(stops, u)
}

fn interpolate_stops(stops: &[[u8; 3]], u: f32) -> [u8; 3] {
    if stops.is_empty() {
        return [255, 255, 255];
    }
    if stops.len() == 1 {
        return stops[0];
    }
    let scaled = u * (stops.len() - 1) as f32;
    let idx = (scaled.floor() as usize).min(stops.len() - 2);
    let frac = scaled - idx as f32;
    let c0 = stops[idx];
    let c1 = stops[idx + 1];
    [
        (c0[0] as f32 * (1.0 - frac) + c1[0] as f32 * frac).round() as u8,
        (c0[1] as f32 * (1.0 - frac) + c1[1] as f32 * frac).round() as u8,
        (c0[2] as f32 * (1.0 - frac) + c1[2] as f32 * frac).round() as u8,
    ]
}

fn parse_hex_color(hex: &str) -> Option<[u8; 3]> {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
        let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
        let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
        Some([r, g, b])
    } else if clean.len() == 3 {
        let r = u8::from_str_radix(&clean[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&clean[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&clean[2..3], 16).ok()? * 17;
        Some([r, g, b])
    } else {
        None
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8]
}

pub fn jpeg_long_edge(frame: &Array3<u8>, edge: u32) -> Result<Vec<u8>, String> {
    let rgb = array_to_rgb(frame)?;
    let (w, h) = rgb.dimensions();
    let long = w.max(h);
    let (nw, nh) = if long <= edge {
        (w, h)
    } else if w >= h {
        (edge, ((h as u64 * edge as u64) / w as u64).max(1) as u32)
    } else {
        (((w as u64 * edge as u64) / h as u64).max(1) as u32, edge)
    };
    let scaled = image::imageops::resize(&rgb, nw, nh, image::imageops::FilterType::Triangle);
    let mut bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80);
    encoder
        .write_image(
            scaled.as_raw(),
            nw,
            nh,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|err| err.to_string())?;
    Ok(bytes)
}

fn four_words(text: &str) -> String {
    text.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
}

fn dims(frame: &Array3<u8>) -> (usize, usize, usize) {
    let (h, w, c) = frame.dim();
    (w, c, h)
}

fn array_to_rgb(frame: &Array3<u8>) -> Result<image::RgbImage, String> {
    let (h, w, c) = frame.dim();
    if c != 3 {
        return Err("expected rgb".into());
    }
    let mut bytes = Vec::with_capacity(w * h * 3);
    for row in 0..h {
        for col in 0..w {
            bytes.push(frame[[row, col, 0]]);
            bytes.push(frame[[row, col, 1]]);
            bytes.push(frame[[row, col, 2]]);
        }
    }
    image::RgbImage::from_raw(w as u32, h as u32, bytes).ok_or_else(|| "rgb buffer".into())
}

fn rgb_to_array(image: &image::RgbImage) -> Array3<u8> {
    let (w, h) = image.dimensions();
    let mut frame = Array3::<u8>::zeros((h as usize, w as usize, 3));
    for (x, y, pixel) in image.enumerate_pixels() {
        frame[[y as usize, x as usize, 0]] = pixel[0];
        frame[[y as usize, x as usize, 1]] = pixel[1];
        frame[[y as usize, x as usize, 2]] = pixel[2];
    }
    frame
}

fn draw_centered(frame: &mut Array3<u8>, text: &str, baseline: i32, px: f32) {
    let Ok(font) = FontRef::try_from_slice(bevy::text::DEFAULT_FONT_DATA) else {
        return;
    };
    let scale = PxScale::from(px);
    let width: f32 = text
        .chars()
        .map(|ch| font.as_scaled(scale).h_advance(font.glyph_id(ch)))
        .sum();
    let (_, w, _) = (frame.dim().0, frame.dim().1, frame.dim().2);
    let mut cursor = (w as f32 - width) / 2.0;
    for ch in text.chars() {
        paint_glyph(frame, &font, ch, scale, cursor, baseline as f32, [0, 0, 0], 1);
        paint_glyph(frame, &font, ch, scale, cursor, baseline as f32, [255, 255, 255], 0);
        cursor += font.as_scaled(scale).h_advance(font.glyph_id(ch));
    }
}

fn paint_glyph(
    frame: &mut Array3<u8>,
    font: &FontRef<'_>,
    ch: char,
    scale: PxScale,
    x: f32,
    baseline: f32,
    color: [u8; 3],
    spread: i32,
) {
    let (height, width, _) = frame.dim();
    for dy in -spread..=spread {
        for dx in -spread..=spread {
            if spread > 0 && dx == 0 && dy == 0 {
                continue;
            }
            if spread == 0 && (dx != 0 || dy != 0) {
                continue;
            }
            let glyph = font
                .glyph_id(ch)
                .with_scale_and_position(scale, point(x + dx as f32, baseline + dy as f32));
            let Some(outlined) = font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                if coverage < 0.4 {
                    return;
                }
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px < 0 || py < 0 || px as usize >= width || py as usize >= height {
                    return;
                }
                frame[[py as usize, px as usize, 0]] = color[0];
                frame[[py as usize, px as usize, 1]] = color[1];
                frame[[py as usize, px as usize, 2]] = color[2];
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_rect_stays_inside_and_even() {
        let crop = NormCrop {
            x: -0.2,
            y: 0.1,
            w: 0.9,
            h: 0.95,
        };
        let (x, y, w, h) = crop_rect(101, 80, &crop);
        assert_eq!(x % 2, 0);
        assert_eq!(y % 2, 0);
        assert_eq!(w % 2, 0);
        assert_eq!(h % 2, 0);
        assert!(x + w <= 101);
        assert!(y + h <= 80);
        assert!(w >= 2 && h >= 2);
    }

    #[test]
    fn caption_band_is_not_the_background() {
        let mut frame = Array3::<u8>::from_elem((80, 160, 3), 12u8);
        burn_caption(&mut frame, "this is the part extra");
        let band = frame.slice(ndarray::s![60.., .., ..]);
        let changed = band.iter().any(|px| *px != 12);
        assert!(changed, "caption band stayed background");
    }

    #[test]
    fn motion_crop_stays_inside_the_frame() {
        let edge = NormCrop {
            x: 0.72,
            y: 0.0,
            w: 0.28,
            h: 0.9,
        };
        for motion in [Motion::PanRight, Motion::PanLeft, Motion::PushIn, Motion::PullOut, Motion::Hold] {
            for step in 0..5 {
                let moved = motion_crop(&edge, motion, step as f64 / 4.0);
                assert!(moved.x >= -1e-9 && moved.y >= -1e-9, "{moved:?}");
                assert!(moved.x + moved.w <= 1.0 + 1e-6, "{moved:?}");
                assert!(moved.y + moved.h <= 1.0 + 1e-6, "{moved:?}");
            }
        }
    }

    #[test]
    fn styled_caption_burns_effects_and_gradients() {
        let mut frame = Array3::<u8>::from_elem((80, 160, 3), 0u8);
        let cue = crate::editor::Cue::new(0.0, "fire move").with_effect("gradient").with_gradient("fire");
        burn_styled_caption(&mut frame, &cue, 0.5);
        let band = frame.slice(ndarray::s![50.., .., ..]);
        let non_zero = band.iter().any(|px| *px > 0);
        assert!(non_zero, "gradient caption should draw colored pixels");
    }

    #[test]
    fn inline_tagged_caption_burns() {
        let mut frame = Array3::<u8>::from_elem((80, 160, 3), 0u8);
        let cue = crate::editor::Cue::new(0.0, "[shake]pause[/shake] now");
        burn_styled_caption(&mut frame, &cue, 0.2);
        let non_zero = frame.iter().any(|px| *px > 0);
        assert!(non_zero, "inline tagged caption should draw pixels");
    }

    #[test]
    fn compute_crop_rect_9_16_preserves_aspect_ratio() {
        // Landscape 1920x1080 (16:9)
        let (x, y, cw, ch) = compute_crop_rect_9_16(1920, 1080, 0.5, 0.5, 1.0);
        assert_eq!(ch, 1080);
        assert_eq!(cw, 608);
        let ar = cw as f64 / ch as f64;
        assert!((ar - 9.0 / 16.0).abs() < 0.01);
        assert_eq!(x, 656);
        assert_eq!(y, 0);

        // Portrait 1080x1920 (already 9:16)
        let (px, py, pcw, pch) = compute_crop_rect_9_16(1080, 1920, 0.5, 0.5, 1.0);
        assert_eq!(pcw, 1080);
        assert_eq!(pch, 1920);
        assert_eq!(px, 0);
        assert_eq!(py, 0);

        // Square 1000x1000
        let (_sx, _sy, scw, sch) = compute_crop_rect_9_16(1000, 1000, 0.5, 0.5, 1.0);
        assert_eq!(sch, 1000);
        assert_eq!(scw, 562);
        let sar = scw as f64 / sch as f64;
        assert!((sar - 9.0 / 16.0).abs() < 0.01);
    }

    #[test]
    fn render_fit_9_16_outputs_reel_resolution_without_distortion() {
        // 320x240 landscape frame
        let mut frame = Array3::<u8>::zeros((240, 320, 3));
        frame.slice_mut(ndarray::s![.., .., 0]).fill(200);

        let out_blur = render_fit_9_16(&frame, BackgroundStyle::Blur).unwrap();
        assert_eq!(out_blur.dim(), (1920, 1080, 3));

        let out_black = render_fit_9_16(&frame, BackgroundStyle::Black).unwrap();
        assert_eq!(out_black.dim(), (1920, 1080, 3));
        // Top edge should be black bars
        assert_eq!(out_black[[10, 10, 0]], 0);
        assert_eq!(out_black[[10, 10, 1]], 0);
        assert_eq!(out_black[[10, 10, 2]], 0);
        // Center should contain content
        assert!(out_black[[960, 540, 0]] > 100);
    }

    #[test]
    fn render_crop_and_move_9_16_pans_smoothly() {
        let frame = Array3::<u8>::zeros((240, 320, 3));
        let out_start = render_crop_and_move_9_16(&frame, 0.1, 0.9, 0.5, 1.0, 0.0).unwrap();
        assert_eq!(out_start.dim(), (1920, 1080, 3));

        let out_mid = render_crop_and_move_9_16(&frame, 0.1, 0.9, 0.5, 1.0, 0.5).unwrap();
        assert_eq!(out_mid.dim(), (1920, 1080, 3));

        let out_end = render_crop_and_move_9_16(&frame, 0.1, 0.9, 0.5, 1.0, 1.0).unwrap();
        assert_eq!(out_end.dim(), (1920, 1080, 3));
    }
}
