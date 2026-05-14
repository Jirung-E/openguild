# 제안: CLI / Guild 연결 / 프론트엔드 통합

> 사용자 검토 후 주석을 달아 결정사항 / 변경사항을 표기. 결정된 항목은 진행 순서에 반영하여 실제 작업 시작.

검토용 항목 목록:

1. CLI 로 어떤 길드(프로젝트)에 연결할지 — 로컬 / 원격 둘 다
2. 로컬 실행 시 backend + frontend 따로 띄우는 문제
3. 사용자 / agent CLI 사용 가이드 작성 + 문서 정리
4. `openguild init` 명령 구현 (b 방식)
5. `openguild open` — CLI 로 프론트엔드 열기 (로컬 / 원격)

---

## 1. CLI 길드 연결 (로컬 / 원격)

### 현재
- CLI 는 서버 URL 만 지정 가능 (`--url <URL>` / `OPENGUILD_URL`)
- "어떤 길드" 는 서버가 시작될 때 결정 (백엔드의 `GUILD_PATH` 환경변수)
- CLI 자체에서 길드 path 를 직접 다루지 않음

### 원하는 동작
```bash
openguild --guild ./monitor quest list      # 로컬 길드 (path)
openguild --url http://example.com:3000 quest list  # 원격 서버
```

### 구현 옵션

| | 방식 | 장점 | 단점 |
|---|---|---|---|
| **a** | CLI 가 로컬 SQLite 직접 열기 | 서버 안 띄워도 됨 | sqlx 의존성 추가 / 서버 코드 일부 복제 / 동시성 문제 (서버도 떠있으면 file lock) |
| **b** | `--guild <path>` 시 CLI 가 백그라운드 로컬 서버 자동 spawn (lock 파일로 재사용) | transparent UX | 구현 복잡 (lifecycle, 포트 할당, 종료 처리) |
| **c** | `--guild <path>` 는 단지 "이 path 의 길드와 연결된 서버를 사전에 띄워두라" 는 안내 의미. CLI 는 url 로만 통신 | 가장 단순. 인터페이스만 잡음 | 사용자 책임 ↑ — 직접 서버 띄워야 |

### 추천 단계
- **1차: (c) 단순 도입** — 인터페이스만 잡고 사용자가 서버 띄움
- **2차: (b) transparent** — 본격 운영 단계에 lock 파일 + auto-spawn

> 💬 (사용자 주석란)
> - '원하는 동작' 에서, 로컬 길드 연결시 `--guild` 대신 `--local`이 더 나을거같음.
> - 또한, 명령을 실행한 위치가 길드 디렉토리일때 별도 위치 옵션을 주지 않으면 자동으로 현재 디렉토리에 있는 길드에 대해 명령이 실행되어야함
> - 로컬 길드에 대해 명령을 실행할때 서버가 안떠있는경우 띄워줘야함
> - 근데 이렇게 되면 매번 명령을 실행할때마다 서버를 다시 띄워야 할수도 있는데 문제 없는지?
> - 실행인자로 옵션과 인자를 넘기는 방식 대신 `openguild` 명령을 사용하면 cli 프로그램이 실행되고 거기에서 명령과 옵션을 입력받는 방식은 어떨지?(예: `openguild .`(현재 디렉토리에 있는 길드에 연결) -> `> quest list`(명령 입력) 이런식으로)
> - 이런식으로 했을때 ai agent가 지속적으로 입력이 가능한지?

---

## 2. backend + frontend 동시 실행

### 현재 dev
- 두 터미널 필요: `cargo run -p server` / `npm run dev`
- 사용자가 직접 둘 다 띄움

### 구현 옵션

| | 방식 | 시점 |
|---|---|---|
| **A** | **백엔드가 frontend static 서빙** — `npm run build` → `frontend/dist/` → axum `tower-http::services::ServeDir` 로 서빙. 단일 프로세스 단일 포트 (예: 3000) | 배포 / 운영 |
| **B** | **dev 통합 script** — `npm run dev:all` 이 `concurrently` 로 둘 다 spawn | 개발 |
| **C** | **Tauri / Wails 같은 데스크톱 wrapper** | 데스크톱 앱 (planning.md 의도) |

### 추천
- **(A) 운영 모드 통합** 우선 도입
- dev 시엔 (B) 가 편할 수 있지만 우선순위 낮음
- (C) 는 별도 단계 (큰 작업)

### 구현 (A)
```toml
# backend/server/Cargo.toml
tower-http = { ..., features = ["cors", "trace", "fs"] }
```

```rust
// main.rs
use tower_http::services::ServeDir;
let app = routes::create_router(pool)
    .layer(...)
    .fallback_service(ServeDir::new("frontend/dist"));
```

서버 시작 시 `frontend/dist` 존재 확인, 없으면 경고만 (API 는 그대로 동작).

> 💬 (사용자 주석란)
> A로 했을때 현재 구조에 영향이 없다면 A로 진행. 아니라면 나중에 C로 진행

---

## 3. 문서 정리 — CLI 사용 가이드

### 현재 문제
- `AGENTS.md` 가 "프로젝트 컨텍스트" (개발자 / agent 가 코드 수정용 참조) 와 "사용 가이드" (사용자 / agent 가 CLI 로 OpenGuild 를 사용) 가 섞여있음
- 제목이 "OpenGuild" 만으로 무엇을 위한 문서인지 불명확

### 정리 계획

| 파일 | 역할 (변경 후) | 비고 |
|---|---|---|
| `AGENTS.md` | **OpenGuild 개발 컨텍스트** — agent / 개발자가 코드를 수정할 때 참조 | 제목 명확화 / 짧게 유지 |
| `docs/usage.md` (신규) | **사용 가이드** — 사용자 / agent 가 CLI 로 OpenGuild 를 사용 | 신규 작성 |
| `README.md` | 두 문서 위치를 명시 | 보강 |

### `AGENTS.md` 제목 후보
- "OpenGuild — 개발 컨텍스트"
- "OpenGuild — 코드베이스 가이드"
- "OpenGuild — Contributing / Development Notes"

### `docs/usage.md` 목차 (안)
1. 셋업
   - 서버 띄우기
   - 길드 init
   - 환경변수 (`OPENGUILD_URL`)
2. 명령어 카탈로그
   - 글로벌 옵션 (`--url`, `--guild`, `--json`)
   - 길드 명령 (`init`, `open`)
   - 퀘스트 명령 (`new`, `list`, `show`, `start/done/reopen`, `update`, `delete`, `restore`, `parent`, `prereq`, `deleted`)
   - 메타 (`types`, `statuses`, `ping`)
3. agent 워크플로 패턴
   - 작업 시작 / 진행 / 완료
   - 서브 작업 분리 / 선행 추가
   - JSON 출력 캡처해서 후속 호출에 사용
4. 안전장치
   - `--yes` / `--dry-run`
   - Soft delete + restore
   - 자동 백업 / audit log
5. 에러 처리
   - exit code 1, stderr 메시지
   - 서버 다운 시 `ping` 으로 사전 체크
6. 예시 시나리오
   - 새 기능 구현 시작 → 진행 → 완료
   - 버그 발견 → BUG quest → 수정 → done
   - 큰 작업 분할 → parent + sub-quest

> 💬 (사용자 주석란 — 다른 위치 / 이름 / 내용 추가 사항?)
> - AGENTS.md는 ai agent가 읽는거임. 이름 바꾸면 안됨. ai agent가 필요로 하는 정보(설명, 문서파일 등)의 위치(경로, 주소 등)를 적어두는 역할을 하면 될거같음(책의 목차처럼). 위치를 따라가면 자세한 내용을 알 수 있도록. 컨텍스트를 아낄 수 있도록 AGENTS.md에는 최소한의 정보만 담는게 좋을거같다.
> - `docs/usage.md` 보다는 `docs/AGENTS_OPENGUILD_USAGE.md` 로 하는게 나을듯
> - README.md는 사용자용이므로 현재 역할을 유지하면 되는데, 사용자에게 필요한 문서 설명을 추가해야함(예: docs/AGENTS_OPENGUILD_USAGE.md에 ai agent가 openguild를 사용하기 위한 가이드가 담겨있다고 설명). 
> - 이 부분은 니가 제대로 이해를 못한거같으니까 다시 읽어보고 나한테 확인 받아

---

## 4. `openguild init` — (b) 방식

### 동작
```bash
openguild init ./monitor                     # 길드명은 path 의 마지막 segment ("monitor")
openguild init ./monitor --name "모니터"      # 길드명 지정
```

### 처리
1. `./monitor/` 디렉토리 생성 (이미 있으면 그대로)
2. `./monitor/<name>.guild` TOML 파일 생성:
   ```toml
   name = "모니터"
   version = "1.0"
   created_at = "2026-05-10"
   ```
3. 안내 출력:
   ```
   ✓ guild created: ./monitor/모니터.guild
   ▸ start the server:  GUILD_PATH=./monitor cargo run -p server
   ```
4. 서버 첫 실행 시 sqlx 가 자동으로 `guild.db` + 마이그레이션 적용 (현재 동작 그대로)

### 구현
- CLI main.rs 에 `Init { path, name }` subcommand 추가
- TOML 작성: `toml` crate 추가 또는 수동 string format (단순 키-값이라 manual 도 OK)
- `--name` 미지정 시 `path.file_name()` 사용

> 💬 (사용자 주석란)
> `openguild init` 까지만 치고 현재 디렉토리를 길드로 초기화하도록. 굳이 path 인자로 디렉토리를 받을 필요는 없을듯. 옵션은 `--name` 정도로 충분할듯

---

## 5. `openguild open <path | --url>` — 프론트엔드 열기

### 동작
```bash
openguild open ./monitor                  # 로컬 — 서버 띄우고 브라우저 열기
openguild open --url http://other.com     # 원격 — 브라우저만 열기 (서버 떠있다고 가정)
```

### 구현 옵션

| | 방식 | 노력 | 비고 |
|---|---|---|---|
| **A** | 로컬: production 모드 — backend 가 frontend 까지 서빙. CLI 는 backend 만 spawn | 항목 2A 가 선행되어야 | 가장 단순 |
| **B** | 로컬: dev 모드 — backend + vite 둘 다 spawn. HMR 가능 | 두 프로세스 lifecycle 관리 필요 | 개발 편의 |

### 추천
- **(A)** — 일반 사용자 / agent 용. 단일 명령 단일 프로세스
- 개발자는 기존처럼 cargo / npm 별도 사용

### 동작 (A 기준)
1. `./monitor/<name>.guild` 존재 확인 (없으면 에러: "openguild init 먼저")
2. 백엔드 binary spawn — `GUILD_PATH=./monitor` 으로 (또는 `cargo run -p server` dev 시)
   - 포트는 환경변수 또는 빈 포트 자동 할당
3. health check (`/health` 까지 polling) 으로 준비 확인
4. `webbrowser::open("http://localhost:<port>")` 으로 브라우저 열기
5. 사용자가 Ctrl+C 시 백엔드도 함께 종료

`--url` 모드는 단순 — 그 url 그대로 브라우저 open.

### 의존성
- `webbrowser` crate (또는 OS 별 직접 호출)
- backend binary 위치 — 빌드된 `backend/target/release/server` 또는 cargo 호출

### 미결
- backend binary 없는 상태에서 `openguild open` 시 자동 빌드? 또는 안내만?
- 포트 충돌 시 처리

> 💬 (사용자 주석란)
> - 로컬을 위한 서버가 떠있을때/떠있지 않을때 동작이 달라질것인데, 포트 연결을 어떻게 할지 고민이 된다.
> - 현재 구조로는 서버가 안떠있으면 백엔드 실행시 가능한 포트를 잡고, 그 포트를 프론트엔드가 알아내야하는데 이게 괜찮은 방법일지?
> - 서버가 떠있으면 떠있는 서버의 포트를 알아내야한다.
> - 무언가 브릿지가 필요하다.
> - 사실 로컬 길드에 대해서는 통신 없이 로컬로 처리하는게 제일 좋을거같은데, 이렇게 되면 백엔드와 프론트엔드의 경계가 무너져버린다..

---

## 진행 순서 제안

| 순서 | 항목 | 의존성 | 노력 | 추천 강도 |
|---|---|---|---|---|
| **1** | 문서 정리 (#3) — AGENTS.md 제목/구조, `docs/usage.md` 작성, README 표시 | — | 30~60분 | ⭐⭐⭐ |
| **2** | `openguild init` (#4) | — | 30~60분 | ⭐⭐ |
| **3** | 백엔드가 frontend static 서빙 (#2A) | `npm run build` 결과 가정 | 30~60분 | ⭐⭐ |
| **4** | `openguild open` (#5) | #3 위에 build | 1~2시간 | ⭐⭐ |
| **5** | CLI `--guild <path>` 옵션 (#1c) — 인터페이스만 | — | 20분 | ⭐ |
| **나중** | transparent local server (#1b) | — | 1일+ | ⭐ (운영 본격화 시) |

> 💬 (사용자 주석란 — 순서 변경 / 우선순위 조정?)
> - 내가 준 답변을 고려해서 순서 다시 선정해

---

## 결정 필요 항목 체크리스트

- [ ] 1. CLI `--guild` 1차 구현 방식: (a) 직접 DB / (b) auto-spawn / **(c) 인터페이스만**
- [ ] 2. frontend 정적 서빙 방식: **(A) 운영 통합** / (B) dev 통합 / (C) 데스크톱 wrapper
- [ ] 3. 사용 가이드 위치: `docs/usage.md` / 다른 이름?
- [ ] 3. AGENTS.md 새 제목: "개발 컨텍스트" / "코드베이스 가이드" / 다른 안?
- [ ] 4. `openguild init` 의 `--name` 기본값: path 마지막 segment
- [ ] 5. `openguild open` 모드: **(A) production** / (B) dev / 둘 다 옵션
- [ ] 5. backend binary 없을 때 처리: 자동 빌드 / 안내만 / `cargo run` 호출

---

## 작업 외 잡일 (참고)

- `.gitattributes` 추가 (LF/CRLF 경고 제거) — 1줄, 5분
- 백업 보관 기간 / 압축 — 운영 본격화 시 재검토
