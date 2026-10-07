fn main() {
    // Exe + tray icon (resource id 1). No-op when not targeting Windows.
    embed_resource::compile("assets/snip.rc", embed_resource::NONE).manifest_optional().unwrap();
}
