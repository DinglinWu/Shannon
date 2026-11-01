Vendored from crates.io sherpa-onnx-sys 1.13.8 (Apache-2.0).

Changes: Windows static builds select the official static-MD-Release-no-tts
archive rather than static-MT, matching Rust's default MSVC dynamic CRT.
Removed unused piper_phonemize/espeak-ng/ucd link directives for that no-TTS
archive. No C ABI bindings or inference logic have been changed.

Other platforms are currently outside Shannon's local speech support.
