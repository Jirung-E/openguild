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

## 퀘스트 수명주기 (필수 순서)

1. **착수**: `git checkout develop && git checkout -b {QUEST_ID}` (브랜치명 = quest_id 그대로) → `openguild quest start {ID}`
2. **구현** → 수동 검증 필요하면 본문에 **"## 테스트 방법"** 추가 후 `openguild quest move {ID} testing`. 자동 테스트로 충분하면 통과 확인 후 done 가능.
3. **사용자 검증 후**: `openguild quest done {ID}`
4. **커밋은 사용자가 명시 지시할 때만.** 형식:
   ```
   [{QUEST_ID}][{scope}] 한 줄 요약

   본문 (왜 중심)

   Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
   ```
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
| 댓글 작성자 누락 | 항상 `--author claude` (소문자) |
| 파괴적 restore | `restore --at` 은 journal truncate(비가역) — 스크래치 길드에서만 실험 |

## 자주 쓰는 패턴

```bash
# 퀘스트 생성 (본문이 - 로 시작하면 = 형식)
openguild quest new --type DEV --urgency 3 --title "..." "--description=..."
# 댓글 (한글은 반드시 파일 경유)
openguild quest comment add {ID} --author claude --file /path/utf8.md
openguild quest comment add {ID} --author claude --parent-id N --file ...   # 답글
# 계획/설계는 본문 확정사항, 논의/보고는 댓글 — 사용자 피드백엔 답글(parent-id)로
openguild campaign list && openguild campaign show C-XXX   # 현재 마일스톤 파악
openguild rules show restore-behavior                      # restore 동작 정리 문서
```
