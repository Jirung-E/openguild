+++
created_at = "2026-06-23T01:30:44+09:00"
updated_at = "2026-08-12T09:38:34+09:00"
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
        "url": "https://github.com/.../openguild_X.Y.Z_x64-setup.exe"
      },
      "darwin-aarch64": {
        "signature": "<minisign>",
        "url": "https://github.com/.../openguild.app.tar.gz"
      },
      "linux-x86_64": {
        "signature": "<minisign>",
        "url": "https://github.com/.../openguild.AppImage"
      }
    }
  }
  ```
- **`latest.json` 은 빌드 잡이 만들지 않는다** (DEV-314). updater 엔드포인트는
  파일 하나(`releases/latest/download/latest.json`)뿐이라, 플랫폼별 잡이 각자
  같은 이름으로 올리면 **마지막 잡이 이겨서 나머지 플랫폼이 통째로 사라진다.**
  빌드 잡은 설치 파일과 `.sig` 만 릴리스에 올리고, 마지막 `updater-manifest`
  잡이 **릴리스에 실제로 올라간 `*.sig` 자산**을 읽어 하나로 합친다.
  - 새 플랫폼 추가 시 해당 빌드 잡의 artifact/Release `.sig` 업로드,
    `workflow_dispatch` artifact 다운로드, `platform_of()` 매핑과 expected set을
    함께 갱신한다.
  - 서명 시크릿이 없으면 `.sig` 자체가 없으므로 `latest.json` 없이 릴리스가
    끝난다 — 설치 파일은 정상, 자동 업데이트만 비활성.
- BUG-045 (예정): `latest.json` 가 없으면 사용자 GUI 의 "업데이트 확인" 이 그냥
  실패. 첫 release 에서 반드시 포함.

## 사후 점검

- [ ] release page 에서 installer 다운로드 / 설치 / 실행 확인.
- [ ] `latest.json` 의 `platforms` 에 **그 릴리스가 지원하는 플랫폼이 전부**
      들어 있는지 (DEV-314 이전엔 한 플랫폼만 남는 사고가 가능한 구조였다).
      현재 대상: `windows-x86_64`, `darwin-aarch64`, `linux-x86_64`.
- [ ] macOS: dmg 열기 → Applications 로 드래그 → 첫 실행이 Gatekeeper 안내대로
      우클릭>열기로 통과되는지 (미서명 배포라 정상 동작이다).
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

## 릴리스 파이프라인을 "시험용 태그"로 테스트하지 말 것 (BUG-171)

updater 엔드포인트가 `releases/latest/download/latest.json` 이고 워크플로가
`prerelease: false` 로 하드코딩돼 있다. 즉 **아무 태그나 밀면 그게 곧
"latest"** 가 되어 모든 사용자 앱이 그 버전을 업데이트로 제안한다. 파이프라인
검증용 태그(`v0.0.0-test` 등)를 밀면 사용자에게 시험 빌드가 배포된다.

대신 릴리스 없이 검증하는 경로를 쓴다:

- `check` 워크플로의 **release notes extraction** 잡 — 매 push 마다 CHANGELOG
  최신 출시 버전으로 추출을 돌리고 compare 링크·인코딩 손상까지 검사한다.
  결과 본문은 `release-notes-preview` artifact 로 확인.
- `release` 워크플로를 **workflow_dispatch** 로 수동 실행 — 릴리스는 만들지
  않고(태그 gate) 추출만 돌려 `release-notes-{OS}` artifact 로 실제 러너의
  본문을 확인할 수 있다. 같은 실행의 `updater-manifest-preview`에는 Windows,
  macOS, Linux 서명을 합친 `latest.json`이 들어간다.

정말 태그 기반 검증이 필요하면 그때는 `prerelease: true` 를 먼저 넣고
(latest 에서 제외됨) 검증 후 태그·릴리스를 삭제한다.

## 릴리스 노트에는 **요약**을 반드시 쓴다 (사람이 쓰는 부분)

CHANGELOG 의 각 버전 절은 `### Added` 같은 카테고리 목록 **앞에** 요약이
와야 한다. 릴리스 본문은 이 절을 그대로 싣기 때문에, 요약이 없으면 릴리스
페이지가 20~30개 항목의 평평한 목록이 되어 "이번 버전이 뭘 바꿨는지"가
읽히지 않는다. 이 부분은 자동 생성할 수 없다 — 항목을 기계적으로 요약해도
중요도 판단이 빠지기 때문이다. `extract-release-notes.ps1` 이 요약 없는 절을
**에러로 막는다**(절이 `### ` 로 시작하면 실패).

형식:

1. **2~4줄 문단** — 이 릴리스의 성격을 사용자 관점으로. "무엇을 할 수 있게
   됐는지 / 무엇이 덜 아프게 됐는지". 예: *"문서를 오가는 비용을 줄이는 데
   초점을 둔 릴리스. …"*
2. **`**주요 변경점**` + 3~5개 불릿** — 각 불릿은
   `**굵은 한 줄 제목** — 무엇이 달라졌는지 1~2문장 (퀘스트 ID)`.

쓸 때 판단 기준:

- **묶어서 말한다.** 한 주제를 여러 퀘스트가 나눠 구현한 경우 하나의 불릿으로
  합친다(예: DEV-276 + DEV-294 = "최근 본 문서"). 퀘스트 단위로 나열하면
  요약이 아니라 목록의 반복이다.
- **영향이 큰 것부터.** 사용자가 실제로 부딪히던 문제(실패·느림·불가)를 위로.
  내부 리팩터링·CI·문서 작업은 요약에서 빼고 카테고리 목록에만 남긴다.
- **숫자를 쓴다.** "빨라졌다"가 아니라 "0.5초 → 15밀리초", "1.5 MB → 64 MB".
- **퀘스트 ID 는 괄호로만.** 제목에 넣지 않는다 — 읽는 사람은 ID 를 모른다.
- **3~5개를 넘기지 않는다.** 전부 중요하다고 쓰면 아무것도 강조되지 않는다.
  넘칠 것 같으면 주제를 더 크게 묶는다.
