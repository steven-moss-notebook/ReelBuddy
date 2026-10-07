use std::num::NonZeroU32;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::{params::LlamaModelParams, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::mtmd::{mtmd_default_marker, MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputText};
use llama_cpp_2::sampling::LlamaSampler;

use crate::editor::drive::Talk;
use crate::editor::model::Config;
use crate::pet_state::{EDIT_TEMPERATURE, LINE_TEMPERATURE, SAMPLE_TOP_K, SAMPLE_TOP_P};

pub struct Brain {
    ctx: Option<llama_cpp_2::context::LlamaContext<'static>>,
    mtp: Option<llama_cpp_2::speculative::MtpSpeculative<'static>>,
    draft_model: Option<LlamaModel>,
    mtmd: Option<MtmdContext>,
    mtmd_params: MtmdContextParams,
    mmproj: String,
    image_budget: i32,
    chat_template: Option<LlamaChatTemplate>,
    model: LlamaModel,
    _backend: LlamaBackend,
}

impl Drop for Brain {
    fn drop(&mut self) {
        self.mtp.take();
        self.ctx.take();
        self.mtmd.take();
        self.draft_model.take();
    }
}

impl Brain {
    pub fn load(config: &Config) -> Result<Self, String> {
        let backend = LlamaBackend::init().map_err(|err| err.to_string())?;
        let params = LlamaModelParams::default()
            .with_n_gpu_layers(999)
            .with_load_mtp(config.enable_mtp);
        let model = LlamaModel::load_from_file(&backend, &config.model_path, &params)
            .map_err(|err| err.to_string())?;
        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(32768));
        let ctx = model
            .new_context(&backend, ctx_params)
            .map_err(|err| err.to_string())?;
        let ctx: llama_cpp_2::context::LlamaContext<'static> = unsafe { std::mem::transmute(ctx) };

        let mut spec_opt = None;
        let mut draft_model_opt = None;
        let mut main_ctx = Some(ctx);

        if config.enable_mtp && config.mtp_path.is_file() {
            let dft_params = LlamaModelParams::default().with_n_gpu_layers(999);
            match LlamaModel::load_from_file(&backend, &config.mtp_path, &dft_params) {
                Ok(dft_model) => {
                    let dft_ctx_params = LlamaContextParams::default()
                        .with_n_ctx(NonZeroU32::new(32768))
                        .with_context_type(llama_cpp_2::context::params::LlamaContextType::Mtp);
                    let dft_ctx_res = dft_model
                        .new_context(&backend, dft_ctx_params)
                        .map(|c| unsafe {
                            std::mem::transmute::<
                                llama_cpp_2::context::LlamaContext<'_>,
                                llama_cpp_2::context::LlamaContext<'static>,
                            >(c)
                        });
                    match dft_ctx_res {
                        Ok(dft_ctx) => {
                            if let Some(c) = main_ctx.take() {
                                match llama_cpp_2::speculative::MtpSpeculative::new(
                                    c,
                                    dft_ctx,
                                    llama_cpp_2::speculative::MtpSpeculativeParams {
                                        n_max: 3,
                                        n_min: 0,
                                        p_min: 0.0,
                                    },
                                ) {
                                    Ok(spec) => {
                                        spec_opt = Some(spec);
                                        draft_model_opt = Some(dft_model);
                                    }
                                    Err(err) => {
                                        eprintln!("reelbuddy: mtp speculative init: {err:?}");
                                        let c_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(32768));
                                        if let Ok(c_new) = model.new_context(&backend, c_params) {
                                            main_ctx = Some(unsafe { std::mem::transmute(c_new) });
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("reelbuddy: failed to create mtp draft context: {err}");
                        }
                    }
                }
                Err(err) => {
                    eprintln!("reelbuddy: failed to load mtp draft model: {err}");
                }
            }
        }

        let mmproj = config
            .mmproj_path
            .to_str()
            .ok_or_else(|| "mmproj path is not utf-8".to_string())?;
        let mut mtmd_params = MtmdContextParams::default();
        mtmd_params.use_gpu = true;
        mtmd_params.n_threads = 4;
        mtmd_params.print_timings = false;
        mtmd_params.image_min_tokens = 70;
        mtmd_params.image_max_tokens = 280;
        let mtmd = MtmdContext::init_from_file(mmproj, &model, &mtmd_params)
            .map_err(|err| err.to_string())?;
        let chat_template = model.chat_template(None).ok();
        Ok(Self {
            ctx: main_ctx,
            mtp: spec_opt,
            draft_model: draft_model_opt,
            mtmd: Some(mtmd),
            mtmd_params,
            mmproj: mmproj.to_string(),
            image_budget: 280,
            chat_template,
            model,
            _backend: backend,
        })
    }

    fn context_mut_from<'a>(
        mtp: &'a mut Option<llama_cpp_2::speculative::MtpSpeculative<'static>>,
        ctx: &'a mut Option<llama_cpp_2::context::LlamaContext<'static>>,
    ) -> Result<&'a mut llama_cpp_2::context::LlamaContext<'static>, String> {
        if let Some(spec) = mtp.as_mut() {
            Ok(spec.target_context_mut())
        } else if let Some(c) = ctx.as_mut() {
            Ok(c)
        } else {
            Err("model context missing".to_string())
        }
    }

    fn context_mut(&mut self) -> Result<&mut llama_cpp_2::context::LlamaContext<'static>, String> {
        Self::context_mut_from(&mut self.mtp, &mut self.ctx)
    }

    fn ensure_image_budget(&mut self, images: usize) -> Result<(), String> {
        let budget = image_token_budget(images);
        if self.image_budget == budget && self.mtmd.is_some() {
            return Ok(());
        }
        self.mtmd = None;
        let mut params = self.mtmd_params.clone();
        params.image_max_tokens = budget;
        params.image_min_tokens = 70.min(budget);
        let mtmd = MtmdContext::init_from_file(&self.mmproj, &self.model, &params)
            .map_err(|err| err.to_string())?;
        self.mtmd = Some(mtmd);
        self.image_budget = budget;
        Ok(())
    }

    fn render_prompt(&self, prompt: &str) -> String {
        let Ok(message) = LlamaChatMessage::new("user".into(), prompt.to_string()) else {
            return gemma_fallback(prompt);
        };
        if let Some(tmpl) = &self.chat_template
            && let Ok(text) = self.model.apply_chat_template(tmpl, &[message], true)
        {
            return text;
        }
        gemma_fallback(prompt)
    }

    fn presets() -> (LlamaSampler, LlamaSampler) {
        (
            sampler(EDIT_TEMPERATURE),
            sampler(LINE_TEMPERATURE),
        )
    }
}

fn sampler(temp: f32) -> LlamaSampler {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0x524b);
    LlamaSampler::chain(
        [
            LlamaSampler::top_k(SAMPLE_TOP_K),
            LlamaSampler::top_p(SAMPLE_TOP_P, 1),
            LlamaSampler::temp(temp),
            LlamaSampler::dist(seed),
        ],
        false,
    )
}

/// Gemma 4 image budgets. Fewer tokens per frame when a scout attaches many JPEGs,
/// so the answer still fits beside them.
pub fn image_token_budget(images: usize) -> i32 {
    if images <= 6 {
        280
    } else if images <= 12 {
        140
    } else {
        70
    }
}

fn gemma_fallback(prompt: &str) -> String {
    format!("<start_of_turn>user\n{prompt}<end_of_turn>\n<start_of_turn>model\n")
}

impl Talk for Brain {
    fn generate(&mut self, prompt: &str, jpegs: &[Vec<u8>]) -> Result<String, String> {
        self.ensure_image_budget(jpegs.len())?;
        let marker = mtmd_default_marker();
        let mut user = prompt.to_string();
        let mut bitmaps = Vec::new();
        {
            let mtmd = self.mtmd.as_ref().ok_or_else(|| "mmproj missing".to_string())?;
            for jpeg in jpegs {
                user.push('\n');
                user.push_str(marker);
                let bitmap = MtmdBitmap::from_buffer(mtmd, jpeg, false).map_err(|err| err.to_string())?;
                bitmaps.push(bitmap);
            }
        }
        let text = self.render_prompt(&user);
        let mtmd = self.mtmd.as_ref().ok_or_else(|| "mmproj missing".to_string())?;
        let refs: Vec<&MtmdBitmap> = bitmaps.iter().collect();
        let chunks = mtmd
            .tokenize(
                MtmdInputText {
                    text,
                    add_special: true,
                    parse_special: true,
                },
                &refs,
            )
            .map_err(|err| err.to_string())?;
        let ctx = Self::context_mut_from(&mut self.mtp, &mut self.ctx)?;
        ctx.clear_kv_cache();
        let mut n_past = chunks
            .eval_chunks(mtmd, ctx, 0, 0, 512, true)
            .map_err(|err| err.to_string())?;
        let (mut edit, _line) = Self::presets();
        let mut out = String::new();
        let mut batch = LlamaBatch::new(4, 1);
        let mut last_token = None;
        if let Some(spec) = self.mtp.as_mut() {
            let _ = spec.begin(&[]);
        }

        for _ in 0..2048 {
            let mut accepted_tokens = Vec::new();

            if let Some(spec) = self.mtp.as_mut()
                && let Some(prev) = last_token
            {
                if let Ok(drafts) = spec.draft(n_past, prev, &[]) {
                    if !drafts.is_empty() {
                        let mut draft_batch = LlamaBatch::new(drafts.len(), 1);
                        for (idx, draft_tok) in drafts.iter().enumerate() {
                            let _ = draft_batch.add(*draft_tok, n_past + idx as i32, &[0], true);
                        }
                        if spec.target_context_mut().decode(&mut draft_batch).is_ok() {
                            let mut n_acc: u16 = 0;
                            for (idx, draft_tok) in drafts.iter().enumerate() {
                                let sampled = edit.sample(
                                    spec.target_context(),
                                    draft_batch.n_tokens() - (drafts.len() - idx) as i32,
                                );
                                if sampled == *draft_tok {
                                    accepted_tokens.push(*draft_tok);
                                    n_acc += 1;
                                    if self.model.vocab().is_eog(*draft_tok) {
                                        break;
                                    }
                                } else {
                                    accepted_tokens.push(sampled);
                                    break;
                                }
                            }
                            let _ = spec.accept(n_acc);
                            let rollback_start = n_past + accepted_tokens.len() as i32;
                            let _ = spec
                                .target_context_mut()
                                .clear_kv_cache_seq(Some(0), Some(rollback_start as u32), None);
                            let mut accepted_batch =
                                LlamaBatch::new(accepted_tokens.len(), 1);
                            for (idx, acc_tok) in accepted_tokens.iter().enumerate() {
                                let _ = accepted_batch.add(*acc_tok, n_past + idx as i32, &[0], true);
                            }
                            let _ = spec.process(&accepted_batch);
                        } else {
                            let _ = spec.accept(0);
                        }
                    }
                }
            }

            if accepted_tokens.is_empty() {
                let ctx = Self::context_mut_from(&mut self.mtp, &mut self.ctx)?;
                let token = edit.sample(ctx, -1);
                accepted_tokens.push(token);
                batch.clear();
                batch
                    .add(token, n_past, &[0], true)
                    .map_err(|err| err.to_string())?;
                ctx.decode(&mut batch).map_err(|err| err.to_string())?;
                if let Some(spec) = self.mtp.as_mut() {
                    let _ = spec.process(&batch);
                }
            }

            let mut stop = false;
            for token in accepted_tokens {
                last_token = Some(token);
                n_past += 1;
                if self.model.vocab().is_eog(token) {
                    stop = true;
                    break;
                }
                let bytes = self.model.vocab().token_to_piece(token, false, None);
                out.push_str(&String::from_utf8_lossy(&bytes));
                if out.contains("<end_of_turn>") || out.contains("<eos>") || out.contains("<|im_end|>") {
                    stop = true;
                    break;
                }
                if (out.trim_end().ends_with('}') || out.contains("\"action\":\"done\""))
                    && crate::editor::actions::parse_action(&out).is_ok()
                {
                    stop = true;
                    break;
                }
            }
            if stop {
                break;
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet_state;

    #[test]
    fn many_frames_use_a_smaller_image_budget() {
        assert_eq!(image_token_budget(2), 280);
        assert_eq!(image_token_budget(10), 140);
        assert_eq!(image_token_budget(20), 70);
        assert!(image_token_budget(20) < image_token_budget(2));
    }

    #[test]
    fn fallback_opens_the_model_turn() {
        let text = gemma_fallback("Reply with one JSON object.");
        assert!(text.contains("<start_of_turn>model"));
        assert!(text.contains("Reply with one JSON object."));
    }

    #[test]
    fn sampler_presets_match_the_spec() {
        assert!((EDIT_TEMPERATURE - 1.0).abs() < f32::EPSILON);
        assert!((LINE_TEMPERATURE - 1.0).abs() < f32::EPSILON);
        assert_eq!(SAMPLE_TOP_K, 64);
        assert!((SAMPLE_TOP_P - 0.95).abs() < f32::EPSILON);
        let _ = pet_state::LINE_TEMPERATURE;
    }

    #[test]
    #[ignore]
    fn load_weights_when_present() {
        let config = crate::editor::model::load_or_create(&crate::editor::model::app_data_dir()).unwrap();
        if !config.model_path.is_file() || !config.mmproj_path.is_file() {
            return;
        }
        Brain::load(&config).unwrap();
    }
}
