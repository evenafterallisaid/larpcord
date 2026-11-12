fn main() {
    #[cfg(target_os = "windows")]
    embed_resource::compile("packaging/windows/app.rc", embed_resource::NONE)
        .manifest_required()
        .expect("embed larpcord branding");
}
