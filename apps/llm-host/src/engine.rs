//! The model: loading it and writing an answer, token by token.

use std::{num::NonZeroU32, path::Path, time::Instant};

use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, LlamaChatMessage, LlamaModel},
    sampling::LlamaSampler,
};
use offstage_llm_proto::Message;

use crate::utf8::Utf8Stream;

/// Tokens the model can see at once: the prompt and the answer together. The assistant's prompt is
/// about 1,500 tokens, and the bench ran at this size (`docs/LLM-BENCH.md`).
pub const N_CTX: u32 = 4096;
/// Tokens read per step while taking in the prompt.
const N_BATCH: usize = 512;

pub struct Engine {
    backend: LlamaBackend,
    model: LlamaModel,
    /// Qwen3 writes a long "thinking" section first unless it is told there is none, and that
    /// would ruin the time to the first token.
    close_thinking: bool,
    pub load_ms: u64,
}

impl Engine {
    pub fn load(path: &Path, close_thinking: bool) -> Result<Self, String> {
        let started = Instant::now();
        let mut backend = LlamaBackend::init().map_err(|e| e.to_string())?;
        // llama.cpp logs a lot, and nobody reads it here.
        backend.void_logs();
        let model = LlamaModel::load_from_file(
            &backend,
            path,
            &LlamaModelParams::default().with_n_gpu_layers(999),
        )
        .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self {
            backend,
            model,
            close_thinking,
            load_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }

    /// The prompt text for a conversation, in the model's own chat format.
    fn prompt(&self, messages: &[Message]) -> Result<String, String> {
        let template = self.model.chat_template(None).map_err(|e| e.to_string())?;
        let chat = messages
            .iter()
            .map(|m| LlamaChatMessage::new(m.role.clone(), m.content.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut prompt = self
            .model
            .apply_chat_template(&template, &chat, true)
            .map_err(|e| e.to_string())?;
        if self.close_thinking {
            prompt.push_str("<think>\n\n</think>\n\n");
        }
        Ok(prompt)
    }

    /// Writes an answer, passing each piece of text to `on_text` as soon as it is complete. Stops
    /// at the end of the answer, after `max_tokens`, or when `stop()` says so. Greedy decoding, so
    /// the same question gives the same answer.
    pub fn answer(
        &self,
        messages: &[Message],
        max_tokens: u32,
        stop: &dyn Fn() -> bool,
        on_text: &mut dyn FnMut(&str),
    ) -> Result<(), String> {
        let prompt = self.prompt(messages)?;
        let vocab = self.model.vocab();
        let tokens = vocab.tokenize(prompt.as_bytes(), false, true);
        if tokens.is_empty() {
            return Err("empty prompt".to_owned());
        }
        if tokens.len() + max_tokens as usize > N_CTX as usize {
            return Err(format!(
                "the prompt of {} tokens does not fit with {max_tokens} tokens of answer",
                tokens.len()
            ));
        }

        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_batch(N_BATCH as u32)
            .with_n_ubatch(N_BATCH as u32);
        let mut ctx = self
            .model
            .new_context(&self.backend, params)
            .map_err(|e| e.to_string())?;

        let mut batch = LlamaBatch::new(N_BATCH, 1);
        let mut position = 0;
        for (chunk_index, chunk) in tokens.chunks(N_BATCH).enumerate() {
            if stop() {
                return Ok(());
            }
            batch.clear();
            for (i, token) in chunk.iter().enumerate() {
                let last = chunk_index * N_BATCH + i == tokens.len() - 1;
                batch
                    .add(*token, position, &[0], last)
                    .map_err(|e| e.to_string())?;
                position += 1;
            }
            ctx.decode(&mut batch).map_err(|e| e.to_string())?;
        }

        let mut sampler = LlamaSampler::greedy();
        let mut text = Utf8Stream::default();
        let mut token = sampler.sample(&ctx, batch.n_tokens() - 1);
        let mut written = 0;
        while written < max_tokens && !vocab.is_eog(token) && !stop() {
            let piece = text.push(&vocab.token_to_piece(token, false, None));
            if !piece.is_empty() {
                on_text(&piece);
            }
            written += 1;
            batch.clear();
            batch
                .add(token, position, &[0], true)
                .map_err(|e| e.to_string())?;
            position += 1;
            ctx.decode(&mut batch).map_err(|e| e.to_string())?;
            token = sampler.sample(&ctx, batch.n_tokens() - 1);
        }
        let rest = text.finish();
        if !rest.is_empty() {
            on_text(&rest);
        }
        Ok(())
    }
}
