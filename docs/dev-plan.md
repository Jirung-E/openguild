# OpenGuild 개발 순서 계획 (MVP)

## 1단계 — 프로젝트 초기 설정
- GitHub 저장소 생성 + main/develop 브랜치
- Cargo workspace 초기화 (backend)
- Svelte 프로젝트 초기화 (frontend)
- CLAUDE.md 작성
- .gitignore, README

## 2단계 — 백엔드 기반
- DB 스키마 설계 + 마이그레이션
- `core` crate: 모델 정의 (Quest, Guild 등)
- `{name}.guild` TOML 파싱
- Axum 서버 기본 설정 (CORS, 라우터)

## 3단계 — 백엔드 API
- Quest CRUD
- 상태 변경 API
- 서브퀘스트 / 선행 퀘스트 관계
- 노드 위치 저장/조회

## 4단계 — 프론트엔드 기반
- Svelte 라우팅 설정
- API client
- 공통 컴포넌트 (레이아웃, 네비게이션)

## 5단계 — Quest List
- 트리 뷰 (서브퀘스트 접기/펼치기)
- 필터바

## 6단계 — Quest Board
- Cytoscape.js + 레인 구성
- 노드 드래그 + 위치 저장
- 화살표 (선행/서브 관계 구분)
- 상태 변경 드래그

## 7단계 — Quest Detail
- 메타 헤더
- marked.js 렌더링
- CodeMirror 6 편집 모드

## 8단계 — CI/CD + 배포
- GitHub Actions 설정
- AWS EC2 배포
