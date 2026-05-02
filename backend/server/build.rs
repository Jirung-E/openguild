use std::path::PathBuf;

fn main() {
    // CARGO_MANIFEST_DIR = backend/server/
    // 한 단계 위 = backend/  ← cargo run의 CWD
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_dir = manifest_dir.parent().unwrap();

    let guild_file = workspace_dir.join("test.guild");
    if !guild_file.exists() {
        let _ = std::fs::write(
            &guild_file,
            "name = \"Test Guild\"\nversion = \"1.0\"\ncreated_at = \"2026-05-02\"\n",
        );
    }

    println!("cargo:rerun-if-changed=build.rs");
}
