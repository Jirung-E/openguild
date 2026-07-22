+++
book_id = "BOOK-001"
title = "index.db 는 파일의 일방향·폐기가능 투영이다 (아키텍처 불변식)"
path = ""
created_at = "2026-07-22T22:30:00+09:00"
updated_at = "2026-07-22T22:58:06+09:00"
deleted = false
+++

# index.db 는 파일의 일방향·폐기가능 투영이다

## 불변식 (핵심)

`.guild/` 의 **git-tracked 파일이 유일한 진리원**이다. `.guild/index.db` 는
그 파일들의 **일방향·폐기가능 투영(projection)** 이다:

1. **파일 → DB 만 있고, DB → 파일 역류는 없다.**
2. 밑의 파일이 바뀌면(=**브랜치 전환 포함**) 투영을 다시 만든다.
   `rm .guild/index.db && openguild reindex` 는 회피책이 아니라 불변식의 일부.
3. index.db 는 언제든 삭제해도 파일에서 100% 재구축된다(gitignored).

저장 클래스는 사실 **셋**이다 — 혼동 금지:
- **tracked 진리**: `.guild/quests|campaigns|types|statuses|rules|library|
  tags|history/*` — git 이 관리, 브랜치를 따라감.
- **폐기가능 캐시**: `.guild/index.db` — 파생물, 브랜치를 안 따라감 → 반드시
  파일에 종속(위 불변식).
- **로컬 UI 상태**: `.guild/positions.json` — tracked 도 캐시도 아님. 보드
  좌표 등 개인 로컬. gitignored, 절대 권위 아님.

## 왜 (문제의 근원)

index.db 는 (a) git 이 관리 안 하는 working-dir 상태라 자기가 지금 어느
브랜치 파일을 반영하는지 모르고(branch-blind), (b) tracked 파일을 되쓸 수
있었다. **이 둘이 곱해질 때만** 사고가 난다 — 어긋난 캐시가 파일을 덮어써
git 진리로 전파. 둘 중 하나만 없애도 문제는 사라진다. git 진리원은 고정
요구(GitHub 동기화·브랜치별 확인)이고 DB 는 성능상 필요하므로, 답은
**DB 를 진리에 완전히 종속**시키는 것뿐.

"DB 를 쓰면서 git 까지 써서" 가 아니라, **DB 를 진리처럼 취급해서** 가 죄다.

## DB→파일 역류 지도 (제거/게이트 대상)

| # | 경로 | 위치 | 트리거 | 방어하던 속성 | 판정 방향 |
|---|------|------|--------|---------------|-----------|
| A1 | history 사이드카 export | reindex.rs `history export` | 사이드카 없음 | DEV-180 이전 이력 보존(일회성) | 현재 브랜치 실존 퀘스트로 **게이트** (유령 부활 차단, 레거시 보존 유지) |
| A2 | counter write-back | reindex.rs `counter self-heal`(DEV-242) | 파일 카운터 < DB | 파일 카운터 신선도 | **파일-로컬 heal 로 교체**(디스크 실존 최대번호 기준). DB→파일 역류 제거. 다음 ID = `max(파일 카운터, 실존 최대번호)+1` |
| A3 | attachment blob 복원 | ops/attachments.rs `sync_attachment_blobs`(DEV-069) | 참조 첨부 파일 없음 | 실수 삭제 복구 | 유지하되 명시적/게이트 검토 (자동 복원이 브랜치 전환과 충돌하는지) |
| A4 | auto-block 재작성 | reindex.rs 7단계 | — | — | 현재 **비활성**(no-op). 재활성 금지 또는 파일-멱등만 |
| B1 | `updated_at` write-back | incremental.rs 외부편집 동기화 | 파일 mtime > cached | 외부 `.md` 편집 시각 정확도 | 내용-비교 가드 **강화 또는 폐지**. BUG-145(브랜치 checkout mtime 변조 → 무관 퀘스트 일괄 오염)의 근원 |

명시적/정당 복원은 역류 아님: 스냅샷·journal replay(사용자 호출), 일반
mutation 의 파일 쓰기(파일=진리 정상 경로).

## 운영 규칙

- **퀘스트/캠페인/규칙/도서관 등 `.guild` 변경은 develop(추적 브랜치)에서만.**
- 브랜치 전환 후에는 `rm .guild/index.db && openguild reindex` 로 투영 재생성.

### release 브랜치와 `.guild` (2026-07-22 결정)

기존 규칙 `release-process` 는 **별도 release 브랜치 없이** develop→master
FF merge + tag 모델이다. **0.4.1 은 이번 한정 예외** — `release-0.4.1` 브랜치를
쓰되 **버그수정만** 담는다. 위 역류 4경로 수정(A1/A2/A3/B1)을 포함한 이번
개선은 0.4.1 에 넣지 않고 **0.5.0(develop)** 에서 적용한다. 상시 release
브랜치 채택 여부(정식 모델)는 0.5.0 시점에 `release-process` 와 함께 재확정.

예외 브랜치에서도 운영 규칙은 그대로 — `.guild` 변경은 develop 에서만,
브랜치 전환 시 `rm .guild/index.db && reindex`. release-0.4.1 에 `.guild` 를
가져와야 하면 piecemeal 체리픽 말고 develop 것을 통째 동기화.

## 관련

- 파일 배치(flat vs 퀘스트별 폴더)는 **별개** 구조 질문 → DEV-225 에서 논의.
- 위 A1/A2/A3/B1 각 수정은 이 불변식의 **구현**으로 개별 퀘스트화.
