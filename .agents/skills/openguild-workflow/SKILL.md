---
name: openguild-workflow
description: openguild 저장소에서 agent 가 퀘스트 기반으로 작업할 때의 워크플로 규칙·명령 패턴·함정 요약. 세션 시작 시, 또는 퀘스트 생성/상태 변경/댓글/커밋 작업 전에 참조.
---

# openguild agent 워크플로

> 상세 규범: [AGENTS.md](../../../AGENTS.md) (절대 규칙) ·
> [skills/openguild-plugin/skills/openguild/reference/](../../../skills/openguild-plugin/skills/openguild/reference/)
> (CLI 전체 카탈로그 — 배포용 `openguild` 스킬의 주제별 레퍼런스(quest/
> comments/library/… 파일로 분할됨), 이 repo 개발과 무관하게 CLI 사용법
> 자체는 동일).
> 이 스킬은 요약 + 함정 모음 — 충돌 시 AGENTS.md 가 우선.

## ⚠️ 코드 보기 전에 — 도서관(library) 먼저

**설계/아키텍처/구조와 관련된 작업을 시작하기 전에 반드시 `openguild library
list` 로 도서관 문서(BOOK)를 먼저 확인**하고 관련 문서를 읽어라. 도서관엔
코드만 봐서는 안 보이는 **설계 불변식·결정 기록·함정**이 있다(예: BOOK-001
"index.db 는 파일의 일방향 투영" 불변식). 코드부터 파고들면 도서관에만 있는
결정을 어기게 된다 — **실사고 있음.** 순서: `library list` → 관련 BOOK
`library show` → 그 다음에 코드.

```bash
openguild library list                        # 설계·불변식 문서(BOOK) — 코드 읽기 전에 먼저
openguild library show BOOK-1                  # 관련 문서 정독
```

## 세션 시작 루틴

```bash
openguild library list                                     # 설계 문서(BOOK) — 항상 먼저
openguild quest list --sort updated --reverse --limit 10   # 최근 움직인 퀘스트
openguild quest list --status testing,returned             # 검증 대기/반려
openguild comments --author admin --limit 10               # 최근 사용자 피드백 (DEV-221)
openguild comments --unresolved                            # 미해결 토론 전체
```

`comments` 기본 출력은 본문 **전체**(DEV-230, 요약만 보고 뒷줄 놓쳐 오답한
사고 이후 변경) — 여러 건 훑어볼 땐 `--summary` 로 첫 줄만.

## 길드 위치 — 이름만 주어졌을 때

cwd 에 `.guild` 가 없고 사용자가 길드 **이름**만 준 경우, `~/.openguild/
recents.json` 을 읽어 위치를 알아낸다. 이 파일은 GUI/CLI 가 최근 연 길드
목록으로, 배열 `[{ "name": ..., "path": ..., "last_opened": ... }]` 형식.
`name` 이 일치(부분 일치 시 사용자 확인)하는 항목의 `path` 를 찾아
`openguild --guild <path> ...` 로 실행한다.

```bash
# 예: 사용자가 "coco-ai 길드 상태 봐줘" → recents 에서 path 조회 후
openguild --guild <recents 에서 찾은 path> quest list --status testing
```

## 퀘스트 수명주기 (필수 순서)

> **보드는 현실과 일치해야 한다 — 일하기 전과 후에 상태를 옮겨라.**
> - **코드/파일을 한 줄이라도 건드리기 전에 In Progress 로.** 끝내고 나서가
>   아니라 착수 직전. 에이전트가 가장 자주 빼먹는 단계이고, 특히 여러
>   퀘스트를 연달아 처리할 때 앞 퀘스트 흐름에 휩쓸려 건너뛴다 — **퀘스트마다
>   매번** 다시 한다. (실사고로 두 번 지적받음)
> - **구현이 끝나면 Testing 으로.** done 은 사용자 검증 몫(자동 테스트로
>   충분한 경우만 예외 — 아래 3번).
> - 소급으로 가짜 타임스탬프 이력을 만들지 말 것. 빼먹었으면 지금 옮기고 진행.
>
> **예외 — 길드가 상태 이름을 바꿨거나 없앤 경우.** 상태는 길드마다 설정
> 가능하다. 확실치 않으면 `openguild status list` 로 확인하고, In Progress /
> Testing 에 해당하는 상태가 없으면 이 규칙은 적용 대상이 아니다 — 그 길드의
> 대응 단계를 쓰거나 상태를 건드리지 않는다. 이 규칙을 지키려고 상태를 새로
> 만들거나 이름을 바꾸지 말 것.

1. **착수**: `git checkout develop && git checkout -b {QUEST_ID}` (브랜치명 = quest_id 그대로) → `openguild quest start {ID}` (**첫 수정 전에, 착수 당시 상태가 Open 이든 On Hold 든 무관하게 항상**)
2. **구현** → 수동 검증 필요하면 본문에 **"## 테스트 방법"** 추가 후 `openguild quest move {ID} testing`. 자동 테스트로 충분하면 통과 확인 후 done 가능.
3. **Testing 전환 직후 — 묻지 말고 바로 커밋 + develop 에 merge 까지
   (2026-08-08 확정, 유일한 자동 커밋/merge 지점).**
   커밋 형식:
   ```
   [{QUEST_ID}][{scope}] 한 줄 요약

   본문 (왜 중심)

   Co-Authored-By: {지금 작업 중인 에이전트 자신의 이름/모델} <noreply@anthropic.com>
   ```
   (마지막 줄은 고정 문자열이 아니라 지금 이 작업을 하고 있는 에이전트가
   자기 자신의 이름/모델로 채워 넣는다 — 어떤 에이전트가 실행하든 동일하게
   적용되어야 하므로 특정 브랜드를 하드코딩하지 않는다.)
   conventional-commits 금지. 커밋 직후 곧바로 `git checkout develop && git
   merge --ff-only {QUEST_ID}` 도 묻지 않고 실행 — 커밋만 하고 merge 는
   빼먹지 말 것. 머지된 브랜치 삭제 금지.
4. **사용자 검증 후**: `openguild quest done {ID}`. **이 전환 및 그 커밋은
   자동 아님 — 사용자에게 커밋해도 될지 먼저 물어본다** (예외는 3번 하나뿐).
   여러 퀘스트를 모아 한번에 처리하는 경우, chore 성 변경(퀘스트 등록,
   카운터 bump 등) 도 마찬가지로 매번 먼저 물어본다.

## 퀘스트를 새로 만들 때 — **연관을 그 자리에서 건다**

새 퀘스트가 기존 것과 관계가 있으면 **생성 직후 바로** 연관을 건다. 본문에
`[[…]]` 만 적어두는 것으로는 부족하다 — 그건 읽는 사람을 위한 링크일 뿐,
보드의 의존성 그래프 / 트리 / `--has-prereq` 필터 어디에도 안 잡힌다.
(실사고: 강화 검색 후속 3건을 만들면서 본문에만 `[[REQ-009]]` 를 적고 선행
관계를 안 걸어, 보드에서 셋이 떠 있는 것처럼 보였다.)

```bash
openguild quest new --type REQ --title "..." --parent {부모}     # 서브퀘스트로 생성
openguild quest prereq add {새ID} {선행ID}                        # 선행 관계는 생성 후 별도
```

**어느 쪽인지 판단**:

| 관계 | 쓰는 것 |
|------|---------|
| B 를 하려면 A 가 **먼저 끝나야** 한다 | `prereq add B A` |
| B 가 A 의 **일부**다 (쪼갠 것) | `--parent A` |
| 그냥 **참고**하면 좋다 | 본문 `[[A]]` 만 |

`quest new` 에는 `--parent` 만 있고 `--prereq` 는 없다 — 선행은 생성 후 한 줄
더 실행해야 한다. **잊기 쉬우니 생성과 한 묶음으로 처리한다.**

## cross-link `[[…]]` — 보이면 따라가라

퀘스트 본문·댓글·규칙·도서관 문서에서 `[[…]]` 토큰을 만나면 그건 **같은 길드의
다른 문서를 가리키는 포인터**다. 작성자가 "여기서 그 문서가 필요하다"고 붙여둔
것이므로, 이미 내용을 알거나 현재 작업과 명백히 무관한 경우가 아니면 **읽고
나서 판단한다.** 평문으로 넘기거나 ID 만 보고 내용을 추측하지 말 것.

| 토큰 | 대상 | 읽는 명령 |
|------|------|-----------|
| `[[DEV-001]]` · `[[quest:DEV-001]]` | 퀘스트 | `openguild quest show DEV-001` |
| `[[C-001]]` · `[[campaign:C-001]]` | 캠페인 | `openguild campaign show C-001` |
| `[[BOOK-012]]` · `[[library:BOOK-012]]` | 도서관 문서 | `openguild library show BOOK-012` |
| `[[some-rule]]` · `[[rules:some-rule]]` | 규칙 | `openguild rule show some-rule` |

접두 별칭: `quest`/`q`, `campaign`/`c`, `rule`/`rules`/`r`, `book`/`library`/`lib`
(접두 생략 시 ID 형태로 판별). 규칙 slug 는 공백 포함 가능(`[[코딩 규칙]]`,
BUG-156). 대상이 없는 토큰은 GUI 에서 빨간 링크 — 실재 문서를 참조할 의도였다면
ID 를 확인한다.

**쓸 때도** 내용을 복사하지 말고 링크한다 — 도서관 문서 본문을 퀘스트 설명에
붙여넣는 대신 `[[BOOK-012]]` 로 참조하고, 관련 퀘스트끼리 cross-link 를 걸어
다음 사람(또는 에이전트)이 흐름을 따라갈 수 있게 한다.

## 절대 금지

- **커밋/푸시/빌드/테스트 명령을 시키지 않았는데 실행** (cargo build/test, npm test 포함 — 수정 후 자동 검증 금지, 검증 명령만 안내). **유일한 예외**: 퀘스트 수명주기 3번(Testing 전환 직후) 커밋 — 그 외의 커밋과 push/build/test 는 전부 이 금지 그대로 적용.
- **develop 직접 커밋** (quest 없는 메타 변경도 chore 브랜치 경유)
- **`.guild` frontmatter(status/urgency/parent/prereq/deleted) 직접 편집** — 반드시 CLI. 본문(description)만 직접 편집 가능(직후 `openguild reindex`)
- **조회도 CLI 우선 — `.guild/**` 파일을 직접 열람/grep 으로 뒤지지 말 것**
  (사용자 지적, BUG-154): 도서관은 `openguild library list` / `library show
  BOOK-N`, 댓글은 `openguild quest comment list {ID}` / 전역 `comments`,
  퀘스트는 `quest show {ID}`. 파일 직접 조회는 사이드카/캐시 구조를 우회해
  놓치는 정보(반응/토론 상태/정렬)가 생기고, 사용자에게 보이는 뷰와 다른
  결과를 읽게 된다. (허용 예외: CLI 버그 자체를 조사할 때의 대조 확인)
- **미커밋 변경 있는 채 `git reset --hard`** (실사고 이력 — .guild 는 tracked 인데 보통 미커밋)
- 상태 변경에 deprecated `quest status <id> <status>` — **`quest move`** 사용

## 함정 (실사고 기반)

| 함정 | 대응 |
|------|------|
| 한글을 exe stdin/인라인 인자로 → 깨짐 | **UTF-8 파일 경유** — 댓글 `--file`, 본문 `--description-file` (DEV-222) |
| `--description` 값이 `--` 로 시작 → clap 이 플래그 오인 | `--description-file` 사용 (또는 `--description=...` 등호 형식) |
| git checkout/pull 후 CLI 실행 → **전체 quest updated_at 오탐 변조** (BUG-103/BUG-145 — 소스는 수정됐지만 **수정 이전에 빌드된 구 바이너리**(설치본/오래된 target)가 돌면 재발) | 브랜치 전환 직후 `git status` 로 .guild 대량 diff 확인, 오염 시 1+1 diff 검증 후 `git restore` + `reindex`. 대량 변조 재발 시 구버전 GUI/CLI 프로세스부터 의심 |
| checkout 이 미커밋 .guild 와 충돌해 Abort → 그대로 커밋하면 develop 직접 커밋 사고 | checkout 후 **반드시 `git branch --show-current` 확인** |
| 댓글 작성자 누락 | 항상 `--author {에이전트 자신의 이름, 소문자}` |
| `cargo fmt`(인자 무시하고 **워크스페이스 전체** 포맷) → repo 가 fmt-clean 이 아니라 50파일 노이즈 diff (2회 실사고) | 단일 파일은 `rustfmt --edition 2024 <파일>` 만 사용, `cargo fmt` 금지 |
| 파괴적 restore | `restore --at` 은 journal 을 truncate한다. 현재 상태의 자동 pre-backup으로 되돌릴 수 있지만, 검증은 여전히 스크래치 길드에서만 수행 |
| CLI top-level 이름 헷갈림 — `type`/`status` 는 `types`/`statuses` alias 가 **있음**, `rule` 은 `rules`/`create` alias 가 **없음**(DEV-231/232, 사용자가 케이스별로 결정) | 예전 예시나 스크립트에 `rules ...`/`rule create ...` 가 있으면 깨짐 — `rule ...`/`rule new ...` 로 |
| `comments`(전역 검색) 요약 60자만 보고 답글 달았다가 뒷줄 놓침(실사고, BUG-105) | DEV-230 이후 기본이 본문 전체로 바뀜 — 그래도 여러 건 훑을 땐 뒷줄까지 있는지 항상 의심 |
| `echo "한글" \| openguild ...` 파이프 → PowerShell 콘솔 인코딩에 따라 깨짐 (comment/memo/rule 전부 같은 read_content 헬퍼 사용) | `--file <UTF8파일>` 로 넘기기 — 해당 명령들 `--help` 에도 이제 이 안내가 있음(DEV-232) |

## 자주 쓰는 패턴

```bash
# 퀘스트 생성 (본문이 - 로 시작하면 = 형식)
openguild quest new --type DEV --urgency 3 --title "..." "--description=..."
# 댓글 작성 정책 — comment add 는 CLI 문법상 optional 이지만 agent 는 항상
# `--author` 를 전달. reaction 은 CLI 필수, memo 는 author 필드 자체가 없음.
# 한글 본문은 반드시 파일 경유(stdin/인자는 콘솔 인코딩에 깨짐).
openguild quest comment add {ID} --author {자기 이름} --file /path/utf8.md
openguild quest comment add {ID} --author {자기 이름} --parent-id N --file ...   # 답글
# 토론은 **말머리가 아니라 기능**이다 (DEV-361). 본문에 "[토론]" 이라고 적는
# 것은 아무 효과가 없다 — 완료 차단도, `comments --unresolved` 필터도, GUI
# 토론 표시도 전부 discussion 플래그를 본다.
openguild quest comment add {ID} --author {자기 이름} --file ... --discussion   # 토론 댓글
openguild quest memo set {ID} --file /path/utf8.md
openguild quest comment react {ID} {댓글번호} 👍 --author {자기 이름}
# 계획/설계는 본문 확정사항, 논의/보고는 댓글 — 사용자 피드백엔 답글(parent-id)로
openguild campaign list && openguild campaign show C-XXX   # 현재 마일스톤 파악
openguild rule show restore-behavior                       # restore 동작 정리 문서 (DEV-231: `rules` alias 제거됨 — `rule` 만)
# 조회는 파일 grep 이 아니라 CLI 로 (BUG-154)
openguild library list && openguild library show BOOK-3    # 도서관 문서 확인
openguild quest comment list {ID} --tree                   # 특정 퀘스트 댓글 확인
```
