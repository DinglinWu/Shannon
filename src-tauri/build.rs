fn main() {
    if std::env::var("PROFILE").as_deref()==Ok("release") {
        let native=std::env::var_os("CARGO_FEATURE_NATIVE_RUNTIME").is_some();
        let config=std::env::var("TAURI_CONFIG").unwrap_or_default();
        if native {assert!(std::env::var_os("SHANNON_PLUGIN_RUNTIME_BUILD").is_some(),"Plugin code may only be built by scripts/build/build_official_plugins.py for separate distribution");}
        else {assert!(config.contains("dist-core"),"Release launcher must use tauri.core.conf.json; plugin pages must not be embedded");}
    }
    tauri_build::build()
}
