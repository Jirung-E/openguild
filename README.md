# OpenGuild
A project issue tracker.

## Terminology
| Term | Description |
|---|---|
| **Guild** | A single project unit. Each guild has its own quests and settings. |
| **Quest** | An individual issue or task within a guild. |
| **Sub-Quest** | A child task that belongs to a parent quest. |
| **Prerequisite Quest** | A quest that must be completed before another quest can begin. |
| **Campaign** | A planning document that groups quests toward a release or milestone. |
| **Quest Board** | A node-based board where quests are arranged by status in swim lanes. |
| **Quest List** | A list view of all quests, with sub-quests shown as a collapsible tree. |
| **Guild Master** | The administrator of a guild. |
| **Quest Holder** | The person assigned to a quest. |
| **Requester** | The person who created the quest. |
| **Urgency** | Priority level of a quest: Critical / High / Medium / Low. |


## Usage

### Opening a Guild
Each guild is stored as an independent directory. A guild is identified by a `{name}.guild` file inside the directory.

```
# Open an existing guild
openguild.exe ./my-project

# If no .guild file is found, an initialization prompt will appear
```

You can also open a guild from the GUI by selecting a directory or choosing from the recent guilds list.

### Creating a Quest
Quests are created within a guild. Each quest has a type prefix and an auto-incremented ID (e.g., `DEV-001`, `BUG-003`).

Default quest types:
- `DEV` — General development task
- `BUG` — Bug report
- `REQ` — Feature request

### Quest Board
Quests are displayed as nodes arranged in swim lanes by status. Drag a node to a different lane to change its status. Arrows between nodes indicate prerequisite or sub-quest relationships.

### Quest List
All quests are shown in a flat list. Sub-quests appear as a collapsible tree under their parent. Status cannot be changed from this view — use the Quest Board or Quest Detail page.


## Development
**Backend**
```bash
cd backend
cargo run -p server
```

**Frontend**
```bash
cd frontend
npm run dev
```



---



# OpenGuild
프로젝트 이슈 트래커.

## 용어 설명
| 용어 | 설명 |
|---|---|
| **Guild (길드)** | 프로젝트 단위. 각 길드는 독립된 퀘스트와 설정을 가진다. |
| **Quest (퀘스트)** | 길드 내 개별 이슈 또는 작업. |
| **Sub-Quest (서브퀘스트)** | 부모 퀘스트에 속하는 하위 작업. |
| **Prerequisite Quest (선행 퀘스트)** | 특정 퀘스트를 시작하기 전에 완료되어야 하는 퀘스트. |
| **Campaign (캠페인)** | 릴리즈 또는 마일스톤을 향해 퀘스트를 묶는 기획 문서. |
| **Quest Board (의뢰게시판)** | 퀘스트를 상태별 레인으로 배치하는 노드 기반 보드. |
| **Quest List (퀘스트 목록)** | 모든 퀘스트를 리스트로 보여주는 뷰. 서브퀘스트는 접기/펼치기 트리로 표시. |
| **Guild Master (길드마스터)** | 길드 관리자. |
| **Quest Holder (담당자)** | 퀘스트를 담당하는 사람. |
| **Requester (의뢰인)** | 퀘스트를 생성한 사람. |
| **Urgency (긴급도)** | 퀘스트의 우선순위: Critical / High / Medium / Low. |


## 사용 방법

### 길드 열기
각 길드는 독립된 디렉터리로 저장된다. 디렉터리 안의 `{이름}.guild` 파일로 길드를 식별한다.

```
# 기존 길드 열기
openguild.exe ./my-project

# .guild 파일이 없으면 초기화 프롬프트가 표시됨
```

GUI에서 디렉터리를 직접 선택하거나 최근 길드 목록에서 열 수도 있다.

### 퀘스트 생성
퀘스트는 길드 내에서 생성된다. 각 퀘스트는 타입 prefix와 자동 증가 ID를 가진다 (예: `DEV-001`, `BUG-003`).

기본 퀘스트 타입:
- `DEV` — 일반 개발 작업
- `BUG` — 버그 보고
- `REQ` — 기능 요청

### Quest Board
퀘스트가 노드로 표시되며, 상태별 레인으로 배치된다. 노드를 다른 레인으로 드래그하면 상태가 변경된다. 노드 간 화살표는 선행 퀘스트 또는 서브퀘스트 관계를 나타낸다.

### Quest List
모든 퀘스트를 단일 리스트로 표시한다. 서브퀘스트는 부모 퀘스트 하위에 접기/펼치기 트리로 나타난다. 이 뷰에서는 상태를 변경할 수 없으며, Quest Board 또는 Quest Detail 페이지에서 변경한다.


## 개발 환경 실행
**백엔드**
```bash
cd backend
cargo run -p server
```

**프론트엔드**
```bash
cd frontend
npm run dev
```
