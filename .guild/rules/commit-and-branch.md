+++
created_at = "2026-09-06T19:41:16+09:00"
updated_at = "2026-09-06T19:41:16+09:00"
+++
# 커밋 & 브랜치

`docs/guild-rules.md` 에 있던 것을 옮겼다([[DEV-371]]) — 규칙은 길드가
정본이다. 옮기면서 낡은 내용은 현행에 맞췄다.

## 권한

커밋 / push 는 **사용자가 명시적으로 요청할 때만** 실행한다. amend / reset /
force push 도 명시 요청이 필요하다.

**예외 하나**: 퀘스트를 In Progress → Testing 으로 옮길 때는 바로 커밋한다.
그 외 모든 커밋은 물어본다.

## 브랜치 전략

```
master       ─── 릴리즈 전용 (태그 v0.x.y)
  ↑ release merge only
develop      ─── 통합 / 검증 단계
  ↑ feature merge
quest/DEV-001, quest/BUG-045, ...  ─── 작업 브랜치 (develop 기반)
```

- **master** — 릴리즈 전용. 직접 commit / push 금지. develop 에서만 받는다.
- **develop** — 통합 분기. 일상 작업의 기준.
- **작업 브랜치** — `quest/{QUEST_ID}`. 시작은 develop 최신에서.

```bash
git checkout develop && git pull && git checkout -b quest/DEV-001
```

## 커밋 메시지

```
[{QUEST_ID}][{CATEGORY?}] 요약 한 줄

본문 — 무엇이 아니라 **왜**. diff 가 what 은 이미 보여준다.
```

- `[QUEST_ID]` 필수. 브랜치의 quest_id 와 같아야 한다.
- `[CATEGORY]` 선택 — `gui`, `core`, `cli`, `server`, `skill`, `docs`, `chore`.
- 요약은 70자 이내.
- **한 커밋에 다른 퀘스트 변경을 섞지 않는다**([[BUG-016]]). 다른 퀘스트 파일이
  stage 됐으면 `git reset HEAD <path>` 로 뺀다.
- AI agent 가 쓴 커밋은 `Co-Authored-By` trailer 를 붙인다. **모델 이름은
  그때그때 다르므로 세션이 지정한 값을 쓴다** — 여기 특정 버전을 박아 두면
  낡는다(예전에 `Opus 4.7` 로 박혀 있었다).

## 머지

- 기본은 `git merge --ff-only quest/{QUEST_ID}` — develop 의 히스토리를 선형으로
  유지한다.
- `--no-ff` 강제 금지.
- develop 이 앞서 갔으면 작업 브랜치를 rebase 한 뒤 FF 머지.

## 릴리즈

`release-process` 규칙을 따른다 — 버전 동기화 목록, CHANGELOG 형식, 태그 절차,
릴리스 후 확인까지 거기에 있다.
