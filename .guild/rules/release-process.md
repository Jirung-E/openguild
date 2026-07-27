+++
created_at = "2026-06-23T01:30:44+09:00"
updated_at = "2026-07-27T20:57:38+09:00"
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
- description: `CHANGELOG.md` 의 해당 버전 절 그대로 — **자동화됨**(BUG-171).
  `scripts/extract-release-notes.ps1` 이 태그(`vX.Y.Z`)에 맞는 `## X.Y.Z` 절을
  뽑아 릴리스 본문으로 쓰고 compare 링크를 붙인다. 따라서:
  - 태그를 밀기 전에 CHANGELOG 절이 **반드시** 있어야 한다 — 없거나 헤딩이
    태그와 안 맞으면 릴리스 잡이 그 자리에서 실패한다(의도된 가드).
  - `generate_release_notes` 는 쓰지 않는다. 플랫폼별 잡이 같은 릴리스를 각각
    갱신하는 구조라, 자동 생성을 켜면 본문이 잡 수만큼 중복된다(0.4.1 실사고).
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

## 릴리즈 브랜치를 따로 쓸 땐 **develop 에서 분기** (0.4.1 실사고)

기본은 별도 브랜치 없이 develop → master FF + 태그다(위 절차). 부분 릴리스
등으로 release 브랜치가 필요하면 **반드시 develop 에서 분기**한다.
master 에서 따서 필요한 커밋만 체리픽해 채우면 아래가 전부 터진다 —
0.4.1(예외적으로 master 에서 분기)에서 실제로 다 겪었다:

- **중복 커밋**: 릴리스 후 develop 로 되가져올 때 체리픽 사본이 그대로
  유입된다(0.4.1: 머지된 41 커밋 중 **21 이 중복**).
- **`.guild` 갈라짐**: 캠페인이 링크한 퀘스트 파일이 그 브랜치에 없어
  dangling 이 되고(GUI 칸반이 빈 것처럼 보임), 퀘스트 상태가 브랜치마다
  어긋난다. 퀘스트 `.md` + history 사이드카를 수동 동반해야 했다.
- **카운터 오염**: index.db 는 브랜치를 안 따라간다([[BOOK-001]]). 브랜치
  전환 후 SQL 카운터가 파일보다 뒤처지면(0.4.1: SQL DEV 270 ↔ 파일 300)
  ID 발급이 **기존 파일과 충돌**할 수 있다(발급은 DB MAX 기준 self-heal).
- **스키마 경고**: 두 브랜치의 마이그레이션 개수가 다르면, 새 쪽 바이너리가
  만든 index.db 를 옛 쪽 앱이 열 때 "DB 가 더 새롭다" 경고가 뜬다.

### 부득이 master 에서 분기했다면 (릴리스 후 정리)

1. `git merge master` 로 develop 에 되가져온다 — 조상 관계가 생겨 다음
   릴리스 싱크가 자동화된다(중복 로그는 `git log --first-parent` 로 회피).
2. 아래 4항목은 머지가 자동으로 못 맞추므로 **직접 확인**한다(0.4.1 에선
   전부 갭이었다):
   - 버전 파일 6개 + `skills/openguild-plugin/.claude-plugin/plugin.json`
   - CHANGELOG — 확정된 릴리스 절 편입 + `Unreleased` 에서 **이미 출시된
     항목 제거**(안 하면 다음 릴리스 노트에 중복)
   - 스킬(`.agents/` · `skills/`) — 릴리스 브랜치에서만 고친 게 있는지
   - `.guild` 퀘스트/캠페인 상태(done) — 캠페인은 별도 확인
3. 머지 충돌 해결 원칙: **소스는 develop**(기능이 앞섬, 단 릴리스 수정
   마커를 grep 으로 전수 검증) · **퀘스트/캠페인 상태는 릴리스 브랜치** ·
   **history 사이드카는 합집합**(append-only 감사 로그 — 어느 쪽도
   상위집합이 아닐 수 있다) · **카운터는 큰 쪽**(단조 증가).
4. 브랜치 전환/머지 후 `rm .guild/index.db && openguild reindex` →
   `openguild check drift` + `openguild check counters` 로 확인.
