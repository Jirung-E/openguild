---
name: openguild-workflow
description: openguild 저장소에서 agent 가 퀘스트 기반으로 작업할 때의 워크플로 규칙·명령 패턴·함정 요약. 세션 시작 시, 또는 퀘스트 생성/상태 변경/댓글/커밋 작업 전에 참조.
---

# openguild agent 워크플로

> 상세 규범: [AGENTS.md](../../../AGENTS.md) (절대 규칙) ·
> [docs/AGENTS_OPENGUILD_USAGE.md](../../../docs/AGENTS_OPENGUILD_USAGE.md) (CLI 전체 카탈로그).
> 이 스킬은 요약 + 함정 모음 — 충돌 시 AGENTS.md 가 우선.

## 세션 시작 루틴

```bash
openguild quest list --sort updated --reverse --limit 10   # 최근 움직인 퀘스트
openguild quest list --status testing,returned             # 검증 대기/반려
openguild comments --author admin --limit 10               # 최근 사용자 피드백 (DEV-221)
openguild comments --unresolved                            # 미해결 토론 전체
```

`comments` 기본 출력은 본문 **전체**(DEV-230, 요약만 보고 뒷줄 놓쳐 오답한
사고 이후 변경) — 여러 건 훑어볼 땐 `--summary` 로 첫 줄만.

## 퀘스트 수명주기 (필수 순서)

1. **착수**: `git checkout develop && git checkout -b {QUEST_ID}` (브랜치명 = quest_id 그대로) → `openguild quest start {ID}`
2. **구현** → 수동 검증 필요하면 본문에 **"## 테스트 방법"** 추가 후 `openguild quest move {ID} testing`. 자동 테스트로 충분하면 통과 확인 후 done 가능.
3. **사용자 검증 후**: `openguild quest done {ID}`
4. **커밋은 사용자가 명시 지시할 때만.** 형식:
   ```
   [{QUEST_ID}][{scope}] 한 줄 요약

   본문 (왜 중심)

   Co-Authored-By: {지금 작업 중인 에이전트 자신의 이름/모델} <noreply@anthropic.com>
   ```
   (마지막 줄은 고정 문자열이 아니라 지금 이 작업을 하고 있는 에이전트가
   자기 자신의 이름/모델로 채워 넣는다 — 어떤 에이전트가 실행하든 동일하게
   적용되어야 하므로 특정 브랜드를 하드코딩하지 않는다.)
   conventional-commits 금지. 퀘스트별 브랜치 커밋 → develop 에 `git merge --ff-only`. 머지된 브랜치 삭제 금지.

## 절대 금지

- **커밋/푸시/빌드/테스트 명령을 시키지 않았는데 실행** (cargo build/test, npm test 포함 — 수정 후 자동 검증 금지, 검증 명령만 안내)
- **develop 직접 커밋** (quest 없는 메타 변경도 chore 브랜치 경유)
- **`.guild` frontmatter(status/urgency/parent/prereq/deleted) 직접 편집** — 반드시 CLI. 본문(description)만 직접 편집 가능(직후 `openguild reindex`)
- **미커밋 변경 있는 채 `git reset --hard`** (실사고 이력 — .guild 는 tracked 인데 보통 미커밋)
- 상태 변경에 deprecated `quest status <id> <status>` — **`quest move`** 사용

## 함정 (실사고 기반)

| 함정 | 대응 |
|------|------|
| 한글을 exe stdin/인라인 인자로 → 깨짐 | **UTF-8 파일 경유** — 댓글 `--file`, 본문 `--description-file` (DEV-222) |
| `--description` 값이 `--` 로 시작 → clap 이 플래그 오인 | `--description-file` 사용 (또는 `--description=...` 등호 형식) |
| git checkout/pull 후 CLI 실행 → **전체 quest updated_at 오탐 변조** (BUG-103 미수정) | 브랜치 전환 직후 `git status` 로 .guild 대량 diff 확인, 오염 시 1+1 diff 검증 후 `git restore` + `reindex` |
| checkout 이 미커밋 .guild 와 충돌해 Abort → 그대로 커밋하면 develop 직접 커밋 사고 | checkout 후 **반드시 `git branch --show-current` 확인** |
| 댓글 작성자 누락 | 항상 `--author {에이전트 자신의 이름, 소문자}` |
| `cargo fmt`(인자 무시하고 **워크스페이스 전체** 포맷) → repo 가 fmt-clean 이 아니라 50파일 노이즈 diff (2회 실사고) | 단일 파일은 `rustfmt --edition 2024 <파일>` 만 사용, `cargo fmt` 금지 |
| 파괴적 restore | `restore --at` 은 journal truncate(비가역) — 스크래치 길드에서만 실험 |
| CLI top-level 이름 헷갈림 — `type`/`status` 는 `types`/`statuses` alias 가 **있음**, `rule` 은 `rules`/`create` alias 가 **없음**(DEV-231/232, 사용자가 케이스별로 결정) | 예전 예시나 스크립트에 `rules ...`/`rule create ...` 가 있으면 깨짐 — `rule ...`/`rule new ...` 로 |
| `comments`(전역 검색) 요약 60자만 보고 답글 달았다가 뒷줄 놓침(실사고, BUG-105) | DEV-230 이후 기본이 본문 전체로 바뀜 — 그래도 여러 건 훑을 땐 뒷줄까지 있는지 항상 의심 |
| `echo "한글" \| openguild ...` 파이프 → PowerShell 콘솔 인코딩에 따라 깨짐 (comment/memo/rule 전부 같은 read_content 헬퍼 사용) | `--file <UTF8파일>` 로 넘기기 — 해당 명령들 `--help` 에도 이제 이 안내가 있음(DEV-232) |

## 자주 쓰는 패턴

```bash
# 퀘스트 생성 (본문이 - 로 시작하면 = 형식)
openguild quest new --type DEV --urgency 3 --title "..." "--description=..."
# 댓글 (한글은 반드시 파일 경유)
openguild quest comment add {ID} --author {자기 이름} --file /path/utf8.md
openguild quest comment add {ID} --author {자기 이름} --parent-id N --file ...   # 답글
# 계획/설계는 본문 확정사항, 논의/보고는 댓글 — 사용자 피드백엔 답글(parent-id)로
openguild campaign list && openguild campaign show C-XXX   # 현재 마일스톤 파악
openguild rule show restore-behavior                       # restore 동작 정리 문서 (DEV-231: `rules` alias 제거됨 — `rule` 만)
```
