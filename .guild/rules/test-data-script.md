+++
created_at = "2026-07-07T09:32:56+09:00"
updated_at = "2026-08-06T12:50:03+09:00"
+++
# 테스트 데이터 주입 스크립트 — 운영 규칙

`scripts/seed-test-data.mjs` 의 목적 / 갱신 절차 / 검증 범위. DEV-075 의 산출
산물이지만 매 신규 기능마다 sync 필요한 운영 규칙이라 길드 룰로 정착.
DEV-320(2026-08-06)에서 PowerShell(`seed-test-data.ps1`)에서 Node 로 재작성 —
pwsh 가 로컬 mac/linux 개발 환경에 기본 설치돼 있지 않고, 이식성 버그(바이너리
탐색 `.exe` 하드코딩, `$env:TEMP` 가 mac/linux 에서 빈 값)도 있었다.

## 목적

빈 디렉토리에 한 번에 다음 상태의 길드를 만들어 GUI 의 모든 메인 UI 를 한
화면에서 검증:

- 다양한 type / status / urgency 의 quest.
- carousel / conveyor / marquee 임계값 케이스가 전부 채워진 campaign 셋.
- Quest Detail 의 Campaigns 섹션이 비어있지 않게 quest ↔ campaign 연결.
- 마감 임박 / Overdue 뱃지 검증용 due date.
- Rules 페이지 검증용 sample 규칙 (다중 파일).

## 실행

```sh
cd <빈 폴더>
node <openguild repo>/scripts/seed-test-data.mjs
```

바이너리 선택 (첫 위치 인자 = 바이너리가 들어있는 폴더):

- 인자 없음 → PATH 의 `openguild` 사용 (기본).
- `node seed-test-data.mjs <폴더>` → 그 폴더의 `openguild`(윈도우는
  `openguild.exe`, 플랫폼별로 자동 판단).
- 둘째 인자로 길드 이름 지정 가능 (기본 `test-guild`):
  `node seed-test-data.mjs <바이너리폴더> my-guild`.

순수 Node 내장 모듈(child_process/fs/os/path)만 사용 — `npm install` 불필요.
`npm run dev`/`cargo build` 등으로 이미 최신 빌드가 있다면 그 폴더
(`target/debug` 또는 `target/release`)를 첫 인자로 지정.

## 검증 단계 (= 스크립트의 11 단계)

| 단계 | 내용 | 검증 대상 UI |
|------|------|--------------|
| 1 | `init` | Welcome / 길드 생성 흐름 |
| 2 | 12 quest 생성 (DEV/BUG/REQ 혼합, urgency 1~4) | Quest List / Board / Home 의 "최근 퀘스트" |
| 3 | 일부 quest status 전환 (in_progress / on_hold) | status badge 색상 / 정렬 |
| 4 | **DEV-076** 일부 quest 에 due date (과거/임박/미래) | Home 의 "마감 임박" / Overdue 뱃지 |
| 5 | 13 campaign 생성 (active 5 + upcoming 7 + future 1) + 체크리스트 / 진행률 | Home carousel / conveyor / marquee 임계값 |
| 6 | campaign ↔ quest 연결 + 관계(하위/선행)/태그/soft-delete/템플릿 | Quest Detail 의 Campaigns 섹션 / 보드 엣지·트리·의존성 그래프 / 태그 칩 / 삭제 목록 / NewQuestModal 템플릿 드롭다운 |
| 7 | **DEV-094/099/102** 첫 quest 에 댓글(top+reply+토론 미해결/해결+토론 답글) + 메모 + 첨부 3개 | Quest Detail 댓글/메모/첨부 섹션 + DB 캐시 sync (snapshot 안 살아남는지) |
| 8 | **DEV-016 multi-file** sample 규칙 3 개 + 변경 이력 데모 | Rules 페이지 sidebar / 선택 / 편집 / 상세의 변경 이력 |
| 9 | **DEV-215~218, DEV-239** 도서관 문서 3 개(본문+[[cross-link]] / 빈 본문 / 폴더 안) + 폴더 1개 + 댓글의 [[BOOK-001]] 참조 | Library 페이지 목록/편집 / 딥링크 / 렌더·자동완성의 도서관 링크 / 폴더 트리·탐색기 보기 토글 / 경로 기반 자동완성 |
| 10 | **DEV-167** worklog 노트 2 개 (오늘/이틀 전) — 활동은 스크립트 실행 자체가 생성 | HOME 히트맵 카드 / /worklog 상세 (일/주 뷰, 노트, 타임라인) |
| 11 | **DEV-306** 백업 스냅샷 1개 | 설정 > 백업 목록/복원 UI |

## 갱신 절차 — 신규 기능 추가 시

본 스크립트는 **신규 사용자 노출 기능 (GUI / CLI) 이 release 되면 매번 sync**
해야 진정한 회귀 검증 수단. 새 기능 추가 시:

1. **CLI 명령 확인** — 그 기능을 데이터로 만들 CLI 명령이 존재하는가?
   - 있으면 스크립트 단계 추가 (예: DEV-076 의 `quest due`, DEV-016 의 `rules
     create`).
   - 없으면 **CLI 추가가 선행** (별도 quest). 댓글 (DEV-094) / 태그 (DEV-068)
     처럼 GUI / HTTP only 인 기능은 일단 보류 (또는 file 직접 작성 — 단 그
     경우 `openguild reindex` 도 같이 호출해야 함).
2. **스크립트 단계 번호 갱신** — 단계 수가 늘어나면 모든 `[N/M]` 헤더 일관 갱신.
3. **본 규칙 (이 파일) 의 "검증 단계" 표 갱신** — 새 행 추가.
4. **DEV-075 quest 본문에 어떤 fix 가 들어갔는지 짧게 기록** (또는 본 규칙의
   "최근 변경" 섹션 — quest 본문은 1회성이라 영구 기록엔 부적합).

## 안전장치

- `.guild` 가 이미 있는 폴더에서 거부 (실수 덮어쓰기 방지).
- 각 명령 실패 시 즉시 throw + 0 아닌 exit code (`spawnSync` 의 `status` 체크).

## 한계 / 비검증

스크립트가 다루지 **않는** 기능 — 별도 절차 또는 수동 확인:

- **댓글 답글의 본격 threading** — top + 1 reply 만 주입. 다단 / 답글의 답글
  flatten 동작은 수동 확인.
- **태그 (DEV-068)** — open 상태 / CLI 없음.
- **첨부파일 — 이미지/동영상 외 임의 파일 (DEV-237)** — open 상태 / 기능
  미구현 (도서관 문서에 이미지/동영상은 이미 됨, 그 외 파일 타입만 미구현).
- **도서관 검색 (DEV-238)** — open 상태 / 기능 미구현.
- **외부 편집 후 자동 reindex (BUG-049)** — GUI 가 Store::open 직후 자동
  `drift::auto_resync`. 본 스크립트 후엔 별도 호출 불필요.
- **schema ahead banner (BUG-041)** — 같은 binary 가 만든 DB 라 ahead 안 됨;
  수동 시뮬레이션 필요 (`_sqlx_migrations` 에 fake row INSERT).
- **updater (BUG-045)** — release 가 있어야 의미; production binary 만 동작.
- **메모 user_id 격리 (DEV-021)** — single-user 단계 user_id=0 sentinel만
  검증. multi-user JWT 진입 시 별도 시드 필요.

## 최근 변경

- 2026-08-06 (DEV-320): PowerShell → Node 재작성(`seed-test-data.ps1` →
  `.mjs`). 실행법을 `node ...` 로 갱신, 이전에 문서에만 있고 실제로는
  구현된 적 없던 `OPENGUILD_BIN`/release-debug 우선순위 자동탐색 설명을
  제거(실제 동작 = 위치 인자로 바이너리 폴더 지정 또는 PATH) — 문서와 실제
  스크립트가 어긋나 있었다. 단계 수 표기를 10 → 11 로 정정(백업 스냅샷
  단계가 표에서 누락돼 있었음), 6/6b 단계 설명도 실제 스크립트에 맞게 보강.
- 2026-07-07 (DEV-239): 도서관 폴더 기능 — 단계 9 에 폴더 1개(아키텍처) +
  그 안 문서 1개(BOOK-003) 추가. 표 갱신, "첨부파일(DEV-097)" 한계 항목을
  DEV-237(임의 파일 첨부 — 이미지/동영상은 이미 지원)로 정정, DEV-238(검색)
  한계 항목 추가.
- 2026-06-03: DEV-076 due date / DEV-016 multi-file rules 단계 추가.
  스크립트 단계 5 → 7 로 확장. 본 규칙 신설 (DEV-075 quest 본문은 그대로).
- 2026-06-05 (DEV-104): DEV-099 의 댓글/메모 CLI 단계 추가 (단계 7) →
  스크립트 7 → 8 로 확장. DEV-102 의 DB 캐시 sync 도 함께 검증 (snapshot
  안 살아남음). 한계 절 stale 항목 (댓글/메모 "CLI 없음") 제거.
