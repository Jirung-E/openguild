+++
created_at = "2026-07-11T21:26:35+09:00"
updated_at = "2026-07-11T21:26:35+09:00"
+++
# CLI 목록형 명령의 공통 옵션셋

## 규칙

목록을 출력하는 CLI 명령(`quest list`, `campaign list`, `type list`,
`status list`, `library list`, `quest deleted` 등)은 다음 옵션을 **모두**
지원한다:

- `--json` (전역) — agent 파싱용. `--compact` 로 한 줄 JSON.
- `--table` — 사람용 정렬 표. `--json` 과 상호배타.

## 구현 규약

- 표 렌더는 `render_table()` 공용 헬퍼를 쓴다 — 명령별 복붙 금지.
  - **한글 등 가변폭 문자가 올 수 있는 텍스트(제목/설명)는 마지막 컬럼에**
    — 마지막 컬럼은 패딩하지 않으므로 폭 계산이 깨지지 않는다.
  - 색은 패딩 후 `colorize()` (ANSI 코드가 폭 계산에 안 섞이게).
- JSON 직렬화는 `json_str()` 공용 헬퍼 — `--compact` 가 자동 적용된다.

## 강제 장치

clap 으로 컴파일 타임 강제는 불가(명령마다 인자 struct 가 별개) — 대신
`cli/src/main.rs` 의 `all_list_commands_accept_table_flag` 테스트가 알려진
목록형 명령 전수에 대해 `--table` 파싱을 검사한다. **새 목록형 명령을
추가하면 그 테스트의 LIST_COMMANDS 목록에도 추가할 것** — 빠뜨리면 코드
리뷰 체크리스트(이 규칙)에서 걸린다.
