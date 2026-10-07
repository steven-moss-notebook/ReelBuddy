//! Gabor phase-field signatures. The editor thread computes these. Bevy only draws them.

const KERNEL: usize = 7;
const SPRITE: usize = 32;
/// Mean absolute response below this is a flat frame: all flat, no core.
const FLAT_ENERGY: f32 = 1.5;

#[derive(Clone, Copy, Debug)]
pub struct FieldSig {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub theta: f32,
    pub freq: f32,
    pub energy: f32,
}

impl FieldSig {
    pub fn flat() -> Self {
        Self {
            r: 40,
            g: 40,
            b: 40,
            theta: 0.0,
            freq: 0.2,
            energy: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recipe {
    pub color: [u8; 3],
    pub theta: f32,
    pub freq: f32,
    pub seed: u64,
    pub lumps: u8,
    pub strokes: u8,
    pub is_flat: bool,
}

#[derive(Clone, Debug)]
pub struct FieldSpawn {
    pub t: f64,
    pub sig: FieldSig,
}

pub fn signature_jpeg(bytes: &[u8]) -> FieldSig {
    let Ok(image) = image::load_from_memory(bytes) else {
        return FieldSig::flat();
    };
    let rgb = image.to_rgb8();
    let (w, h) = rgb.dimensions();
    let small = if w > 32 || h > 32 {
        image::imageops::resize(&rgb, 32, 32, image::imageops::FilterType::Triangle)
    } else {
        rgb
    };
    signature_from_rgb(small.width(), small.height(), small.as_raw())
}

pub fn signature_from_rgb(w: u32, h: u32, rgb: &[u8]) -> FieldSig {
    let count = (w as usize).saturating_mul(h as usize);
    if w < KERNEL as u32 || h < KERNEL as u32 || rgb.len() < count * 3 {
        return FieldSig::flat();
    }
    let mut red = 0u64;
    let mut green = 0u64;
    let mut blue = 0u64;
    for px in rgb.chunks_exact(3).take(count) {
        red += px[0] as u64;
        green += px[1] as u64;
        blue += px[2] as u64;
    }
    let n = count as u64;
    let angles = [0.0f32, 45.0, 90.0, 135.0];
    let freqs = [0.18f32, 0.28];
    let mut best_score = 0.0f32;
    let mut best_wave = 0.0f32;
    let mut best_freq = freqs[0];
    let samples = ((w as usize - 6) * (h as usize - 6)).max(1) as f32;
    for deg in angles {
        for freq in freqs {
            let kernel = gabor(deg.to_radians(), freq);
            let score = convolve(w, rgb, &kernel) / samples;
            if score > best_score {
                best_score = score;
                best_wave = deg;
                best_freq = freq;
            }
        }
    }
    let stripe = (best_wave + 90.0) % 180.0;
    FieldSig {
        r: (red / n) as u8,
        g: (green / n) as u8,
        b: (blue / n) as u8,
        theta: stripe.to_radians(),
        freq: best_freq,
        energy: best_score,
    }
}

pub fn silhouette_seed(sig: &FieldSig) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in [sig.r, sig.g, sig.b] {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= sig.theta.to_bits() as u64;
    hash = hash.wrapping_mul(0x100000001b3);
    hash ^= sig.freq.to_bits() as u64;
    hash = hash.wrapping_mul(0x100000001b3);
    hash ^= sig.energy.to_bits() as u64;
    hash.wrapping_mul(0x100000001b3)
}

pub fn recipe_from_signature(sig: &FieldSig) -> Recipe {
    let seed = silhouette_seed(sig);
    Recipe {
        color: [sig.r, sig.g, sig.b],
        theta: sig.theta,
        freq: sig.freq,
        seed,
        lumps: 2 + (seed % 2) as u8,
        strokes: 2 + ((seed >> 3) % 2) as u8,
        is_flat: sig.energy < FLAT_ENERGY,
    }
}

pub fn fidget_signature(n: u64) -> FieldSig {
    FieldSig {
        r: n.wrapping_mul(37).wrapping_add(24) as u8,
        g: n.wrapping_mul(91).wrapping_add(48) as u8,
        b: n.wrapping_mul(53).wrapping_add(36) as u8,
        theta: ((n % 4) as f32) * std::f32::consts::FRAC_PI_2,
        freq: if n % 2 == 0 { 0.18 } else { 0.28 },
        energy: if n % 5 == 0 { 0.2 } else { 8.0 },
    }
}

pub fn sprite_rgba(recipe: &Recipe) -> Vec<u8> {
    let mut out = vec![0u8; SPRITE * SPRITE * 4];
    let (sin, cos) = recipe.theta.sin_cos();
    for y in 0..SPRITE {
        for x in 0..SPRITE {
            let nx = (x as f32 - 15.5) / 15.5;
            let ny = (y as f32 - 15.5) / 15.5;
            let field = phase(nx, ny, recipe.seed, recipe.lumps);
            if field < 0.05 {
                continue;
            }
            let along = nx * cos + ny * sin;
            let grating = (recipe.freq * 14.0 * along * std::f32::consts::TAU).cos();
            let flat = recipe.is_flat || field < 0.55;
            let base = if flat {
                [
                    recipe.color[0] / 3 + 28,
                    recipe.color[1] / 3 + 22,
                    recipe.color[2] / 3 + 16,
                ]
            } else {
                recipe.color
            };
            let shade = if grating > 0.0 { 1.0 } else { 0.7 };
            let i = (y * SPRITE + x) * 4;
            out[i] = (base[0] as f32 * shade) as u8;
            out[i + 1] = (base[1] as f32 * shade) as u8;
            out[i + 2] = (base[2] as f32 * shade) as u8;
            out[i + 3] = (field.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    out
}

fn phase(nx: f32, ny: f32, seed: u64, lumps: u8) -> f32 {
    let mut field = 0.42 - nx * nx - ny * ny;
    for i in 0..lumps {
        let ox = (((seed >> (8 * i)) & 255) as f32 / 255.0 - 0.5) * 0.55;
        let oy = (((seed >> (8 * i + 4)) & 255) as f32 / 255.0 - 0.5) * 0.55;
        let radius = 0.28;
        field += radius * radius - (nx - ox) * (nx - ox) - (ny - oy) * (ny - oy);
    }
    field.tanh().max(0.0)
}

fn gabor(wave: f32, freq: f32) -> [[f32; KERNEL]; KERNEL] {
    let sigma = 1.35;
    let (sin, cos) = wave.sin_cos();
    let mut kernel = [[0.0f32; KERNEL]; KERNEL];
    let mut sum = 0.0f32;
    for y in 0..KERNEL {
        for x in 0..KERNEL {
            let dx = x as f32 - 3.0;
            let dy = y as f32 - 3.0;
            let xr = dx * cos + dy * sin;
            let yr = -dx * sin + dy * cos;
            let env = (-(xr * xr + yr * yr) / (2.0 * sigma * sigma)).exp();
            let value = env * (std::f32::consts::TAU * freq * xr).cos();
            kernel[y][x] = value;
            sum += value;
        }
    }
    let mean = sum / (KERNEL * KERNEL) as f32;
    for row in &mut kernel {
        for value in row.iter_mut() {
            *value -= mean;
        }
    }
    kernel
}

fn convolve(w: u32, rgb: &[u8], kernel: &[[f32; KERNEL]; KERNEL]) -> f32 {
    let width = w as usize;
    let height = rgb.len() / (width * 3);
    let mut acc = 0.0f32;
    for y in 0..(height - 6) {
        for x in 0..(width - 6) {
            let mut sum = 0.0f32;
            for ky in 0..KERNEL {
                for kx in 0..KERNEL {
                    let px = ((y + ky) * width + (x + kx)) * 3;
                    let luma = rgb[px] as f32 * 0.3 + rgb[px + 1] as f32 * 0.59 + rgb[px + 2] as f32 * 0.11;
                    sum += luma * kernel[ky][kx];
                }
            }
            acc += sum.abs();
        }
    }
    acc
}

pub const FRAME_DIM: usize = 240;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

impl Oklab {
    pub fn new(l: f32, a: f32, b: f32) -> Self {
        Self { l, a, b }
    }

    pub fn chroma(&self) -> f32 {
        (self.a * self.a + self.b * self.b).sqrt()
    }

    pub fn hue(&self) -> f32 {
        self.b.atan2(self.a)
    }
}

#[inline]
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[inline]
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

pub fn srgb_to_oklab(r: u8, g: u8, b: u8) -> Oklab {
    let lr = srgb_to_linear(r as f32 / 255.0);
    let lg = srgb_to_linear(g as f32 / 255.0);
    let lb = srgb_to_linear(b as f32 / 255.0);
    let l_cone = (0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb).cbrt();
    let m_cone = (0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb).cbrt();
    let s_cone = (0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb).cbrt();
    Oklab {
        l: 0.2104542553 * l_cone + 0.7936177850 * m_cone - 0.0040720468 * s_cone,
        a: 1.9779984951 * l_cone - 2.4285922050 * m_cone + 0.4505937099 * s_cone,
        b: 0.0259040371 * l_cone + 0.7827717662 * m_cone - 0.8086757660 * s_cone,
    }
}

pub fn oklab_to_srgb(lab: Oklab) -> [u8; 3] {
    let l_cone = lab.l + 0.3963377774 * lab.a + 0.2158037573 * lab.b;
    let m_cone = lab.l - 0.1055613458 * lab.a - 0.0638541728 * lab.b;
    let s_cone = lab.l - 0.0894841775 * lab.a - 1.2914855480 * lab.b;
    let l3 = l_cone * l_cone * l_cone;
    let m3 = m_cone * m_cone * m_cone;
    let s3 = s_cone * s_cone * s_cone;
    let lr = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
    let lg = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
    let lb = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3;
    [
        (linear_to_srgb(lr) * 255.0).clamp(0.0, 255.0).round() as u8,
        (linear_to_srgb(lg) * 255.0).clamp(0.0, 255.0).round() as u8,
        (linear_to_srgb(lb) * 255.0).clamp(0.0, 255.0).round() as u8,
    ]
}

/// Fits the largest dimension of an RGB frame to 240px, preserving exact aspect ratio,
/// and pads the shorter dimension symmetrically to produce a uniform 240x240 RGB buffer.
pub fn fit_240p_padded(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    if w == 0 || h == 0 || rgb.len() < (w as usize * h as usize * 3) {
        return vec![0u8; FRAME_DIM * FRAME_DIM * 3];
    }
    if w == FRAME_DIM as u32 && h == FRAME_DIM as u32 && rgb.len() >= FRAME_DIM * FRAME_DIM * 3 {
        return rgb[..FRAME_DIM * FRAME_DIM * 3].to_vec();
    }
    let scale = (FRAME_DIM as f32) / (w.max(h) as f32);
    let nw = ((w as f32 * scale).round() as u32).clamp(1, FRAME_DIM as u32);
    let nh = ((h as f32 * scale).round() as u32).clamp(1, FRAME_DIM as u32);

    let img = match image::RgbImage::from_raw(w, h, rgb.to_vec()) {
        Some(img) => img,
        None => return vec![0u8; FRAME_DIM * FRAME_DIM * 3],
    };
    let resized = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Nearest);

    let pad_x = (FRAME_DIM as u32 - nw) / 2;
    let pad_y = (FRAME_DIM as u32 - nh) / 2;

    let mut out = vec![0u8; FRAME_DIM * FRAME_DIM * 3];
    for y in 0..nh {
        for x in 0..nw {
            let px = resized.get_pixel(x, y);
            let out_idx = (((y + pad_y) as usize * FRAME_DIM) + (x + pad_x) as usize) * 3;
            out[out_idx] = px[0];
            out[out_idx + 1] = px[1];
            out[out_idx + 2] = px[2];
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct OklabGaborAnalysis {
    pub t: f64,
    pub luma_energy: f32,
    pub color_energy: f32,
    pub mean_chroma: f32,
    pub luma_motion: f32,
    pub color_motion: f32,
    pub total_motion: f32,
    pub interest_score: f32,
    pub oklab: Oklab,
    pub rgb: [u8; 3],
    pub theta: f32,
    pub freq: f32,
}

fn convolve_channel_grid(
    buf: &[f32],
    kernel: &[[f32; KERNEL]; KERNEL],
    sample_w: usize,
    sample_h: usize,
    stride: usize,
) -> f32 {
    let mut acc = 0.0f32;
    for sy in 0..sample_h {
        let y = sy * stride;
        for sx in 0..sample_w {
            let x = sx * stride;
            let mut sum = 0.0f32;
            for ky in 0..KERNEL {
                for kx in 0..KERNEL {
                    let idx = (y + ky) * FRAME_DIM + (x + kx);
                    sum += buf[idx] * kernel[ky][kx];
                }
            }
            acc += sum.abs();
        }
    }
    acc
}

pub struct DualGaborEstimator {
    luma_kernels: Vec<([[f32; KERNEL]; KERNEL], f32, f32)>, // kernel, wave_deg, freq
    color_kernels: Vec<([[f32; KERNEL]; KERNEL], f32, f32)>,
    prev_luma_response: Option<Vec<f32>>,
    prev_color_response: Option<Vec<f32>>,
}

impl Default for DualGaborEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl DualGaborEstimator {
    pub fn new() -> Self {
        let angles = [0.0f32, 45.0, 90.0, 135.0];
        let luma_freqs = [0.08f32, 0.16, 0.28];
        let color_freqs = [0.08f32, 0.16];

        let mut luma_kernels = Vec::new();
        for &deg in &angles {
            for &freq in &luma_freqs {
                luma_kernels.push((gabor(deg.to_radians(), freq), deg, freq));
            }
        }

        let mut color_kernels = Vec::new();
        for &deg in &angles {
            for &freq in &color_freqs {
                color_kernels.push((gabor(deg.to_radians(), freq), deg, freq));
            }
        }

        Self {
            luma_kernels,
            color_kernels,
            prev_luma_response: None,
            prev_color_response: None,
        }
    }

    /// Analyzes a 240x240 padded frame in Oklab colorspace using the dual Gabor filter banks.
    pub fn analyze_frame_240p(&mut self, t: f64, padded_rgb: &[u8]) -> OklabGaborAnalysis {
        let total_px = FRAME_DIM * FRAME_DIM;
        if padded_rgb.len() < total_px * 3 {
            return OklabGaborAnalysis {
                t,
                luma_energy: 0.0,
                color_energy: 0.0,
                mean_chroma: 0.0,
                luma_motion: 0.0,
                color_motion: 0.0,
                total_motion: 0.0,
                interest_score: 0.0,
                oklab: Oklab { l: 0.0, a: 0.0, b: 0.0 },
                rgb: [0, 0, 0],
                theta: 0.0,
                freq: 0.16,
            };
        }

        // 1. Convert 240x240 frame to Oklab buffers
        let mut luma_buf = vec![0.0f32; total_px];
        let mut a_buf = vec![0.0f32; total_px];
        let mut b_buf = vec![0.0f32; total_px];
        let mut chroma_sum = 0.0f32;
        let mut l_sum = 0.0f32;
        let mut a_sum = 0.0f32;
        let mut b_sum = 0.0f32;
        let mut r_sum = 0u64;
        let mut g_sum = 0u64;
        let mut bl_sum = 0u64;

        for (i, px) in padded_rgb.chunks_exact(3).take(total_px).enumerate() {
            let r = px[0];
            let g = px[1];
            let b = px[2];
            r_sum += r as u64;
            g_sum += g as u64;
            bl_sum += b as u64;
            let lab = srgb_to_oklab(r, g, b);
            luma_buf[i] = lab.l;
            a_buf[i] = lab.a;
            b_buf[i] = lab.b;
            let ch = lab.chroma();
            chroma_sum += ch;
            l_sum += lab.l;
            a_sum += lab.a;
            b_sum += lab.b;
        }

        let n = total_px as f32;
        let mean_chroma = chroma_sum / n;
        let mean_l = l_sum / n;
        let mean_a = a_sum / n;
        let mean_b = b_sum / n;
        let mean_oklab = Oklab { l: mean_l, a: mean_a, b: mean_b };
        let mean_rgb = [
            (r_sum / total_px as u64) as u8,
            (g_sum / total_px as u64) as u8,
            (bl_sum / total_px as u64) as u8,
        ];

        // 2. Bank 1: Convolve L channel (sampled with stride 4 for fast computation across 240p)
        let stride = 4usize;
        let sample_w = (FRAME_DIM - KERNEL) / stride + 1;
        let sample_h = (FRAME_DIM - KERNEL) / stride + 1;
        let num_samples = (sample_w * sample_h).max(1) as f32;

        let mut current_luma_responses = Vec::with_capacity(self.luma_kernels.len());
        let mut best_luma_score = 0.0f32;
        let mut best_luma_deg = 0.0f32;
        let mut best_luma_freq = 0.16f32;

        for (kernel, deg, freq) in &self.luma_kernels {
            let score = convolve_channel_grid(&luma_buf, kernel, sample_w, sample_h, stride) / num_samples;
            current_luma_responses.push(score);
            if score > best_luma_score {
                best_luma_score = score;
                best_luma_deg = *deg;
                best_luma_freq = *freq;
            }
        }

        // 3. Bank 2: Convolve Chromatic channels (a & b)
        let mut current_color_responses = Vec::with_capacity(self.color_kernels.len());
        let mut best_color_score = 0.0f32;

        for (kernel, _deg, _freq) in &self.color_kernels {
            let acc_a = convolve_channel_grid(&a_buf, kernel, sample_w, sample_h, stride);
            let acc_b = convolve_channel_grid(&b_buf, kernel, sample_w, sample_h, stride);
            let score = (acc_a + acc_b) / num_samples;
            current_color_responses.push(score);
            if score > best_color_score {
                best_color_score = score;
            }
        }

        // 4. Frequency Domain Motion Energy across consecutive frames
        let luma_motion = match &self.prev_luma_response {
            Some(prev) => {
                let diff_sum: f32 = current_luma_responses
                    .iter()
                    .zip(prev.iter())
                    .map(|(c, p)| (c - p).abs())
                    .sum();
                diff_sum / current_luma_responses.len() as f32
            }
            None => 0.0,
        };

        let color_motion = match &self.prev_color_response {
            Some(prev) => {
                let diff_sum: f32 = current_color_responses
                    .iter()
                    .zip(prev.iter())
                    .map(|(c, p)| (c - p).abs())
                    .sum();
                diff_sum / current_color_responses.len() as f32
            }
            None => 0.0,
        };

        self.prev_luma_response = Some(current_luma_responses);
        self.prev_color_response = Some(current_color_responses);

        let total_motion = luma_motion * 12.0 + color_motion * 18.0;
        let luma_energy = best_luma_score * 10.0;
        let color_energy = best_color_score * 15.0;

        // Interest score balances motion, texture richness, and color vibrancy
        let interest_score = (total_motion * 35.0 + luma_energy * 20.0 + color_energy * 25.0 + mean_chroma * 150.0)
            .clamp(0.0, 100.0);

        let stripe = (best_luma_deg + 90.0) % 180.0;

        OklabGaborAnalysis {
            t,
            luma_energy,
            color_energy,
            mean_chroma,
            luma_motion,
            color_motion,
            total_motion,
            interest_score,
            oklab: mean_oklab,
            rgb: mean_rgb,
            theta: stripe.to_radians(),
            freq: best_luma_freq,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CandidateWindow {
    pub start_s: f64,
    pub end_s: f64,
    pub duration_s: f64,
    pub avg_interest: f32,
    pub hook_energy: f32,
    pub motion_energy: f32,
    pub color_energy: f32,
    pub score: f32,
}

/// Extracts top non-overlapping candidate short video windows of strictly consecutive frames.
///
/// Guaranteed properties:
/// 1. Every window is an unbroken contiguous range: start_s to end_s with consecutive timestamps.
/// 2. min_dur <= (end_s - start_s) <= max_dur.
/// 3. Does not stitch or concatenate disjoint footage.
fn score_window_span(
    slice: &[OklabGaborAnalysis],
    hook_sec: f64,
) -> (f32, f32, f32, f32, f32) {
    let count = slice.len() as f32;
    let avg_interest: f32 = slice.iter().map(|f| f.interest_score).sum::<f32>() / count;
    let motion_energy: f32 = slice.iter().map(|f| f.total_motion).sum::<f32>() / count;
    let color_energy: f32 = slice.iter().map(|f| f.color_energy).sum::<f32>() / count;

    let start_t = slice[0].t;
    let hook_slice_len = slice
        .iter()
        .take_while(|f| f.t - start_t <= hook_sec)
        .count()
        .max(1);
    let hook_energy: f32 = slice[..hook_slice_len]
        .iter()
        .map(|f| f.interest_score)
        .sum::<f32>()
        / (hook_slice_len as f32);

    let has_dead_air = slice
        .windows(3)
        .any(|w| w.iter().all(|f| f.total_motion < 0.05 && f.luma_energy < 0.1));
    let dead_air_penalty = if has_dead_air { 20.0 } else { 0.0 };

    let window_score = (0.35 * hook_energy
        + 0.35 * avg_interest
        + 0.20 * motion_energy * 20.0
        + 0.10 * color_energy * 10.0
        - dead_air_penalty)
        .clamp(0.0, 100.0);

    (avg_interest, hook_energy, motion_energy, color_energy, window_score)
}

fn collect_candidate_windows(
    frames: &[OklabGaborAnalysis],
    min_dur: f64,
    max_dur: f64,
    hook_sec: f64,
) -> Vec<CandidateWindow> {
    let n = frames.len();
    let mut candidate_spans: Vec<CandidateWindow> = Vec::new();
    let max_gap = 3.5;

    for i in 0..n {
        let mut j = i;
        let mut continuous = true;
        while j < n {
            if j > i && (frames[j].t - frames[j - 1].t) > max_gap {
                continuous = false;
                break;
            }
            let dur = frames[j].t - frames[i].t;
            if dur >= min_dur && dur <= max_dur {
                let slice = &frames[i..=j];
                let (avg_interest, hook_energy, motion_energy, color_energy, score) =
                    score_window_span(slice, hook_sec);

                candidate_spans.push(CandidateWindow {
                    start_s: frames[i].t,
                    end_s: frames[j].t,
                    duration_s: dur,
                    avg_interest,
                    hook_energy,
                    motion_energy,
                    color_energy,
                    score,
                });
            }
            if dur > max_dur {
                break;
            }
            j += 1;
        }
        if !continuous {
            continue;
        }
    }
    candidate_spans
}

fn select_non_overlapping_windows(
    mut candidate_spans: Vec<CandidateWindow>,
    cap: usize,
) -> Vec<CandidateWindow> {
    candidate_spans.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let mut selected: Vec<CandidateWindow> = Vec::new();
    for cand in candidate_spans {
        let overlaps = selected.iter().any(|s| cand.start_s < s.end_s && cand.end_s > s.start_s);
        if !overlaps {
            selected.push(cand);
            if selected.len() >= cap {
                break;
            }
        }
    }

    selected.sort_by(|a, b| a.start_s.partial_cmp(&b.start_s).unwrap_or(std::cmp::Ordering::Equal));
    selected
}

pub fn select_contiguous_windows(
    frames: &[OklabGaborAnalysis],
    min_dur: f64,
    max_dur: f64,
    cap: usize,
) -> Vec<CandidateWindow> {
    if frames.is_empty() || cap == 0 {
        return Vec::new();
    }
    let candidates = collect_candidate_windows(frames, min_dur, max_dur, 3.0);
    select_non_overlapping_windows(candidates, cap)
}

/// Find the best short sections (e.g. 3.0s to 5.0s each) across the video footage,
/// selecting up to target_count distinct, non-overlapping candidate moments.
pub fn select_best_sections(
    frames: &[OklabGaborAnalysis],
    min_dur: f64,
    max_dur: f64,
    target_count: usize,
) -> Vec<CandidateWindow> {
    if frames.is_empty() || target_count == 0 {
        return Vec::new();
    }
    let candidates = collect_candidate_windows(frames, min_dur, max_dur, 1.0);
    select_non_overlapping_windows(candidates, target_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_stripes_are_near_vertical_and_flat_is_flat() {
        let mut stripes = vec![0u8; 32 * 32 * 3];
        for y in 0..32 {
            for x in 0..32 {
                let v = if (x / 2) % 2 == 0 { 255 } else { 0 };
                let i = (y * 32 + x) * 3;
                stripes[i] = v;
                stripes[i + 1] = v;
                stripes[i + 2] = v;
            }
        }
        let sig = signature_from_rgb(32, 32, &stripes);
        let deg = sig.theta.to_degrees();
        assert!(
            (deg - 90.0).abs() < 20.0,
            "stripe angle {deg} energy {}",
            sig.energy
        );
        assert!(sig.energy >= FLAT_ENERGY, "stripe energy {}", sig.energy);

        let flat = vec![40u8; 32 * 32 * 3];
        let blank = signature_from_rgb(32, 32, &flat);
        let recipe = recipe_from_signature(&blank);
        assert!(recipe.is_flat, "flat energy {}", blank.energy);
        assert!(sprite_rgba(&recipe).len() == SPRITE * SPRITE * 4);
    }

    #[test]
    fn the_same_signature_yields_the_same_silhouette_seed() {
        let sig = FieldSig {
            r: 12,
            g: 80,
            b: 200,
            theta: 1.2,
            freq: 0.28,
            energy: 4.0,
        };
        assert_eq!(silhouette_seed(&sig), silhouette_seed(&sig));
        let recipe = recipe_from_signature(&sig);
        assert_eq!(recipe.seed, silhouette_seed(&sig));
        assert_eq!(recipe_from_signature(&sig), recipe);
    }

    #[test]
    fn fit_240p_padded_preserves_aspect_ratio_and_centers() {
        // Landscape 16:9 test frame (320x180)
        let w = 320u32;
        let h = 180u32;
        let mut rgb = vec![128u8; (w * h * 3) as usize];
        // Set red marker at center
        let center_idx = ((h / 2 * w) + (w / 2)) as usize * 3;
        rgb[center_idx] = 255;
        rgb[center_idx + 1] = 0;
        rgb[center_idx + 2] = 0;

        let padded = fit_240p_padded(w, h, &rgb);
        assert_eq!(padded.len(), FRAME_DIM * FRAME_DIM * 3);

        // Top and bottom borders should be zero-padded
        assert_eq!(padded[0], 0);
        assert_eq!(padded[1], 0);
        assert_eq!(padded[2], 0);

        // Center should contain non-zero content
        let pad_center = ((FRAME_DIM / 2 * FRAME_DIM) + (FRAME_DIM / 2)) * 3;
        assert!(padded[pad_center] > 100);

        // Portrait 9:16 test frame (180x320)
        let p_w = 180u32;
        let p_h = 320u32;
        let p_rgb = vec![100u8; (p_w * p_h * 3) as usize];
        let p_padded = fit_240p_padded(p_w, p_h, &p_rgb);
        assert_eq!(p_padded.len(), FRAME_DIM * FRAME_DIM * 3);
        // Left and right edges should have padding
        assert_eq!(p_padded[0], 0);
    }

    #[test]
    fn oklab_roundtrip_conversions_are_accurate() {
        let colors = [
            [255, 0, 0],     // Red
            [0, 255, 0],     // Green
            [0, 0, 255],     // Blue
            [255, 255, 255], // White
            [0, 0, 0],       // Black
            [235, 180, 140], // Beige
        ];
        for orig in colors {
            let lab = srgb_to_oklab(orig[0], orig[1], orig[2]);
            let roundtrip = oklab_to_srgb(lab);
            for c in 0..3 {
                let diff = (orig[c] as i32 - roundtrip[c] as i32).abs();
                assert!(diff <= 2, "Color {:?} failed roundtrip -> {:?}", orig, roundtrip);
            }
        }
    }

    #[test]
    fn dual_gabor_detects_motion_across_consecutive_frames() {
        let mut estimator = DualGaborEstimator::new();
        let frame1 = vec![120u8; FRAME_DIM * FRAME_DIM * 3];
        let analysis1 = estimator.analyze_frame_240p(0.0, &frame1);
        assert_eq!(analysis1.total_motion, 0.0);

        // Identical frame has near zero motion
        let analysis2 = estimator.analyze_frame_240p(1.0, &frame1);
        assert!(analysis2.total_motion < 0.01, "Expected zero motion on static frame, got {}", analysis2.total_motion);

        // Moving pattern with stripes
        let mut moving_frame = vec![0u8; FRAME_DIM * FRAME_DIM * 3];
        for y in 0..FRAME_DIM {
            for x in 0..FRAME_DIM {
                let idx = (y * FRAME_DIM + x) * 3;
                let v = if (x / 8) % 2 == 0 { 240 } else { 20 };
                moving_frame[idx] = v;
                moving_frame[idx + 1] = v / 2;
                moving_frame[idx + 2] = 200;
            }
        }
        let analysis3 = estimator.analyze_frame_240p(2.0, &moving_frame);
        assert!(analysis3.total_motion > 0.05, "Expected high motion energy on pattern change, got {}", analysis3.total_motion);
        assert!(analysis3.interest_score > analysis2.interest_score);
    }

    #[test]
    fn select_contiguous_windows_enforces_consecutive_timestamps() {
        let mut frames = Vec::new();
        for sec in 0..60 {
            frames.push(OklabGaborAnalysis {
                t: sec as f64,
                luma_energy: 1.0,
                color_energy: 1.0,
                mean_chroma: 0.1,
                luma_motion: if (15..35).contains(&sec) { 0.8 } else { 0.1 },
                color_motion: 0.1,
                total_motion: if (15..35).contains(&sec) { 0.9 } else { 0.1 },
                interest_score: if (15..35).contains(&sec) { 85.0 } else { 30.0 },
                oklab: Oklab { l: 0.5, a: 0.0, b: 0.0 },
                rgb: [128, 128, 128],
                theta: 0.0,
                freq: 0.16,
            });
        }

        let windows = select_contiguous_windows(&frames, 15.0, 30.0, 2);
        assert!(!windows.is_empty());
        let top = &windows[0];
        assert!(top.duration_s >= 15.0 && top.duration_s <= 30.0);
        // The top window should encompass the high-motion region (15..35)
        assert!(top.start_s <= 20.0 && top.end_s >= 30.0);

        // Gap test: ensure it doesn't cross a 10-second gap
        let mut gapped_frames = Vec::new();
        for sec in 0..10 {
            gapped_frames.push(frames[sec].clone());
        }
        // Jump to second 100
        for sec in 100..110 {
            let mut f = frames[sec - 100].clone();
            f.t = sec as f64;
            gapped_frames.push(f);
        }
        let gapped_windows = select_contiguous_windows(&gapped_frames, 15.0, 30.0, 2);
        // Should find no windows because each streak is only 9 seconds (< min_dur 15.0)
        assert!(gapped_windows.is_empty(), "Should not stitch disconnected footage across gap");
    }

    #[test]
    fn select_best_sections_extracts_distinct_short_sections() {
        let mut frames = Vec::new();
        for sec in 0..120 {
            let in_burst = (10..15).contains(&sec)
                || (30..35).contains(&sec)
                || (55..60).contains(&sec)
                || (80..85).contains(&sec)
                || (100..105).contains(&sec);
            frames.push(OklabGaborAnalysis {
                t: sec as f64,
                luma_energy: 1.0,
                color_energy: 1.0,
                mean_chroma: 0.1,
                luma_motion: if in_burst { 0.9 } else { 0.1 },
                color_motion: 0.1,
                total_motion: if in_burst { 0.95 } else { 0.1 },
                interest_score: if in_burst { 90.0 } else { 20.0 },
                oklab: Oklab { l: 0.5, a: 0.0, b: 0.0 },
                rgb: [128, 128, 128],
                theta: 0.0,
                freq: 0.16,
            });
        }

        let sections = select_best_sections(&frames, 3.0, 5.0, 5);
        assert_eq!(sections.len(), 5);
        for s in &sections {
            let dur = s.end_s - s.start_s;
            assert!(dur >= 3.0 && dur <= 5.0, "duration was {dur}");
        }
        // Verify non-overlapping
        for i in 0..sections.len() {
            for j in (i + 1)..sections.len() {
                assert!(
                    sections[i].end_s <= sections[j].start_s || sections[j].end_s <= sections[i].start_s,
                    "sections overlap: {:?} and {:?}", sections[i], sections[j]
                );
            }
        }
    }
}

