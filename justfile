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
dev-desktop:
    cd gui && cargo tauri dev

# API 서버 dev 모드 (cwd 의 .guild 사용)
dev-server:
    cargo run --bin openguild-server

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
build: build-rust build-frontend

# --- 테스트 (test) ---

# Rust workspace 전체 테스트
test-rust:
    cargo test --workspace

# Frontend vitest
test-frontend:
    cd gui/frontend && npm test -- --run

# 전체 테스트
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
