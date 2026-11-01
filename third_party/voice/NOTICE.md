# Local speech recognition attribution

- **SenseVoiceSmall**, Alibaba Group / Tongyi Lab (FunAudioLLM):
  https://github.com/QwenAudio/SenseVoice
  Original weights: https://www.modelscope.cn/models/iic/SenseVoiceSmall
  INT8 ONNX conversion distributed by k2-fsa:
  https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models
  Model name retained: SenseVoiceSmall. See SenseVoice-LICENSE.txt.
- **Silero VAD**, Silero Team, MIT license. ONNX export from k2-fsa:
  https://github.com/snakers4/silero-vad
  See Silero-VAD-LICENSE.txt.
- **sherpa-onnx 1.13.8**, k2-fsa contributors / Xiaomi Corporation,
  Apache-2.0: https://github.com/k2-fsa/sherpa-onnx
  See sherpa-onnx-LICENSE.txt. Windows uses the official **static-MD/no-TTS**
  release. The Rust FFI build script is vendored with this archive selection
  change; no inference code or C bindings were changed.
- **ONNX Runtime**, Microsoft, MIT license. See ONNX-Runtime-LICENSE.txt and
  ONNX-Runtime-ThirdPartyNotices.txt.
- **Kaldi** (Daniel Povey and contributors), **kaldi-native-fbank** and
  **kaldifst** (k2-fsa contributors), **OpenFst** (Google and contributors),
  **SentencePiece** (Google): Apache-2.0, see native-library-NOTICE.txt.
  Sources: https://github.com/kaldi-asr/kaldi,
  https://github.com/csukuangfj/kaldi-native-fbank,
  https://github.com/k2-fsa/kaldifst,
  https://www.openfst.org/ and https://github.com/google/sentencepiece.
- **KissFFT**, Mark Borgerding and contributors, BSD-3-Clause;
  see KissFFT-LICENSE.txt. https://github.com/mborgerding/kissfft

Speech is processed on the user's CPU. Model preparation downloads public
assets only; microphone audio is not sent to these projects or any server.
