//! Spike: how a small local model answers a RAG question on this machine.
//!
//! Usage: `offstage-llm-bench <model.gguf> [--think] [--repeat N]`
//!
//! Loads the model, then answers a fixed set of Vietnamese and English questions from a few
//! retrieved passages, the way the assistant would. For each answer it prints the prompt size,
//! the time to the first token, the generation speed and the text, and it reports the memory
//! the process uses. Greedy decoding, so a rerun gives the same answers.
//!
//! Qwen3 starts with a long "thinking" section unless it is told not to, which would ruin the
//! time to first token, so the spike closes that section up front. `--think` leaves it open.
//! `--repeat N` sends the passages N times, to see how the time to first token grows with the
//! amount of retrieved text (the real retriever sends 4 to 6 passages of about 1,600 characters).

use std::{
    env,
    error::Error,
    num::NonZeroU32,
    path::PathBuf,
    process::{self, Command},
    time::Instant,
};

use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, LlamaChatMessage, LlamaModel},
    sampling::LlamaSampler,
};

const N_CTX: u32 = 4096;
/// Tokens handled per step while reading the prompt. `BENCH_BATCH` overrides it to see whether a
/// larger step reads the prompt faster.
fn n_batch() -> usize {
    env::var("BENCH_BATCH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512)
}
const MAX_NEW_TOKENS: usize = 200;

/// What the assistant is told. Mirrors the RAG prompt in CLAUDE.md.
const SYSTEM: &str = "Bạn là trợ lý trong một cuộc gọi trực tiếp. Chỉ trả lời dựa trên phần TÀI LIỆU. \
Viết 3 đến 5 gạch đầu dòng ngắn, mỗi dòng bắt đầu bằng \"- \". Dùng cùng ngôn ngữ với câu hỏi. \
Nếu TÀI LIỆU không có thông tin để trả lời, chỉ viết đúng một dòng: Không có trong tài liệu.";

/// Passages as the retriever would return them: a source label and the text.
const PASSAGES: &[(&str, &str)] = &[
    (
        "CV.md > Kinh nghiệm",
        "Kỹ sư phần mềm tại Công ty Âm Sắc (2022 đến nay). Viết lại bộ xử lý âm thanh thời gian thực \
bằng Rust, giảm độ trễ từ 180 ms xuống 45 ms. Phụ trách pipeline thu âm, giảm nhiễu và nhận dạng \
giọng nói chạy trên máy người dùng. Hai năm dùng Rust cho dịch vụ xử lý âm thanh.",
    ),
    (
        "CV.md > Dự án",
        "Dự án Offstage: ứng dụng overlay chạy trên mọi ứng dụng, hiện phụ đề trực tiếp của cuộc gọi và \
gợi ý câu trả lời. Dùng Tauri, whisper.cpp và SQLite. Làm một mình trong 6 tháng, 1.200 người dùng \
thử nghiệm.",
    ),
    (
        "CV.md > Kỹ năng",
        "Rust, TypeScript, React, C++. Xử lý tín hiệu số, âm thanh thời gian thực, whisper.cpp, \
llama.cpp. Tiếng Anh giao tiếp tốt (IELTS 7.0).",
    ),
    (
        "JD.md > Yêu cầu",
        "Vị trí Senior Audio Engineer. Yêu cầu: 3 năm kinh nghiệm xử lý âm thanh, thành thạo Rust hoặc \
C++, đã triển khai mô hình nhận dạng giọng nói. Làm việc hybrid tại Hà Nội.",
    ),
];

/// Questions and what a faithful answer must or must not say.
struct Case {
    question: &'static str,
    /// The answer should mention one of these. Empty when the documents have no answer.
    expect_any: &'static [&'static str],
    /// The documents do not cover it, so the model must say so instead of inventing something.
    unanswerable: bool,
}

const CASES: &[Case] = &[
    Case {
        question: "Bạn có kinh nghiệm với Rust không?",
        expect_any: &["hai năm", "2 năm", "Rust"],
        unanswerable: false,
    },
    Case {
        question: "Mô tả dự án gần đây nhất của bạn.",
        expect_any: &["Offstage"],
        unanswerable: false,
    },
    Case {
        question: "What is your experience with real-time audio?",
        expect_any: &["45 ms", "180 ms", "Âm Sắc", "Rust"],
        unanswerable: false,
    },
    Case {
        question: "Bạn đã làm việc với Kubernetes bao lâu rồi?",
        expect_any: &[],
        unanswerable: true,
    },
    Case {
        question: "Mức lương mong muốn của bạn là bao nhiêu?",
        expect_any: &[],
        unanswerable: true,
    },
    Case {
        question: "Bạn đã giảm độ trễ xuống bao nhiêu và bằng cách nào?",
        expect_any: &["45 ms", "180 ms"],
        unanswerable: false,
    },
];

fn user_message(question: &str, repeat: usize) -> String {
    let documents: Vec<String> = (1..=repeat)
        .flat_map(|round| {
            PASSAGES
                .iter()
                .map(move |(source, text)| format!("[{source} #{round}]\n{text}"))
        })
        .collect();
    format!("TÀI LIỆU:\n{}\n\nCÂU HỎI: {question}", documents.join("\n\n"))
}

/// Resident memory of this process, in MB. Unified memory on Apple silicon means the model's
/// GPU buffers may not all show up here, so `footprint` is read too when it is available.
fn rss_mb() -> f64 {
    Command::new("ps")
        .args(["-o", "rss=", "-p", &process::id().to_string()])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|text| text.trim().parse::<f64>().ok())
        .map_or(f64::NAN, |kb| kb / 1024.0)
}

fn footprint_line() -> String {
    Command::new("footprint")
        .arg(process::id().to_string())
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|text| {
            text.lines()
                .find(|l| l.contains("Footprint"))
                .map(|l| l.trim().to_owned())
        })
        .unwrap_or_else(|| "footprint unavailable".into())
}

struct Answer {
    prompt_tokens: usize,
    first_token_ms: f64,
    generated: usize,
    tokens_per_second: f64,
    text: String,
}

fn answer(
    backend: &LlamaBackend,
    model: &LlamaModel,
    prompt: &str,
    add_bos: bool,
) -> Result<Answer, Box<dyn Error>> {
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(N_CTX))
        .with_n_batch(n_batch() as u32)
        .with_n_ubatch(n_batch() as u32);
    let mut ctx = model.new_context(backend, params)?;
    let vocab = model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), add_bos, true);
    if tokens.len() + MAX_NEW_TOKENS > N_CTX as usize {
        return Err(format!("prompt of {} tokens does not fit", tokens.len()).into());
    }

    let started = Instant::now();
    let n_batch = n_batch();
    let mut batch = LlamaBatch::new(n_batch, 1);
    let mut position = 0;
    for (chunk_index, chunk) in tokens.chunks(n_batch).enumerate() {
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            let last = chunk_index * n_batch + i == tokens.len() - 1;
            batch.add(*token, position, &[0], last)?;
            position += 1;
        }
        ctx.decode(&mut batch)?;
    }

    let mut sampler = LlamaSampler::greedy();
    let mut token = sampler.sample(&ctx, batch.n_tokens() - 1);
    let first_token_ms = started.elapsed().as_secs_f64() * 1000.0;

    let generating = Instant::now();
    let mut bytes: Vec<u8> = Vec::new();
    let mut generated = 0;
    while generated < MAX_NEW_TOKENS && !vocab.is_eog(token) {
        bytes.extend(vocab.token_to_piece(token, false, None));
        generated += 1;
        batch.clear();
        batch.add(token, position, &[0], true)?;
        position += 1;
        ctx.decode(&mut batch)?;
        token = sampler.sample(&ctx, batch.n_tokens() - 1);
    }
    let seconds = generating.elapsed().as_secs_f64();

    Ok(Answer {
        prompt_tokens: tokens.len(),
        first_token_ms,
        generated,
        tokens_per_second: if seconds > 0.0 {
            generated as f64 / seconds
        } else {
            0.0
        },
        text: String::from_utf8_lossy(&bytes).trim().to_owned(),
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let path: PathBuf = args.next().ok_or("usage: offstage-llm-bench <model.gguf> [--think]")?.into();
    let rest: Vec<String> = args.collect();
    let think = rest.iter().any(|a| a == "--think");
    let repeat = rest
        .iter()
        .position(|a| a == "--repeat")
        .and_then(|i| rest.get(i + 1))
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_qwen3 = name.contains("qwen3");
    // The Gemma chat template does not write the start-of-text token, so the tokenizer adds it.
    let add_bos = name.contains("gemma");
    // llama.cpp's built-in chat templates do not know the Gemma 4 format yet, so the prompt is
    // written by hand from the template stored in the model file (thinking is off by default).
    let gemma4_by_hand = name.contains("gemma-4");

    let mut backend = LlamaBackend::init()?;
    backend.void_logs();

    println!("model: {}", path.display());
    println!("rss before load: {:.0} MB", rss_mb());
    let loading = Instant::now();
    let model = LlamaModel::load_from_file(
        &backend,
        &path,
        &LlamaModelParams::default().with_n_gpu_layers(999),
    )?;
    println!(
        "load: {:.2} s | {:.2} B parameters | {:.2} GB of weights",
        loading.elapsed().as_secs_f64(),
        model.n_params() as f64 / 1e9,
        model.size() as f64 / 1e9
    );
    println!("rss after load: {:.0} MB | {}", rss_mb(), footprint_line());

    let template = model.chat_template(None)?;
    if rest.iter().any(|a| a == "--print-template") {
        println!("{}", template.to_str()?);
        return Ok(());
    }
    let mut peak_rss: f64 = 0.0;
    let mut faithful = 0;

    for (index, case) in CASES.iter().enumerate() {
        let user = user_message(case.question, repeat);
        let mut prompt = if gemma4_by_hand {
            format!("<|turn>system\n{SYSTEM}<turn|>\n<|turn>user\n{user}<turn|>\n<|turn>model\n")
        } else {
            let messages = vec![
                LlamaChatMessage::new("system".into(), SYSTEM.into())?,
                LlamaChatMessage::new("user".into(), user)?,
            ];
            model.apply_chat_template(&template, &messages, true)?
        };
        if is_qwen3 && !think {
            prompt.push_str("<think>\n\n</think>\n\n");
        }

        let result = answer(&backend, &model, &prompt, add_bos)?;
        peak_rss = peak_rss.max(rss_mb());

        let says_unsupported = result.text.contains("Không có trong tài liệu");
        let ok = if case.unanswerable {
            says_unsupported
        } else {
            !says_unsupported
                && case
                    .expect_any
                    .iter()
                    .any(|needle| result.text.to_lowercase().contains(&needle.to_lowercase()))
        };
        faithful += usize::from(ok);

        println!("\n--- {}/{}: {}", index + 1, CASES.len(), case.question);
        println!(
            "prompt {} tokens | first token {:.0} ms | {} tokens at {:.1} tok/s | {}",
            result.prompt_tokens,
            result.first_token_ms,
            result.generated,
            result.tokens_per_second,
            if ok { "OK" } else { "CHECK" }
        );
        println!("{}", result.text);
    }

    println!(
        "\nsummary: {faithful}/{} answers faithful by the keyword check | peak rss {:.0} MB | {}",
        CASES.len(),
        peak_rss,
        footprint_line()
    );
    Ok(())
}
