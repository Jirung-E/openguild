# gui — OpenGuild Desktop (Tauri v2)

Tauri v2 데스크탑 앱. Rust shell + Svelte (SvelteKit static) 프론트엔드.

## 구조

```
gui/
├── Cargo.toml           ← Rust crate
├── build.rs             ← tauri-build 호출
├── tauri.conf.json      ← Tauri v2 설정
├── capabilities/
│   └── default.json     ← v2 capability (메인 윈도우 권한)
├── icons/               ← placeholder 아이콘 (32/128/icns/ico)
├── src/
│   ├── lib.rs           ← Tauri Builder + invoke 핸들러 (재사용 가능 entry)
│   └── main.rs          ← desktop bin
└── frontend/            ← Svelte UI (별도 npm 프로젝트)
    └── build/           ← vite build 결과 (gitignore)
```

## 부트스트랩 (fresh clone)

`cargo check -p openguild-gui` 는 `gui/frontend/build/index.html` 이 존재해야 통과한다.
처음 한 번은 frontend 를 빌드해야 한다:

```bash
cd gui/frontend && npm install && npm run build
```

이후 워크스페이스 명령:

```bash
cargo check --workspace     # 전체 crate 컴파일 확인
cargo test --workspace      # 전체 테스트
```

## 개발 (예정 — DEV-004 이후)

```bash
# Rust + frontend 통합 dev 모드 (tauri-cli 설치 후)
cargo tauri dev
```

현재 (DEV-003) 는 crate 초기화 + workspace 등록까지. invoke 핸들러는 `ping` 하나뿐
(`pong` 반환). 본격적 핸들러 (DEV-004) / 파일 연결 (DEV-005) / Recent guild (DEV-006)
은 후속 quest.

## 아이콘

`gui/icons/*` 는 placeholder (PowerShell 생성, 32×32 단색). 실제 아이콘 디자인 후
교체 필요 — 별도 quest 로 추적할 것.
