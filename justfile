# OpenGuild 개발 단축 명령. `just <recipe>` 로 호출.
# 설치: https://github.com/casey/just

# Windows: cmd.exe 사용. `cd dir && cmd` 형태가 그대로 동작.
# (sh 의존성 제거 — Git Bash 등 없어도 OK)
set windows-shell := ["cmd.exe", "/c"]

# 기본: 사용 가능한 recipe 목록
default:
    @just --list

# --- 개발 (dev) ---

# Svelte dev 서버만 (브라우저로 작업 시)
dev-frontend:
    cd gui/frontend && npm run dev

# Tauri desktop 앱 dev 모드 (frontend 자동 동반 실행)
# OPENGUILD_SKIP_FRONTEND=1 필수: cargo tauri dev 는 beforeDevCommand
# (npm run dev) 로 이미 frontend 를 서빙하는데, gui/build.rs 의 BUG-038
# npm-build 안전망까지 같이 돌면 그 결과물(frontend/build) 변경을 watcher
# 가 다시 감지해 재빌드 → 다시 npm build → 다시 변경 감지... 무한루프에
# 빠짐(macOS 에서 재현 — 흰 화면에서 멈춘 것처럼 보임). dev 모드는 embed
# 자산이 필요 없어 안전하게 skip 가능.
[unix]
dev-desktop:
    cd gui && OPENGUILD_SKIP_FRONTEND=1 cargo tauri dev

[windows]
dev-desktop:
    cd gui && set OPENGUILD_SKIP_FRONTEND=1 && cargo tauri dev

# API 서버 dev 모드 (cwd 의 .guild 사용)
dev-server:
    cargo run --bin openguild-server -- host

# CLI dev 빌드 후 실행 (인자 전달: just dev-cli quest list)
dev-cli *args:
    cargo run --bin openguild -- {{args}}

# --- 빌드 (build) ---

# Svelte 정적 자산 빌드 → gui/frontend/dist
build-frontend:
    cd gui/frontend && npm run build

# Tauri desktop binary release 빌드
build-desktop:
    cd gui && cargo tauri build

# 모든 Rust crate release 빌드
build-rust:
    cargo build --workspace --release

# 전체 빌드 (Rust + frontend)
# 순서 중요: gui 의 tauri bundle resources 가 frontend/build 를 요구 —
# frontend 를 먼저 만들어야 build-rust 가 성공 (BUG-107).
build: build-frontend build-rust

# 전체 debug 빌드 (frontend 정적 자산 + Rust workspace debug profile)
# desktop bundle/app 패키징은 하지 않는다.
build-debug: build-frontend
    cargo build --workspace

# --- 테스트 (test) ---

# Rust workspace 전체 테스트 — **CI(check.yml)와 같은 debug 빌드**.
# clippy -D warnings 도 함께 — cargo test 만으론 안 잡혀서 CI 에서만 터지는
# 사고 방지(check.yml 과 동일 게이트를 push 전에 로컬에서 먼저 통과시킴).
#
# BUG-251: 예전엔 `--release` 였다(디스크 절약). 그런데 릴리즈 빌드는
# `debug_assertions` 가 꺼져 있어 **clap 의 인자 정의 검증이 통째로 사라진다** —
# `conflicts_with` 에 없는 인자를 적어도 로컬은 조용히 통과하고 CI(debug)에서만
# 터졌다. 실제로 그렇게 한 번 겪었다. "CI 와 동일한 게이트" 라는 이 레시피의
# 목적 자체가 무너지므로 debug 로 맞춘다.
test-rust:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

# Frontend 검증 — check.yml 의 gui-frontend job 과 **같은 순서·같은 항목**.
# BUG-251: `check:no-emoji` 가 빠져 있었다(BUG-169 의 재발 방지 검사).
test-frontend:
    cd gui/frontend && npm run check
    cd gui/frontend && npm run check:no-hex
    cd gui/frontend && npm run check:no-emoji
    cd gui/frontend && npm test -- --run

# 전체 테스트 (CI 와 동일)
test: test-rust test-frontend

# --- 품질 ---

fmt:
    cargo fmt --all
    cd gui/frontend && npm run format

lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cd gui/frontend && npm run lint

check:
    cargo check --workspace
    cd gui/frontend && npm run check

# --- 초기 설치 ---

# 의존성 설치 (clone 후 1회 실행)
install:
    cd gui/frontend && npm install
    cargo fetch
