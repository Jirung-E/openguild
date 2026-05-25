# gui — OpenGuild Desktop (Tauri v2)

Tauri v2 데스크탑 앱. Rust shell + Svelte (SvelteKit static) 프론트엔드.

## 구조

```
gui/
├── Cargo.toml           ← Rust crate
├── build.rs             ← tauri-build 호출
├── tauri.conf.json      ← Tauri v2 설정 (fileAssociations 포함)
├── capabilities/
│   └── default.json     ← v2 capability (메인 윈도우 권한)
├── icons/               ← OpenGuild 아이콘 (32/64/128/128@2x/icns/ico + Windows Store / Android / iOS)
├── src/
│   ├── lib.rs           ← Tauri Builder + invoke 핸들러 + resolve_guild_path
│   ├── commands.rs      ← invoke 핸들러 23개 (DEV-004)
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

## 실행

```bash
# 현재 디렉토리의 .guild 자동 탐색
cargo run -p openguild-gui

# 특정 길드 디렉토리 명시
cargo run -p openguild-gui /path/to/my-project

# .guild 파일 직접 지정 (더블클릭 시나리오)
cargo run -p openguild-gui /path/to/my-project/my.guild

# env 로 지정
OPENGUILD_GUILD=/path/to/my-project cargo run -p openguild-gui
```

## guild 경로 우선순위

`resolve_guild_path()` 가 다음 순서로 결정:

1. **CLI argv[1]** — 첫 번째 positional 인자.
   - `.guild` 확장자 파일 → 부모 디렉토리를 길드로.
   - 디렉토리 → 그대로 사용.
2. `OPENGUILD_GUILD` env.
3. cwd 부터 부모 방향 `.guild` 탐색 (git 방식).
4. cwd fallback — `.guild` 없어도 빈 길드로 부트스트랩.

## `.guild` 파일 연결 (OS file association)

`gui/tauri.conf.json` 의 `bundle.fileAssociations` 에 `.guild` 확장자 등록.
**배포 빌드 (`cargo tauri build`) 시점에 OS 별 설치 패키지에 자동 포함**.

### Windows (개발 중 수동 등록)

`tauri build` 로 생성한 `.msi` / `.exe` 설치 패키지가 자동 등록.
개발 중 (`cargo run`) 에는 PowerShell 로 수동 등록:

```powershell
# 1. .guild 확장자를 ProgID OpenGuildFile 에 연결
New-Item -Path "HKCU:\Software\Classes\.guild" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\.guild" -Name "(Default)" -Value "OpenGuildFile"

# 2. ProgID 의 실행 명령 — debug 빌드 절대 경로 사용
$exe = (Resolve-Path .\target\debug\openguild-gui.exe).Path
New-Item -Path "HKCU:\Software\Classes\OpenGuildFile\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\OpenGuildFile\shell\open\command" `
  -Name "(Default)" -Value "`"$exe`" `"%1`""

# 3. 아이콘 (선택)
New-Item -Path "HKCU:\Software\Classes\OpenGuildFile\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\OpenGuildFile\DefaultIcon" `
  -Name "(Default)" -Value "`"$((Resolve-Path .\gui\icons\icon.ico).Path)`""
```

해제: `Remove-Item -Path "HKCU:\Software\Classes\.guild" -Recurse;
Remove-Item -Path "HKCU:\Software\Classes\OpenGuildFile" -Recurse`.

### macOS

`tauri build` 의 `.app` 번들이 `Info.plist` 에 `CFBundleDocumentTypes` 자동 등록.
개발 중에는 LaunchServices 가 dev 바이너리에 ProgID 가 없어 등록 못 함.

### Linux

`tauri build` 의 `.deb` / `.rpm` / `.AppImage` 가 `.desktop` 엔트리에 MimeType
`application/x-openguild` 등록. `update-desktop-database` + `xdg-mime` 가 적용.
개발 중에는 수동:

```bash
xdg-mime default openguild-gui.desktop application/x-openguild
```

## 아이콘

`gui/icons/*` 는 OpenGuild 아이콘 (DEV-035, 2026-05-25). `source.png` (1254×1254)
를 source 로 `cargo tauri icon ./icons/source.png` 한 번에 전체 세트 생성:
- `icon.ico` — Windows 멀티사이즈 (6-size RGBA), exe / 파일 탐색기 / 작업표시줄.
- `icon.icns` — macOS 멀티사이즈 (1.1MB), .app 번들.
- `32x32.png` / `64x64.png` / `128x128.png` / `128x128@2x.png` — Linux / 일반.
- `icon.png` — 512×512 master.
- `Square*Logo.png` / `StoreLogo.png` — Windows Store 자산.
- `android/` / `ios/` — 모바일 자산 (현재 빌드 대상 아니지만 자동 생성).

재생성: source.png 만 교체 후 `cd gui && cargo tauri icon ./icons/source.png`.

**빌드 캐시 함정** (debug / release 둘 다): tauri-build 의 build script 가
icon resource 를 `target/{debug,release}/build/openguild-gui-*/out/` 에
캐시함. icon.ico 변경 후 그냥 `cargo build` 하면 옛 아이콘 그대로 임베드.
`cargo clean -p openguild-gui` 도 이 디렉토리는 안 지움 — 수동 삭제 필수:
```bash
cargo clean -p openguild-gui
rm -rf target/debug/build/openguild-gui-* target/release/build/openguild-gui-*
cargo build --release -p openguild-gui
```
Windows Explorer 가 같은 경로의 exe 아이콘을 또 캐시함 — 갱신 안 되면
`ie4uinit.exe -show` 또는 `%LocalAppData%\IconCache.db` 삭제 + explorer
재시작.

## Installer 빌드 (Windows NSIS — DEV-034)

`bundle.targets = ["nsis"]`. `gui/nsis/installer.nsi` 가 default
template 을 확장한 custom version (MUI_PAGE_COMPONENTS + Section 분리).
사용자가 설치 마법사에서 GUI / CLI / Server / Add-to-PATH 를 체크박스로
선택 가능. Server 만 기본 unchecked.

```bash
# 1) CLI / Server release 빌드 — NSIS template 이 target/release/openguild.exe
#    와 openguild-server.exe 를 ..\..\ 상대경로로 참조하므로 미리 있어야 함.
cargo build --release -p openguild-cli -p openguild-server

# 2) (필요 시) icon / build-script 캐시 정리 — 위 "빌드 캐시 함정" 참고.

# 3) NSIS 빌드 — tauri-cli 가 frontend build → release build → makensis.
cd gui && cargo tauri build
#   → target/release/bundle/nsis/OpenGuild_<ver>_x64-setup.exe (~9.2 MB)
```

setup.exe / uninstall.exe 의 파일 아이콘은 `tauri.conf.json::bundle.windows.nsis.installerIcon`
+ `uninstallerIcon` 으로 지정 (안 하면 NSIS 기본 다운로드 아이콘 남음).
custom template 에 `Icon` / `UninstallIcon` 명령 명시.

PATH 추가는 PowerShell 로 HKLM `Environment\PATH` 갱신 — 별도 NSIS plugin
없이 됨. 중복 추가 방지 검사 포함, uninstall 시 자동 제거.

자동 배포 (tag push → GitHub Release) 는 별도 quest **DEV-071**.
