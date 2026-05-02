# OpenGuild 소프트웨어 아키텍처

## 전체 구조

```mermaid
graph TB
    subgraph Client["클라이언트"]
        B[브라우저]
        M[모바일]
        D[데스크탑]
    end

    subgraph Frontend["프론트엔드 (Svelte + Vite)"]
        subgraph Pages["Pages / Routes"]
            P1[GuildList - 길드 목록]
            P2[GuildView - 보드+리스트 탭]
            P3[QuestDetail - 퀘스트 상세]
            P4[CampaignDetail - 캠페인 상세]
            P5[Settings - 설정]
        end
        subgraph Components["Components"]
            C1[QuestBoard - Cytoscape.js]
            C2[QuestList - 트리뷰]
            C3[QuestCard - 노드/행]
            C4[MarkdownEditor - CodeMirror 6]
            C5[FilterBar - 필터/검색]
        end
        subgraph State["Svelte Stores"]
            S1[questStore]
            S2[campaignStore]
            S3[filterStore]
            S4[uiStore]
        end
        API_CLIENT[API Client - fetch wrapper]
    end

    subgraph Backend["백엔드 (Rust + Axum) - AWS EC2"]
        subgraph Middleware["Middleware"]
            MW1[CORS]
            MW2[Logger]
            MW3[Auth - 추후 JWT]
        end
        subgraph Routes["Routes / Handlers"]
            R1["/api/quests - CRUD"]
            R2["/api/quests/:id/status - 상태 변경"]
            R3["/api/quest-positions - 노드 위치"]
            R4["/api/campaigns - CRUD"]
            R5["/api/guild - 길드 정보"]
            R6["/webhook/github - 추후"]
        end
        subgraph Services["Services (core crate)"]
            SV1[QuestService]
            SV2[CampaignService]
            SV3[GuildService]
        end
        subgraph Repository["Repository (core crate)"]
            RP1[QuestRepository]
            RP2[CampaignRepository]
            RP3[GuildRepository]
        end
    end

    subgraph Storage["스토리지"]
        DB[(SQLite - openguild.db)]
        GF["{name}.guild - TOML"]
        MD["quests/*.md"]
    end

    subgraph GitHub["GitHub"]
        GI[Issues]
        GR[Repository]
        GA[Actions CI/CD]
    end

    Client --> Frontend
    Frontend --> State
    State --> API_CLIENT
    API_CLIENT -->|HTTP REST| Middleware
    Middleware --> Routes
    Routes --> Services
    Services --> Repository
    Repository --> DB
    Repository --> MD
    GF -->|시작 시 읽기| Backend
    GA -->|자동 배포| Backend
    GR --> GA
```

## Cargo Workspace 구조

```
backend/
├── Cargo.toml       ← workspace 루트
├── server/          ← Axum 서버 (바이너리)
│   └── src/
│       ├── main.rs
│       ├── routes/
│       └── middleware/
├── core/            ← 공통 라이브러리
│   └── src/
│       ├── models/
│       ├── repository/
│       └── services/
└── tools/           ← 유틸리티 바이너리들
    └── src/
```

## API 엔드포인트 (MVP)

| Method | Path | 설명 |
|---|---|---|
| GET | /api/guild | 길드 정보 조회 |
| GET | /api/quests | 퀘스트 목록 |
| POST | /api/quests | 퀘스트 생성 |
| GET | /api/quests/:id | 퀘스트 상세 |
| PUT | /api/quests/:id | 퀘스트 수정 |
| DELETE | /api/quests/:id | 퀘스트 삭제 |
| PUT | /api/quests/:id/status | 상태 변경 |
| GET | /api/quest-positions | 노드 위치 목록 |
| PUT | /api/quest-positions | 노드 위치 저장 |
