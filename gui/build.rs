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
    tauri_build::build();

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
