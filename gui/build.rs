// BUG-038: `cargo build -p openguild-gui` (또는 `cargo run`) 는 Tauri CLI 가
// 아닌 일반 cargo 흐름이라 `tauri.conf.json` 의 `beforeBuildCommand`
// (`npm run build`) 가 실행되지 않음 → frontend 자산 stale 인 채로 embed.
// 매 release 마다 사용자가 수동으로 `cd gui/frontend && npm run build` 해야
// 했음.
//
// 본 build.rs 가 frontend src 변경을 감지하면 자동으로 `npm run build` 실행.
//
// 환경 안전망:
// - `OPENGUILD_SKIP_FRONTEND=1` 로 명시 skip (CI / docker / 빠른 backend 만
//   iter 시).
// - npm 미설치 환경에서는 warning 만 출력하고 기존 `frontend/build/` 그대로
//   embed (강제 fail 안 함 — 사용자가 frontend 미터치 백엔드 작업 중일 수
//   있음).
fn main() {
    // BUG-349: 프런트를 **먼저** 만든다. tauri-build 는 `frontend/build` 가 있는지부터 보고 없으면 멈추는데, 예전엔
    // 그 검사가 먼저라 새로 받은 저장소(`frontend/build` 없음)에서는 아래 안전망(BUG-038)까지 못 왔다 — `just test` 가
    // 한 번에 안 돌았다. CI 는 프런트를 따로 먼저 만들어 안 보였다.
    build_frontend();

    // BUG-349: 윈도우 매니페스트(Common Controls v6)를 tauri 대신 **링커로 모든 대상에** 넣는다. tauri 는 실행 파일에만
    // 넣어서, 같은 crate 의 시험 실행 파일은 comctl32 v5 를 잡고 `TaskDialogIndirect` 를 못 찾아 뜨지도 못했다
    // (STATUS_ENTRYPOINT_NOT_FOUND) — 윈도우에서 gui 시험이 한 번도 안 돈 이유다. 둘 다 넣으면 매니페스트가 겹쳐 링크가
    // 깨지므로 tauri 쪽은 끈다. 내용은 tauri 기본과 같다(`windows-app-manifest.xml`).
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri-build 실패");
    embed_windows_manifest();
}

/// BUG-038: frontend src 가 바뀌었으면 `npm run build` — 아래 설명은 원래 자리에 있던 것.
fn build_frontend() {
    // BUG-038: frontend src 또는 빌드 설정이 변경되면 npm rerun.
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/static");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/package-lock.json");
    println!("cargo:rerun-if-changed=frontend/svelte.config.js");
    println!("cargo:rerun-if-changed=frontend/vite.config.ts");
    println!("cargo:rerun-if-changed=frontend/tsconfig.json");

    if std::env::var("OPENGUILD_SKIP_FRONTEND").is_ok() {
        println!(
            "cargo:warning=OPENGUILD_SKIP_FRONTEND set — skipping frontend build (embed uses existing frontend/build)"
        );
        return;
    }

    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let status = std::process::Command::new(npm)
        .args(["run", "build"])
        .current_dir("frontend")
        .status();
    match status {
        Ok(s) if s.success() => {
            println!("cargo:warning=BUG-038 frontend rebuilt (npm run build)");
        }
        Ok(s) => {
            // 실제 build 실패 — embed 시 stale 이 됨. 명시적 panic.
            panic!(
                "frontend `npm run build` failed (exit {:?}). \
                 BUG-038 안전망: set `OPENGUILD_SKIP_FRONTEND=1` to bypass.",
                s.code()
            );
        }
        Err(e) => {
            // npm 자체 미설치 / 실행 불가 — backend-only 사용자 안전망.
            println!(
                "cargo:warning=BUG-038 npm not runnable ({e}) — skipping frontend build, embed will reuse existing frontend/build"
            );
        }
    }
}

/// BUG-349: MSVC 링커에 매니페스트를 넣게 한다 — `rustc-link-arg` 는 이 crate 의 **모든** 대상(앱 · 시험)에 붙는다.
fn embed_windows_manifest() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || env != "msvc" {
        return;
    }
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
