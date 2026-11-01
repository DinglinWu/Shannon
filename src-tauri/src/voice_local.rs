//! Offline SenseVoice + Silero VAD. Audio stays in bounded memory and is never uploaded.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use sherpa_onnx::{LinearResampler, OfflineRecognizer, OfflineRecognizerConfig, VadModelConfig, VoiceActivityDetector};
use std::{path::Path, sync::{atomic::{AtomicBool, Ordering}, mpsc::{self, SyncSender}, Arc, Mutex, OnceLock}, time::{Duration, Instant}};

static ENGINE: OnceLock<Mutex<Option<OfflineRecognizer>>> = OnceLock::new();
const RATE: i32 = 16_000;
const WINDOW: usize = 512;

fn asset(dir: &Path, name: &str) -> Result<String, String> {
    let path = dir.join(name);
    if !path.is_file() { return Err(format!("缺少语音资源 {name}，请重新安装完整应用")); }
    Ok(path.to_string_lossy().into_owned())
}
fn create_engine(dir: &Path) -> Result<OfflineRecognizer, String> {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.sense_voice.model = Some(asset(dir, "model.int8.onnx")?);
    config.model_config.sense_voice.language = Some("auto".into());
    config.model_config.sense_voice.use_itn = true;
    config.model_config.tokens = Some(asset(dir, "tokens.txt")?);
    config.model_config.num_threads = std::thread::available_parallelism().map(|n| n.get().min(4) as i32).unwrap_or(2);
    config.model_config.provider = Some("cpu".into());
    OfflineRecognizer::create(&config).ok_or_else(|| "SenseVoice 本地模型加载失败，请检查模型文件是否完整".into())
}
fn create_vad(dir: &Path) -> Result<VoiceActivityDetector, String> {
    let mut config = VadModelConfig::default();
    config.silero_vad.model = Some(asset(dir, "silero_vad.onnx")?);
    config.silero_vad.threshold = 0.5;
    config.silero_vad.min_silence_duration = 0.5;
    config.silero_vad.min_speech_duration = 0.25;
    config.silero_vad.max_speech_duration = 10.0;
    config.silero_vad.window_size = WINDOW as i32;
    config.sample_rate = RATE;
    config.num_threads = 1;
    config.provider = Some("cpu".into());
    VoiceActivityDetector::create(&config, 30.0).ok_or_else(|| "语音活动检测模型加载失败".into())
}
fn decode(engine: &OfflineRecognizer, samples: &[f32]) -> Result<String, String> {
    let stream = engine.create_stream();
    stream.accept_waveform(RATE, samples);
    engine.decode(&stream);
    stream.get_result().map(|r| clean_text(&r.text)).ok_or_else(|| "本地语音识别未返回结果".into())
}
// Remove language/emotion/audio-event tags rather than inserting them into a query.
fn clean_text(text: &str) -> String {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<|") {
        output.push_str(&rest[..start]);
        if let Some(end) = rest[start + 2..].find("|>") { rest = &rest[start + 2 + end + 2..]; }
        else { rest = &rest[start..]; break; }
    }
    output.push_str(rest);
    output.trim().to_owned()
}
fn drain(vad: &VoiceActivityDetector, engine: &OfflineRecognizer, emit: &mut impl FnMut(String)) -> Result<(), String> {
    while let Some(segment) = vad.front() {
        let text = decode(engine, segment.samples())?;
        vad.pop();
        if !text.is_empty() { emit(text); }
    }
    Ok(())
}
fn enqueue<T: cpal::Sample + Copy>(data: &[T], channels: usize, tx: &SyncSender<Result<Vec<f32>, String>>, overflow: &AtomicBool)
where f32: cpal::FromSample<T> {
    let mono = data.chunks_exact(channels).map(|frame| {
        frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32
    }).collect();
    if let Err(mpsc::TrySendError::Full(_)) = tx.try_send(Ok(mono)) { overflow.store(true, Ordering::Relaxed); }
}
fn capture(device: &cpal::Device, config: &cpal::SupportedStreamConfig, tx: SyncSender<Result<Vec<f32>, String>>, overflow: Arc<AtomicBool>) -> Result<cpal::Stream, String> {
    let channels = usize::from(config.channels());
    if channels == 0 { return Err("麦克风声道配置无效".into()); }
    let errors = tx.clone();
    let err_fn = move |error| { let _ = errors.try_send(Err(format!("麦克风中断：{error}"))); };
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(&config.config(), move |data: &[f32], _| enqueue(data, channels, &tx, &overflow), err_fn, None),
        cpal::SampleFormat::I16 => device.build_input_stream(&config.config(), move |data: &[i16], _| enqueue(data, channels, &tx, &overflow), err_fn, None),
        cpal::SampleFormat::U16 => device.build_input_stream(&config.config(), move |data: &[u16], _| enqueue(data, channels, &tx, &overflow), err_fn, None),
        cpal::SampleFormat::I32 => device.build_input_stream(&config.config(), move |data: &[i32], _| enqueue(data, channels, &tx, &overflow), err_fn, None),
        other => return Err(format!("麦克风音频格式不支持：{other:?}")),
    };
    stream.map_err(|e| format!("无法打开麦克风：{e}。请检查默认输入设备和桌面应用麦克风权限。"))
}
pub fn recognize(dir: &Path, stop: &AtomicBool, event: impl Fn(&str, &str)) -> Result<(), String> {
    event("loading", "正在加载本地语音模型…");
    let mut cache = ENGINE.get_or_init(|| Mutex::new(None)).lock().map_err(|_| "语音引擎异常，请重启应用")?;
    if cache.is_none() { *cache = Some(create_engine(&dir)?); }
    let engine = cache.as_ref().unwrap();
    if stop.load(Ordering::SeqCst) { return Ok(()); }
    let vad = create_vad(&dir)?;
    let device = cpal::default_host().default_input_device().ok_or("未找到默认麦克风，请连接输入设备")?;
    let config = device.default_input_config().map_err(|e| format!("无法读取麦克风设置：{e}"))?;
    let resampler = LinearResampler::create(config.sample_rate().0 as i32, RATE).ok_or("音频重采样初始化失败")?;
    let (tx, rx) = mpsc::sync_channel(512);
    let overflow = Arc::new(AtomicBool::new(false));
    let stream = capture(&device, &config, tx, overflow.clone())?;
    if stop.load(Ordering::SeqCst) { return Ok(()); }
    stream.play().map_err(|e| format!("麦克风启动失败：{e}"))?;
    event("listening", "");
    let mut pending = Vec::new();
    let mut emit = |text: String| event("result", &text);
    let mut feed = |samples: &[f32], flush| -> Result<(), String> {
        pending.extend(resampler.resample(samples, flush));
        let complete = pending.len() / WINDOW * WINDOW;
        for chunk in pending[..complete].chunks_exact(WINDOW) { vad.accept_waveform(chunk); }
        pending.drain(..complete);
        drain(&vad, engine, &mut emit)
    };
    let started = Instant::now();
    while !stop.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(120) {
        if overflow.load(Ordering::Relaxed) { return Err("本机识别速度不足，已停止录音，请缩短每次说话时长".into()); }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(samples) => feed(&samples?, false)?,
            Err(mpsc::RecvTimeoutError::Timeout) => {},
            Err(_) => return Err("麦克风已断开".into()),
        }
    }
    // Stop capture first, then finish the final phrase rather than discarding it.
    drop(stream);
    event("stopping", "");
    for samples in rx.try_iter() { feed(&samples?, false)?; }
    feed(&[], true)?;
    drop(feed);
    if !pending.is_empty() { pending.resize(WINDOW, 0.0); vad.accept_waveform(&pending); }
    vad.flush();
    drain(&vad, engine, &mut emit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_metadata_without_damaging_text() {
        assert_eq!(clean_text("<|zh|><|NEUTRAL|><|Speech|>今天买牛奶。<|withitn|>"), "今天买牛奶。");
        assert_eq!(clean_text("hello <|unfinished"), "hello <|unfinished");
    }
    #[test]
    fn downmixes_channels_and_bounds_audio_queue() {
        let (tx, rx) = mpsc::sync_channel(1);
        let full = AtomicBool::new(false);
        enqueue(&[32767i16, -32767, 16384, 16384], 2, &tx, &full);
        enqueue(&[0i16, 0], 2, &tx, &full);
        assert!(full.load(Ordering::Relaxed));
        let samples = rx.try_recv().unwrap().unwrap();
        assert_eq!(samples[0], 0.0);
        assert!((samples[1] - 0.5).abs() < 0.001);
    }
    #[test]
    #[ignore = "Runs downloaded model against Chinese speech without microphone"]
    fn chinese_offline_recognition() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("voice-assets");
        let started = Instant::now();
        let engine = create_engine(&dir).unwrap();
        eprintln!("Model load: {:?}", started.elapsed());
        let wave = sherpa_onnx::Wave::read(dir.join("zh.wav").to_str().unwrap()).unwrap();
        assert_eq!(wave.sample_rate(), RATE);
        let started = Instant::now();
        let text = decode(&engine, wave.samples()).unwrap();
        eprintln!("Direct recognition: {text} ({:?}, {:.2}s audio)", started.elapsed(), wave.samples().len() as f32 / RATE as f32);
        // Upstream's INT8 example can produce 开饭/开放 for the first two characters.
        // Keep a full reference and tolerate at most one substitution; do not just test nonempty.
        let expected = "开放时间早上9点至下午5点。";
        let errors = text.chars().zip(expected.chars()).filter(|(a, b)| a != b).count();
        assert_eq!(text.chars().count(), expected.chars().count());
        assert!(errors <= 1, "Unexpected transcription: {text}");
        let vad = create_vad(&dir).unwrap();
        for chunk in wave.samples().chunks(WINDOW) {
            let mut padded = chunk.to_vec(); padded.resize(WINDOW, 0.0); vad.accept_waveform(&padded);
        }
        vad.flush();
        let mut parts = Vec::new();
        drain(&vad, &engine, &mut |text| parts.push(text)).unwrap();
        assert!(parts.concat().contains("早上9点至下午5点"), "VAD must preserve the Chinese utterance: {parts:?}");
        let vad = create_vad(&dir).unwrap();
        for _ in 0..100 { vad.accept_waveform(&[0.0; WINDOW]); }
        vad.flush();
        assert!(vad.is_empty(), "Silence must not produce a phrase");
    }
}
