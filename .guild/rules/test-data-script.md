# 테스트 데이터 주입 스크립트 — 운영 규칙

`scripts/seed-test-data.ps1` 의 목적 / 갱신 절차 / 검증 범위. DEV-075 의 산출
산물이지만 매 신규 기능마다 sync 필요한 운영 규칙이라 길드 룰로 정착.

## 목적

빈 디렉토리에 한 번에 다음 상태의 길드를 만들어 GUI 의 모든 메인 UI 를 한
화면에서 검증:

- 다양한 type / status / urgency 의 quest.
- carousel / conveyor / marquee 임계값 케이스가 전부 채워진 campaign 셋.
- Quest Detail 의 Campaigns 섹션이 비어있지 않게 quest ↔ campaign 연결.
- 마감 임박 / Overdue 뱃지 검증용 due date.
- Rules 페이지 검증용 sample 규칙 (다중 파일).

## 실행

```powershell
cd <빈 폴더>
pwsh -File <openguild repo>/scripts/seed-test-data.ps1
```

환경 변수 `OPENGUILD_BIN` 으로 binary 경로 override 가능. 기본 우선순위:
`OPENGUILD_BIN` > `target/release/openguild.exe` > `target/debug/openguild.exe`
> PATH 의 `openguild`.

## 검증 단계 (= 스크립트의 7 단계)

| 단계 | 내용 | 검증 대상 UI |
|------|------|--------------|
| 1 | `init` | Welcome / 길드 생성 흐름 |
| 2 | 12 quest 생성 (DEV/BUG/REQ 혼합, urgency 1~4) | Quest List / Board / Home 의 "최근 퀘스트" |
| 3 | 일부 quest status 전환 (in_progress / on_hold) | status badge 색상 / 정렬 |
| 4 | **DEV-076** 일부 quest 에 due date (과거/임박/미래) | Home 의 "마감 임박" / Overdue 뱃지 |
| 5 | 12 campaign 생성 (active 5 + upcoming 7 + future 1) + 체크리스트 / 진행률 | Home carousel / conveyor / marquee 임계값 |
| 6 | campaign ↔ quest 연결 | Quest Detail 의 Campaigns 섹션 / Campaign Detail 의 Quests |
| 7 | **DEV-016 multi-file** sample 규칙 3 개 (branch-policy / code-review / release-checklist) | Rules 페이지 sidebar / 선택 / 편집 |

## 갱신 절차 — 신규 기능 추가 시

본 스크립트는 **신규 사용자 노출 기능 (GUI / CLI) 이 release 되면 매번 sync**
해야 진정한 회귀 검증 수단. 새 기능 추가 시:

1. **CLI 명령 확인** — 그 기능을 데이터로 만들 CLI 명령이 존재하는가?
   - 있으면 스크립트 단계 추가 (예: DEV-076 의 `quest due`, DEV-016 의 `rules
     create`).
   - 없으면 **CLI 추가가 선행** (별도 quest). 댓글 (DEV-094) / 태그 (DEV-068)
     처럼 GUI / HTTP only 인 기능은 일단 보류 (또는 file 직접 작성 — 단 그
     경우 `openguild-server reindex` 도 같이 호출해야 함).
2. **스크립트 단계 번호 갱신** — 단계 수가 늘어나면 모든 `[N/M]` 헤더 일관 갱신.
3. **본 규칙 (이 파일) 의 "검증 단계" 표 갱신** — 새 행 추가.
4. **DEV-075 quest 본문에 어떤 fix 가 들어갔는지 짧게 기록** (또는 본 규칙의
   "최근 변경" 섹션 — quest 본문은 1회성이라 영구 기록엔 부적합).

## 안전장치

- `.guild` 가 이미 있는 폴더에서 거부 (실수 덮어쓰기 방지).
- 각 명령 실패 시 즉시 throw (`$ErrorActionPreference = "Stop"` + `$LASTEXITCODE`
  체크).
- PowerShell 5.1 의 cp949 stdout 깨짐 방지 — UTF-8 강제 설정 (`[Console]::
  OutputEncoding = UTF8` + `chcp 65001`).

## 한계 / 비검증

스크립트가 다루지 **않는** 기능 — 별도 절차 또는 수동 확인:

- **댓글 / 메모 (DEV-012, DEV-094)** — CLI 없음. GUI 에서 수동 확인.
- **태그 (DEV-068)** — open 상태 / CLI 없음.
- **첨부파일 (DEV-097)** — open 상태 / 기능 미구현.
- **외부 편집 후 자동 reindex (DEV-095)** — 본 스크립트 후 `openguild-server
  reindex` 로 cache 정합 확인.
- **schema ahead banner (BUG-041)** — 같은 binary 가 만든 DB 라 ahead 안 됨;
  수동 시뮬레이션 필요 (`_sqlx_migrations` 에 fake row INSERT).
- **updater (BUG-045)** — release 가 있어야 의미; production binary 만 동작.

## 최근 변경

- 2026-06-03: DEV-076 due date / DEV-016 multi-file rules 단계 추가.
  스크립트 단계 5 → 7 로 확장. 본 규칙 신설 (DEV-075 quest 본문은 그대로).
