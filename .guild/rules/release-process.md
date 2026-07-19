+++
created_at = "2026-06-23T01:30:44+09:00"
updated_at = "2026-06-23T01:30:44+09:00"
+++
# 릴리즈 패키지 절차

매 릴리즈마다 반복되는 종합 절차. DEV-036 의 본문을 본 길드 규칙으로 정착.

## 사전 점검 체크리스트

- [ ] develop 의 모든 testing quest 가 done 으로 정리.
- [ ] `cargo test --workspace` 통과.
- [ ] `cd gui/frontend && npm test -- --run` 통과.
- [ ] `cd gui/frontend && npm run check` 0 errors.
- [ ] `cd gui/frontend && npm run build` 성공 (Tauri 가 embed 할 frontend 자산).
- [ ] `cargo tauri build` 정상 (3 OS 각각 — Windows / macOS / Linux).
- [ ] AGENTS.md / docs/planning.md / docs/architecture-refactor.md 갱신.
- [ ] 알려진 BUG 의 closure 또는 known-issue 명시.
- [ ] BUG-041 후속: `core/migrations/` 의 새 mig 가 있다면 `_sqlx_migrations`
      에 영향 — README 의 "복구" 절차 인지.

## 버전 동기화

다음 파일들의 version 값을 `X.Y.Z` 로 **일관 갱신**:

- `core/Cargo.toml`
- `cli/Cargo.toml`
- `server/Cargo.toml`
- `gui/Cargo.toml`
- `gui/tauri.conf.json` (`version` 필드)
- `gui/frontend/package.json`

후속 quest 후보: 한 곳 (`[workspace.package]`) 에서 관리하도록 통합.

## CHANGELOG

`CHANGELOG.md` 신설 / 갱신 — Keep-a-Changelog 형식:

```
## X.Y.Z — YYYY-MM-DD

### Added
- ...

### Changed
- ...

### Fixed
- ...

### Known issues
- ...
```

## tag 작업 절차

```bash
git checkout develop
# 모든 testing 정리, 버전 업데이트 commit
git commit -m "[chore] X.Y.Z 버전"

git checkout master
git merge develop                # FF 권장 (사용자 정책)
git tag vX.Y.Z
git push origin master --tags
```

## GitHub Release artifact

- DEV-034 의 workflow 가 tag push 트리거로 installer 자동 첨부.
- description: `CHANGELOG.md` 의 해당 버전 절 그대로.
- **`latest.json` 도 attach** — Tauri updater 가 `endpoints` 로 가리키는 파일.
  형식:
  ```json
  {
    "version": "X.Y.Z",
    "notes": "release notes",
    "pub_date": "YYYY-MM-DDTHH:MM:SSZ",
    "platforms": {
      "windows-x86_64": {
        "signature": "<minisign>",
        "url": "https://github.com/.../openguild-gui_X.Y.Z_x64-setup.nsis.zip"
      }
    }
  }
  ```
- BUG-045 (예정): `latest.json` 가 없으면 사용자 GUI 의 "업데이트 확인" 이 그냥
  실패. 첫 release 에서 반드시 포함.

## 사후 점검

- [ ] release page 에서 installer 다운로드 / 설치 / 실행 확인.
- [ ] `openguild-gui --version` 출력이 새 version 맞는지.
- [ ] BUG-041 의 SchemaAheadBanner 가 구버전 길드 열 때 정상 표시되는지.
- [ ] Tauri updater (구 release 에서) 새 release 감지 동작 확인.

## 후속

- 사용자 피드백 채널 (GitHub Issues / discussions).
- 메이저 cleanup quest (멀티유저 / JWT / Campaign / 등).
- auto-updater (Tauri updater plugin) 의 첫 release 검증.
