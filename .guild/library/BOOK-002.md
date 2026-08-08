+++
book_id = "BOOK-002"
title = "Quest Board 2손가락 트랙패드 제스처 지연 — 조사 전체 기록"
path = ""
created_at = "2026-08-01T22:37:23+09:00"
updated_at = "2026-08-08T20:20:19+09:00"
deleted = false
+++

# Quest Board 2손가락 트랙패드 제스처 지연 — 조사 전체 기록 (2026-08-01)

BUG-180/DEV-316/DEV-317 관련 실기 조사 전체 정리. macOS(Apple Silicon,
WKWebView) 실기, 일부 브라우저(Safari/Chrome) 교차 확인 포함.

## 증상

Quest Board 에서 트랙패드 **두 손가락**으로 pan/zoom 하면 배경(lane
DOM)은 즉시 움직이는데 **노드(카드)는 한 박자 늦게 따라옴**. 같은 보드를
**한 손가락**으로 클릭드래그하면 완전히 매끄러움 — 지연이 전혀 없음.

## 확정된 사실 (실기로 검증됨, 순서대로 밝혀진 것)

1. **WKWebView 뿐 아니라 순수 Safari(Tauri 전혀 무관)에서도 동일 재현**
   → Tauri 자체 버그 아님. 크롬에서는 거의 안 느껴짐 → 브라우저 엔진
   (WebKit 계열) 특성.
2. **한 손가락 드래그(cytoscape 내장 pan, 고빈도 pointermove 로 직접
   `cy.pan()`)는 문제없음. 두 손가락(`wheel` 이벤트 경유)만 문제.**
   → 우리 코드가 뭘 어떻게 처리하느냐가 아니라, **입력 소스가 OS/브라우저
   에게 "스크롤 제스처"로 분류되는지 여부**가 핵심 변수.
3. **드래그 중 화면 밖에서 새로 들어오는 영역은 안 그려지다 손을 떼야
   그려짐** — WebKit 이 제스처 진행 중엔 캔버스 리페인트(정확히는 화면
   컴포지팅)를 의도적으로 미루는 정책이 실재함을 보여주는 직접 증거.
4. **렌더 "방식"을 Canvas2D → WebGL 로 바꿔도(cytoscape 내장 `webgl:true`)
   증상 동일** — GPU 연산량/그리기 속도가 병목이 아님을 반증. (64개
   노드 GPU 인스턴싱 렌더는 이론상 1ms 미만이어야 함.)
5. **그리기 자체를 빠르게 해도(노드 배경 SVG 사전 디코드) 지연은 그대로
   재현** — "느려서"가 아니라 "화면에 보여주는 시점을 미루는 정책" 문제
   라는 걸 재확인.
6. **DOM 엘리먼트의 `transform`/`opacity` 전용 변경(lane 배경, 그리드
   스냅 점)은 이 지연이 전혀 없음** — 컴포지터 전용 갱신이라 메인스레드
   페인트 자체가 필요 없어서. WebKit 이 지연시키는 건 **`<canvas>` 컴포
   지팅에 한정**된 것으로 보임.
7. **`cy.png()` 캡처는 화면 표시 지연과 무관하게 그 시점 cy 의 논리적
   상태를 정확히 담는다**(동기 JS 연산, 화면 밖이었던 영역까지 포함해
   항상 정확) — 다만 **반복 호출 자체의 비용이 예상보다 훨씬 크다**
   (아래 11번 참조, 크롬에서도 체감 저하 발생).

## 시도한 방법들 (전부 최종 원복 — 요약 이력, 상세는 커밋/댓글 참조)

| # | 방법 | 가설 | 결과 | 트레이드오프/폐기 이유 |
|---|------|------|------|------------------------|
| 1 | `textureOnViewport`+`hideEdgesOnViewport` (cytoscape 내장) | pan/zoom 중 캐시 텍스처만 값싸게 transform 하면 페인트 비용 감소 | **무효** — 이미 리눅스(BUG-144, 2026-07-18)에서 WebKit Inspector 로 실측 후 화질 저하(엣지 사라짐) 대비 효과 미미로 기각됐던 옵션. 재발견 전까지 몰랐음 | 화질 저하, 근본 원인(컴포지팅 지연) 자체엔 무관하다는 게 나중에 밝혀짐 |
| 2 | `forceRender()` (cytoscape 강제 리렌더) | 캔버스↔DOM 스케줄 격차를 강제로 없앰 | **무효** | 스케줄 순서가 아니라 "언제 화면에 보여줄지"가 문제라 무관했음 |
| 3 | `webgl: true` (cytoscape 내장 WebGL 렌더러) | Canvas2D 소프트웨어 래스터가 병목 | **무효**, 화질만 저하 | 이 결과가 "그리기 속도"가 병목이 아님을 입증한 결정적 반증 |
| 4 | `cy.png()` 1회 캡처 + DOM `<img>` 오버레이(단일 버퍼) | 캔버스 대신 이미 그려진 스냅샷을 값싸게 transform | 부분 성공(지연은 없앰) 하지만 **디코딩 전 빈 프레임으로 깜빡임** | 단일 버퍼라 새 이미지 교체 순간 빈 프레임 발생 |
| 5 | 위 + 더블버퍼링(`<img>` 2장, `onload` 확인 후 교체) | 빈 프레임 제거 | 지연/깜빡임 크게 개선되지만 **재캡처가 잦은 순간(줌인/아웃, 캡처 범위 이탈)마다 버벅임** | `cy.png()` 캡처 자체가 반복 호출하기엔 비쌈(픽셀 읽기+PNG 인코딩, pixelRatio 2~3배) — 이때는 몰랐음 |
| 6 | CSS transform 전용 "가상" pan/zoom (캡처 없음, 제스처 중엔 cy 안 건드림) | 캡처 비용 자체를 없애면 버벅임 해소 | **버벅임은 해소**, 그러나 (a) 화면 밖 영역은 아예 안 그려짐(제스처 중 캔버스 자체를 안 건드리므로) (b) 제스처 종료 시 커밋 순간 옛 transform + 새 캔버스 내용이 겹쳐 **깜빡임** | 2가지 신규 문제 발생 |
| 7 | 6 + 제스처 종료 시 `cy.png()` 1회 캡처로 "리빌" 전환(더블버퍼) | 종료 시점만 캡처하면 (a)(b) 동시 해결 | 부분 성공하지만 **유예시간(캔버스 실제 캐치업 대기) 이 짧으면 깜빡임** | 캔버스가 "다 그렸다"를 알려주는 API 자체가 없어 유예시간을 추측할 수밖에 없음 |
| 8 | 7 의 유예시간 400→900ms 확대 | 넉넉하게 잡으면 항상 안전 | **미확인** — 사용자가 "느려지는 게 근본 해결이 아니다"라고 원칙적으로 반대해 실측 전 방향 전환. 900ms 자체가 실패했다는 증거는 없음 | 사용자 피드백: "느린 걸 감추는 거지 고치는 게 아니다" — 정당한 지적 |
| 9 | makeSvgUrl() 사전 디코드 + 원안(BUG-144, 매 프레임 직접 `cy.pan()/zoom()`, 캡처/마스킹 전부 제거) | 그리기 자체가 빠르면 WebKit 지연이 체감 안 될 정도로 줄 것 | **그리기는 실제로 빨라짐(화면 밖 영역 렌더 지연 개선)**, 그러나 **WebKit 표시 지연 자체는 그대로 재현** | 이 결과가 "느려서가 아니라 정책 문제"라는 결론의 핵심 증거. 사전 디코드 자체는 **유일하게 유지된 개선** |
| 10 | 6~7 재도입(CSS transform 가상 상태) + 유예시간 200ms(사전 디코드로 짧아도 될 것으로 기대) | 9의 사전 디코드가 실제 캐치업 시간을 단축했을 것 | **깜빡임/여백 재발** — 200ms 는 여전히 부족 | 유예시간 추측 문제로 원점 회귀 |
| 11 | 유예시간 추측 대신 **입력 있는 동안 120ms 주기로 `cy.png()` 재캡처 + 교체**(더블버퍼) — "언제 준비될지 몰라도 매번 최신 정확한 스냅샷만 보여주면 된다"는 아이디어 | 고정 유예시간 추측 자체를 없앰 | **크롬에서까지 체감 저하** — `cy.png()` 반복 호출 비용이 예상보다 훨씬 컸음 | 아이디어 자체는 타당했으나 `cy.png()` 의 실제 비용을 과소평가함. 즉시 원복 |

## 최종 상태 (유지된 것)

- **BUG-144 원안**(매 프레임 직접 `cy.pan()/zoom()` 호출, rAF 배칭) 그대로.
- **`makeSvgUrl()` 사전 디코드만 추가** — 노드 배경 SVG data-URI 를 생성
  시점(화면 표시 여부 무관)에 `new Image(); img.src = url` 로 미리
  로드시켜 브라우저 디코드 캐시를 데움. 화면 밖에 있다가 pan 으로
  갑자기 여러 노드가 들어올 때의 렌더 지연을 줄임 — 독립적으로 유효한
  개선.
- 잔여 증상: 두 손가락 제스처 중 노드가 배경보다 살짝(체감상 한 프레임
  ~수백ms) 늦게 따라옴. **이게 이 세션에서 낼 수 있는 최선**.

## 아직 안 해본 것 / 확인 안 된 의심

- **8번(유예시간 900ms)을 실제로 검증 안 함** — 사용자가 방향을 틀자고
  해서 900ms 로 정말 깜빡임이 없어지는지는 끝내 확인 못 함. 만약 "약간의
  지연"보다 "잠깐의 정적 이미지"가 사용자 경험상 더 낫다고 판단되면,
  7+8(유예 900ms, 사전 디코드와 결합)을 재시도해볼 여지는 남아있음 —
  다만 사전 디코드와 결합 시 900ms 까지 필요 없을 수도 있어 재보정
  필요.
- **`cy.png()` 의 정확한 비용 프로파일을 실측 안 함** — Chrome DevTools
  Performance 탭으로 "cy.png() 1회 호출이 정확히 몇 ms 걸리는지",
  "무엇이 비싼지(픽셀 readback vs PNG 인코딩 vs base64 변환)"를 안
  재봄. 알면 더 빠른 캡처 경로(예: `output:'blob'`/`ImageBitmap` 활용,
  base64 인코딩 생략)가 있을 수도 있음.
- **WebKit 의 "제스처 중 캔버스 컴포지팅 지연" 정책이 정말 존재하는지
  WebKit 소스/공식 문서로 확인 안 함** — 전부 실기 증거로부터의 추론.
  Safari Technology Preview 릴리즈 노트나 WebKit 버그 트래커에 관련
  이슈가 있는지 검색 안 해봄.
- **`will-change: transform` / `contain: paint` 같은 CSS 렌더 힌트를
  캔버스 엘리먼트에 걸어보는 시도는 안 함** — 컴포지터 레이어 승격을
  강제하면 WebKit 의 지연 정책 적용 대상에서 제외될 가능성 이론상 있음
  (미검증, 승산 낮다고 판단해 시도 안 함).
- **`requestIdleCallback`/`Scheduler.postTask` 같은 최신 스케줄링 API 로
  "지금 페인트해도 되는 타이밍"을 더 정교하게 잡을 수 있는지 미검토.**
- **네이티브 wgpu 서페이스를 웹뷰 밖에 별도로 띄우는 방법**(Tauri 창에
  네이티브 렌더 레이어를 얹는 것) — 이론상 WebKit 컴포지터 개입 자체를
  피할 수 있어 가장 근본적이지만, 플랫폼별 창 합성 구현 난이도가 높고
  웹 배포 타깃과는 완전히 별도 구현이 필요해 검토만 하고 시도 안 함.
- **DOM 기반 노드 렌더링(DEV-317)** — lane 배경이 이 문제가 없는 이유와
  정확히 같은 원리로 근본 해결이 될 것으로 예상되나, 보드 렌더링
  아키텍처 전체를 바꾸는 큰 작업이라 설계 검토 단계에서 멈춤. 노드 수가
  많을 때 DOM 엘리먼트 오버헤드가 캔버스보다 나을지 실측도 안 됨(가상화/
  windowing 필요 여부 포함).
- **Windows(WebView2/Chromium)에서의 실기 검증 전무** — 이 세션 전체가
  macOS 단독 조사. Chromium 계열은 이 문제 자체가 없을 것으로 추정만
  하고 있음(Chrome 브라우저 테스트로 방증은 됐으나 실제 Windows Tauri
  앱에서 확인 안 함).
- **터치스크린 핀치(DEV-208, 실제 터치 하드웨어)의 macOS/WebKit 동작
  미확인** — 이 세션에서 코드는 wheel 경로와 동일하게 맞춰뒀지만, 실제
  터치 디바이스가 없어 검증 못 함. touch 이벤트는 wheel 과 달리 OS
  제스처 분류가 안 걸릴 가능성도 있어(한 손가락 드래그처럼) 어쩌면
  이 경로는 애초에 문제가 없을 수도 있음 — 미확인.

## 참고

- 관련 퀘스트: BUG-180(on_hold, 이 조사 전체), DEV-316(testing, 사전
  디코드만 남음), DEV-317(open, DOM 렌더 근본 해결책 후보).
- 리눅스 선례: BUG-144(WebKitGTK, 2026-07-18) — 같은 결론(WebKit 계열
  렌더링 성능 한계, textureOnViewport 검토 후 기각)에 이미 도달했었으나
  퀘스트 요약에는 안 남고 커밋 본문에만 있어 이번 조사 초반에 놓쳤음.
  향후 유사 조사 시 `git log` 커밋 본문까지 검색할 것.

## 후속 재검토 — Canvas 제거와 DOM/SVG 단일 월드 설계 (2026-08-05)

[[BUG-180]] / [[DEV-316]] / [[DEV-317]] 기록과 현재 코드를 다시 검토하고
사용자와 증상을 단계별로 재정리했다.

### 원인 설명의 정정·구체화

현재 보드는 하나처럼 보이지만 두 렌더링 계층이 섞여 있다.

```text
Quest Board
├─ 레인/헤더/그리드: DOM/CSS
└─ 노드/관계선: Cytoscape Canvas
```

wheel 입력으로 pan/zoom 값이 바뀌면 DOM 레인은 기존 픽셀의 위치만 CSS로
합성할 수 있지만, Cytoscape는 고정 크기 Canvas 내부 카메라를 바꾸고 노드와
관계선을 새 프레임으로 다시 그려 화면에 제출해야 한다. WebKit의 두 손가락
scroll gesture 중 이 새 Canvas 프레임의 repaint 또는 화면 합성 반영이 늦어져
새 위치의 DOM 레인과 이전 Canvas 프레임의 노드가 잠시 함께 보이는 것이
디싱크의 핵심으로 판단된다.

따라서 "제스처 중 Canvas가 아예 다시 그려지지 않는다"까지는 확정할 수 없다.
논리 상태, Canvas backing frame 생성, 최종 compositor 표시 중 정확히 어느
경계에서 지연되는지는 미확정이다. 다만 `cy.png()`가 새 논리 상태를 정확히
담고, Canvas2D와 WebGL에서 동일하며, CSS transform으로 기존 Canvas surface
자체를 움직였을 때 제스처 중 버벅임이 사라졌다는 실기 결과는 "새 Canvas
프레임의 화면 반영 경로"가 문제라는 설명과 일치한다.

### 현재 최우선 해결 방향

레인·노드·관계선을 같은 렌더링 방식과 월드 좌표계로 통일한다.

```text
board viewport
└─ board-world              ← pan/zoom 시 이 부모 transform 하나만 갱신
   ├─ DOM lane layer
   ├─ SVG edge layer
   └─ DOM quest-card layer
```

- 노드는 absolute-positioned Svelte/HTML 카드로 렌더하고 `left/top`은 월드
  좌표로 유지한다.
- 관계선은 같은 월드 좌표계의 SVG `<path>`로 렌더한다.
- pan/zoom마다 노드 각각의 style이나 SVG path를 갱신하지 않고
  `board-world`의 `translate3d(...) scale(...)`만 한 번 변경한다.
- 카드 드래그/자동 배치/상태 변경 때만 카드 좌표와 연결된 path를 갱신한다.
- 레인·노드·관계선이 같은 부모 transform을 공유하므로 서로 다른 시각
  프레임으로 어긋날 수 없고, 제스처 중 Canvas repaint/commit 경로도 제거된다.

기존 [[DEV-317]] 본문의 "pan/zoom 이벤트마다 각 DOM 노드 transform 갱신"은
O(N) style write가 되므로 폐기하고, 부모 transform 1회 원칙으로 수정한다.

### 구현 선택 — 외부 노드 에디터보다 기존 Svelte UI 재사용 우선

Svelte Flow는 DOM custom node, SVG edge, pan/zoom, 드래그, 박스/다중 선택,
visible-only rendering을 제공해 기술적으로 유력한 대안이다. 그러나 이번
보드는 범용 flow editor보다 프로젝트 전용 보드 성격이 강하다.

- 기존 Quest 카드/확장 팝업/버튼/배지/테마/i18n 구현을 그대로 활용해야 한다.
- 레인 접기·숨김, grid snap, 상태 변경 확인, 저장 좌표 변환, highlight/dim,
  undo/redo 등 프로젝트 고유 동작이 이미 앱 코드에 깊게 구현돼 있다.
- 범용 라이브러리에 이 동작들을 맞추면 기존 Cytoscape 의존을 다른 렌더러
  의존으로 교체하면서 adapter와 상태 동기화 계층을 새로 만들 가능성이 높다.
- 현재 Cytoscape 레이아웃도 자동 그래프 레이아웃이 아니라 `preset`이며,
  좌표·레인·상호작용의 상당 부분을 앱이 이미 직접 관리한다.

따라서 **운영 구현은 기존 Svelte 컴포넌트와 보드 로직을 재사용하는 직접
DOM/SVG renderer를 우선**한다. Svelte Flow는 직접 구현 전에 별도 POC로
성능/행동을 비교하거나, 직접 구현의 입력·선택 로직이 예상보다 커질 때
재검토하는 후보로 남긴다.

### 대안의 우선순위

1. 기존 Svelte 카드/팝업/버튼을 재사용한 직접 DOM node + SVG edge renderer.
2. 필요하면 D3 Zoom 같은 작은 도구만 pan/zoom 수학에 제한적으로 사용.
3. Svelte Flow는 비교 POC 또는 직접 구현 복잡도가 과도할 때 검토.
4. SVG 단일 renderer는 텍스트 줄바꿈·버튼·접근성·Safari `foreignObject`
   위험 때문에 우선하지 않는다.
5. 네이티브 overflow 스크롤이나 Tauri native wgpu surface는 DOM/SVG 방식의
   실기 결과가 불충분할 때 후속 대안으로 남긴다.

### POC 검증 기준

- Safari/WKWebView 두 손가락 pan/zoom에서 레인·카드·관계선이 같은 프레임으로
  움직일 것.
- 프레임당 시각 갱신은 부모 transform 1회일 것.
- 50/200/500개 카드에서 FPS·메인 스레드·메모리를 측정할 것.
- 카드별 `will-change`나 개별 합성 레이어 승격은 피하고, 큰 월드 전체가
  과도한 backing layer가 되는지 Web Inspector Layers에서 확인할 것.
- 기존 팝업/버튼, 단일·다중 드래그, 박스 선택, grid snap, 상태 변경,
  edge highlight, viewport 저장/복원을 회귀 항목으로 둘 것.

## DEV-317 구현 완료 — Canvas 없는 직접 DOM/SVG 보드 (2026-08-05)

위 설계를 실제 구현으로 전환했다. 최종 구조에는 Quest Board용 Cytoscape가
남아 있지 않다.

```text
board viewport
└─ board-world                 ← 제스처 중 이 transform만 변경
   ├─ DOM lane/grid
   ├─ SVG relationship paths
   └─ DOM quest cards
```

`BoardGraph`는 renderer가 아니라 위치, 선택, viewport, collection 연산만 보존한
앱 전용 상태 모델이다. mouse/touch drag, wheel pan/zoom, pinch, Ctrl/Meta 선택,
박스 선택은 컴포넌트가 직접 처리한다. 노드 drag와 자동 배치처럼 월드 좌표가
실제로 바뀔 때만 해당 카드와 연결 path를 rAF로 갱신한다. pan/zoom은 카드 수와
무관하게 부모 transform 한 번이다.

### 검증 결과

- self guild에서 DOM card 546개, SVG edge 183개가 렌더됐고 보드 내부 Canvas는
  0개였다.
- mouse drag pan, trackpad형 wheel pan, Ctrl+wheel zoom, popup, Ctrl/Meta 선택이
  실제 브라우저에서 동작했고 console warning/error는 0건이었다.
- 500-node 테스트에서 pan+zoom은 node position/graph 갱신을 전혀 유발하지 않고
  viewport callback만 호출했다.
- frontend 365 tests, Rust workspace 709 tests, svelte-check, production build,
  theme token 검사가 통과했다.

### 의존성과 메모리 판단

`cytoscape`와 `@types/cytoscape`는 직접 의존성에서 제거했다. `npm ls`에 남는
Cytoscape는 Markdown 다이어그램용 Mermaid의 전이 의존성이라 Quest Board와는
무관하다.

Canvas 한 장보다 DOM 546개와 SVG path 183개의 최초 mount/style/layout 메모리는
더 클 수 있다. 대신 이전의 노드별 고해상도 SVG data image/raster decode와
Cytoscape Canvas backing store를 보드에서 없앴고, 카드별 `will-change`도 쓰지
않는다. 핵심 제스처 경로는 전체 노드 재렌더가 아닌 부모 합성 1회라 이번 프레임
드롭 원인에는 직접 대응한다. 수천 개 노드에서 문제가 생길 때만 overscan 가상화를
추가하되, 제스처 중 mount/unmount는 하지 않는 것이 원칙이다.

남은 완료 조건은 macOS release 앱/Safari의 실제 두 손가락 트랙패드 실기와 전체
drag/drop·snap·undo/redo 회귀 확인이다. 구현 퀘스트 [[DEV-317]]은 그 확인을 위해
Testing 상태로 둔다.

## 2026-08-05 추가 검증 — native WKWebView / 격리 UI 회귀

현재 소스로 debug `openguild-gui`를 다시 빌드하고 실제 저장소 경로를 넘겨
Tauri의 guild launch 경로가 정상 기동되는 것을 확인했다.

macOS의 실제 `WKWebView`를 사용하는 스냅샷 하네스에서는 self guild 기준
DOM card 547개, SVG edge 183개, 보드 Canvas 0개를 확인했다. 트랙패드와 같은
작은 delta의 wheel 입력 24회를 연속 전달했을 때 `board-world` transform도
24개의 서로 다른 값으로 진행했고, 시작/중간/끝 스냅샷에서 lane/card/edge가
같은 위치 관계를 유지한 채 함께 이동했다.

다만 자동화용 WKWebView는 창이 숨김 상태라 `requestAnimationFrame`이 정지했다.
하네스에서 이 부분만 timer로 대체했고 wheel도 합성 입력을 썼으므로, 이 결과를
사람이 보이는 Tauri 창에서 실제 두 손가락 트랙패드를 사용한 검증으로 과장하지
않는다.

별도의 6-quest/2-edge 길드에서는 실제 브라우저 포인터 입력과 API 저장값을 함께
대조해 다음 회귀를 확인했다.

- 같은 lane drag가 DB 좌표를 변경하고, undo는 기준 좌표로, redo는 이동 좌표로
  정확히 복원했다.
- Meta 다중 선택과 popup의 `연관 전체 → 선택`이 동작했고, DEV-001과 선행
  DEV-006이 함께 선택됐다.
- 다른 lane drop 취소는 status/좌표를 모두 유지했다. 확정은 DEV-001을
  `open → in_progress`와 새 좌표로 저장했고, undo가 status와 좌표를 함께 복원했다.
- grid snap은 드롭 좌표를 실제 격자 중심 `(318, 324)`에 저장했다.
- 전체 정렬은 모든 대상 좌표를 저장했고, undo가 정렬 전 좌표 전부를 복원했다.

초기 하네스에서 undo 좌표가 복원되지 않은 것처럼 보였던 값은 Svelte DOM 반영을
기다리지 않고 읽은 테스트 오류였다. 실제 포인터 입력 뒤 DOM과 API를 충분히
기다려 대조하자 정상 복원이 확인됐다.

현재 자동화 드라이버는 보조키를 누른 채 포인터 drag하는 입력을 WKWebView에
유지하지 못해 box selection 실입력 검증은 완료하지 못했다. 따라서 남은 수동
완료 조건은 다음과 같다.

1. 보이는 macOS release Tauri/Safari 창에서 실제 두 손가락 pan/zoom 시
   lane/card/edge가 같은 프레임에 붙어 움직이는지 확인.
2. 실제 `Ctrl/Meta + drag` box selection과 실제 터치 장치 pinch 확인.
3. filter, lane 숨김·접기·순서 변경, dark/light 및 한국어/영어 조합의 최종
   육안 점검과 필요 시 Web Inspector Memory/Layers 비교.

자동 회귀와 native WebKit 구조 검증은 통과했지만 1번이 이 버그의 핵심 재현
조건이므로 [[DEV-317]]은 `Testing` 상태를 유지한다.

## 2026-08-05 표시 설정·메모리 기준·collapsed lane 회귀

self guild의 production bundle에서 표시 설정 회귀와 메모리 기준을 추가로
확인했다.

- 보드 기본 상태는 DOM node 547개, SVG edge path 185개, 보드 Canvas 0개였고
  페이지 전체 DOM element는 4,412개였다.
- release Tauri 앱의 idle RSS 한 번 측정값은 main 115,600 KiB,
  WebContent 90,672 KiB, Networking 13,648 KiB였다. 합계는 219,920 KiB
  (약 214.8 MiB)다. 이는 현재 규모의 기준점일 뿐, 이전 Canvas 구현과 동일
  조건 비교나 leak 판정값은 아니다.
- dark theme와 English로 전환해도 node 547 / edge 185 / Canvas 0을 유지했고,
  lane 및 toolbar 문구가 영어로 전환됐다. 이후 System theme와 한국어로 원복했다.
- `진행 중` lane 숨김 시 node 547→544, edge 185→160으로 관련 DOM/SVG가 함께
  제외됐고 재표시 후 547/185로 복원됐다.
- `진행 중` lane을 왼쪽으로 이동했을 때 header 순서가 실제로 바뀌고, 오른쪽으로
  이동하면 원래 순서로 복원됐다.
- DEV type filter는 node 수를 유지한 채 213개 node에 `filter-dim`을 적용했고,
  All로 원복하면 dim 0이 됐다.

레인 접기 회귀 중에는 별도 hit-area 문제를 발견해 함께 수정했다. 모든 lane을
화면에 맞춘 저배율에서 collapsed lane의 화면 폭이 7.59px인데 header의 기존
좌우 padding 합은 8px여서, 다시 펼치는 `.lane-label`의 실제 폭이 0px가 됐다.

collapsed header의 padding을 제거하고 label을 header 전체 폭의 absolute
hit-area로 만들었다. production rebuild/reload 후 같은 조건에서 label 폭이
0→6.59px로 유지됐고, 실제 browser click으로 collapsed `진행 중` lane이 다시
펼쳐졌다. 테스트를 위해 숨긴 lane과 collapsed 상태는 모두 원복했으며 최종
상태는 node 547 / edge 185 / hidden 0 / collapsed 0 / Canvas 0이다.

수정 후 `svelte-check` 0 errors/0 warnings, frontend 34 files/365 tests,
production build, `git diff --check`가 다시 통과했다. 실제 두 손가락 트랙패드와
실제 터치 pinch만 하드웨어 수동 확인으로 남기고 [[DEV-317]]은 `Testing`으로
돌린다.

## 2026-08-05 실제 트랙패드 확인 후 표시·GPU 회귀 수정

사용자가 macOS release 앱에서 실제 두 손가락 제스처를 확인한 결과, 원래 문제인
lane/card/edge의 서로 다른 프레임 이동은 해소됐다. Canvas를 제거하고 세 요소를
같은 DOM/SVG world에 둔 방향이 핵심 증상에는 유효했다.

그 뒤 세 가지 회귀가 발견됐다. 화살표가 기존보다 과하게 휘었고, 확대 시 HTML
노드가 흐렸으며, 깊은 축소 시 GPU 사용량과 버벅임이 커졌다.

### 원인과 수정

1. 새 edge 함수가 모든 관계에 최소 18px bend를 강제했다. 기존 Cytoscape
   bezier의 시각 결과처럼 단일 관계는 직선으로 복원하고, 동일한 두 노드 사이에
   병렬 관계가 있을 때만 40px 간격으로 대칭 분리했다.
2. SVG edge의 `vector-effect: non-scaling-stroke`를 제거했다. 이 옵션은 깊게
   축소해도 화면상 선 굵기를 고정해 논리 좌표에서는 매우 굵은 curve를 계속
   restroke하게 만든다. 이제 선과 marker도 월드 배율을 함께 따른다.
3. `board-world`를 `will-change: transform` 영구 GPU layer로 두던 구조를
   `board-world-viewport`(제스처 중 임시 transform)와 안쪽 `board-world`
   (확정 CSS `zoom`)로 분리했다. 연속 zoom 중 배율 차가 20~25% 이상이고 80ms
   간격을 넘을 때만 중간 raster 배율을 확정하고, 입력이 120ms 멈추면 최종
   배율로 확정한다. 매 frame layout/paint를 피하면서 확대 시 현재 배율로
   선명하게 재래스터하고, 축소 시 거대한 원본 backing layer의 장기 합성을
   피하는 절충이다.

### 검증과 남은 조건

self guild production bundle은 DOM node 547개, SVG edge 183개, Canvas 0개다.
현재 데이터에는 병렬 관계가 없어 183개 edge가 모두 직선이며,
`non-scaling-stroke`와 강제 `will-change`는 모두 0이다. 저장된 전체보기 배율
0.0309255에서 논리 world 6,852×25,514px의 실제 렌더 영역은 약 212×789px로
줄어, 전체 논리 크기의 GPU backing layer를 유지하지 않는다.

`svelte-check` 0 errors/0 warnings, frontend 35 files/367 tests, production
build, `git diff --check`, release GUI rebuild가 통과했다. 화살표 모양, 확대 후
선명도, 깊은 축소의 GPU 사용량과 체감 프레임은 새 release 앱에서 실제
트랙패드로 다시 확인해야 하므로 [[DEV-317]]은 Testing으로 이동한다.

## 2026-08-05 CSS zoom 후속 회귀 — 내부 배율·레인 잘림

GPU/선명도 보정에 사용한 CSS `zoom`은 world 전체를 단순 camera transform하는
것이 아니라 자식의 layout과 font를 새 배율로 다시 계산했다. 실제 release 앱에서
노드 외곽과 pill/font가 다른 비율로 변하고 lane 단색 배경의 위/아래가 잘리는
회귀가 확인돼 이 선택을 철회했다.

- 안쪽 world의 확정 배율을 `zoom`에서 `transform: scale(...)`로 변경했다.
  제스처 중에는 바깥 viewport가 임시 배율 차이만 보간하고, 주기적으로 안쪽
  transform scale을 확정한다. 따라서 카드 외곽·pill·font·SVG가 동일한 기하학적
  배율을 따른다.
- lane은 viewport 전체 높이를 채우는 screen-space 단색 배경과, 카드와 함께
  움직이는 world-space grid dot으로 분리했다. 단색 배경은 X/폭만 pan/zoom을
  따라 위·아래가 잘리지 않고, dot은 기존 world transform을 유지한다.

production bundle의 zoom 0.0309255에서 node 284×80px는 화면
8.7828×2.4740px였고, pill/title 높이를 같은 배율로 역산하면 각각
17.0003/30.0000px였다. 첫 lane의 top/bottom은 board-wrap의 52/720px와
정확히 일치했다. screen lane 7, world grid lane 7, Canvas 0도 확인했다.

`svelte-check` 0/0, frontend 35 files/367 tests, production build, release GUI
rebuild가 통과했다. 새 release 앱에서 실제 트랙패드 육안 확인을 남겨
[[DEV-317]]은 Testing으로 이동한다.

## 2026-08-05 최종 단순화 — 중첩 scale도 제거

CSS `zoom`을 안쪽 `transform: scale()`로 교체한 뒤에도 실제 release 앱에서
pill/font가 노드 외곽과 따로 변하는 현상이 유지됐다. 최종 배율의 수학적 곱은
같더라도 WebKit이 바깥 임시 scale과 안쪽 확정 scale을 별도 layer로 raster할
수 있으므로, 80ms 중간 확정·120ms 종료 확정 및 배율 상태를 모두 제거했다.

현재 카드·내부 요소·SVG edge·grid dot은 오직
`board-world-viewport: translate3d(pan) scale(zoom)` 하나만 공유한다. 안쪽
`board-world`에는 inline transform과 CSS zoom이 없다. 단색 lane 배경만
viewport 높이를 채우기 위해 screen-space로 두고 X/폭을 같은 pan/zoom 값에서
갱신한다.

Tailscale 전용 주소 `http://100.78.25.64:34173`에 production 서버를 띄워
실제 그 주소로 접속했다. viewport transform 하나, 안쪽 transform/zoom 없음,
node 547 / edge 183 / Canvas 0을 확인했다. node와 pill의 측정 배율도 일치했다.
frontend 367 tests, svelte-check, production/release GUI/server build가 통과했다.

서버는 인증이 없으므로 모든 인터페이스 `0.0.0.0` 대신 Tailscale IP
`100.78.25.64`에만 바인딩했다. 외부 기기 실측을 위해 [[DEV-317]]은
Testing으로 이동한다.

## 2026-08-05 터치 UI click 회귀 — 제스처 입력 경계 제한

외부 터치 장치에서 노드만 클릭되고 새 퀘스트, 레인 제목, 보드 도구와 팝업
버튼이 동작하지 않는 회귀가 발견됐다. 보드 전체에 등록된 `touchstart`가 노드
외의 모든 터치를 pan으로 간주해 `preventDefault()`를 호출하면서, 브라우저의
후속 `click` 합성을 막고 있었다.

예외 버튼 selector를 계속 늘리는 방식은 새 UI가 추가될 때 같은 버그를 반복한다.
따라서 허용 목록의 방향을 뒤집어, 제스처 시작점을 실제 노드 `.board-node`와
빈 보드 입력면 `.board` 두 곳으로만 제한했다. toolbar/lane/dialog 등 나머지는
기본 tap/click 경로를 유지하며, pinch 역시 같은 입력 경계를 따른다.

전용 입력 경계 테스트를 포함해 frontend 36 files/370 tests와 svelte-check,
production/release GUI/server build, `git diff --check`가 통과했다. 갱신한
Tailscale production 주소에서 새 퀘스트 모달 열기/취소, 보드 설정 모달 열기,
노드 팝업 닫기를 확인했다. 실제 터치 장치에서 최종 확인한다.

## 2026-08-08 저배율 후속 — screen-space grid, 노드 LOD, 120Hz HUD

[[DEV-317]]의 단일 DOM/SVG world는 원래 Canvas 제스처 지연을 해결했지만,
self guild처럼 노드가 560개인 보드를 깊게 축소하면 다음 후속 문제가 드러났다.

- grid snap 점은 고정 높이의 world SVG bitmap이라 멀리 위·아래로 pan하면
  끊겼고, world와 함께 축소되어 zoom 0.03에서 반지름이 약 0.05 화면 px까지
  작아졌다.
- collapsed lane은 노드만 숨기고 grid 시각 레이어는 숨기지 않았다.
- 전체 보기에서도 모든 카드의 pill, icon, 날짜, 제목 DOM을 계속 paint했다.
- 실제 Tauri/WebKit이 60Hz에 제한됐는지, 120Hz 중 프레임을 놓치는지 앱 안에서
  구분할 수단이 없었다.

[[BUG-225]]에서 grid를 world transform 밖의 viewport-space 레이어로 옮겼다.
각 visible/non-collapsed lane만 화면 높이와 두 타일의 overscan을 갖고, CSS
radial-gradient의 작은 반복 타일과 Y phase transform으로 무한 grid처럼 보인다.
점 반지름은 화면 px 기준 0.9~2.25px로 clamp해 저배율에서도 식별 가능하다.
따라서 world 전체 높이의 bitmap/DOM을 만들지 않고, 접힌 레인의 점도 즉시
사라진다.

노드는 zoom에 따라 세 단계 LOD를 사용한다.

- `detail`(0.55 이상): 기존 pill/icon/date/title 카드.
- `compact`(0.16~0.55): ID와 제목만 있는 단순 카드.
- `overview`(0.16 미만): hit-test 가능한 urgency 색 marker만 유지.

375×720 viewport의 실제 fit zoom 0.0253429에서 node 560개는 유지하되 내부
자식 element가 1,120개에서 0개로 줄었다. grid 점은 반지름 0.9px로 유지됐고,
위·아래 pan 뒤에도 dot layer가 viewport 양쪽을 넘겨 덮었다. 41-point drag
실측에서 rAF 120Hz, median 8.3ms, p95 9.7ms, 12.5ms 초과 0%, viewport update
40/s였다.

도구바의 `Hz 성능` HUD는 1초 창의 rAF Hz, median/p95 frame interval,
12.5ms 초과 비율, viewport transform 갱신 수, zoom과 LOD를 표시한다. 120Hz라면
median이 약 8.3ms, 60Hz cap이면 약 16.7ms와 60Hz로 보인다. rAF 수치는 실제
화면 scan-out 자체의 증명은 아니므로 Tauri에서 60Hz가 나오면 Safari Web
Inspector Timelines/Layers와 함께 판단한다. HUD는 꺼져 있을 때 rAF loop를
유지하지 않는다.

frontend 39 files/399 tests, `svelte-check`, production build, release GUI build,
`git diff --check`가 통과했다. 인앱 브라우저는 120Hz까지 확인했으며 macOS
Tauri의 실제 값은 release 앱에서 HUD를 켜 확인한다.

## 2026-08-08 BUG-225 후속 — 명시적 3열 snap과 debug 전용 HUD

실제 macOS Tauri 확인에서 두 가지 후속 차이가 발견됐다. snap은 laneCols=3이어도
CSS radial-gradient의 가로 반복이 한 열처럼 보였고, 점 반지름 0.9~2.25 화면
px는 특히 축소 화면에서 지나치게 굵었다. 또한 ProMotion(최대 120Hz) MacBook
Pro의 Tauri 앱에서 rAF HUD가 정확히 30Hz로 고정됐다.

snap은 가로 background repeat에 기대지 않도록 바꿨다. 레인마다 최대 세 개의
독립적인 세로 dot column DOM을 만들고 laneCols=1/2/3만큼만 표시한다. 각 열의
중심 X는 첫 cell 중심과 cell 폭으로 직접 계산하므로 3열 설정이면 구조적으로 세
열이 존재한다. 점 반지름은 screen-space 0.55~1.35px로 줄였다. 저배율에서도
최소 1.1px 지름은 유지하지만 이전 최대 지름 4.5px보다 훨씬 작다.

성능 HUD는 일반 사용자 기능이 아니라 진단 장치로 제한했다. 툴바 버튼과 번역
라벨을 제거하고, Vite dev 또는 Rust debug build에서만 `Cmd/Ctrl+Shift+H`로
토글한다. packaged debug는 frontend의 `import.meta.env.DEV`로 판별할 수 없으므로
Tauri `is_debug_build` command를 사용한다. release build에서는 상태가 활성화되지
않아 단축키도 동작하지 않는다. HUD에는 page visibility/focus도 함께 표시한다.

보드 구현에는 30fps timer나 cap이 없다. wheel 이벤트는 같은 display frame의
입력을 requestAnimationFrame 한 번으로 합칠 뿐이고, HUD 역시 rAF callback 간격을
직접 잰다. 동일 보드의 인앱 Chromium dev 화면은 120Hz, median 8.3ms로 측정됐기
때문에 Tauri의 30Hz는 보드 로직이 아니라 macOS WKWebView의 frame scheduling
경로 차이다.

WebKit 공개 이슈 294338은 hybrid WKWebView가 기본 약 60fps에 제한되고 120Hz를
요청할 공개 API가 아직 없다고 기록한다. Safari의 관련 feature flag도 embedded
WKWebView에는 적용되지 않는다. 30Hz는 이 60Hz 상한보다도 낮으므로, HUD가
`visible/focus`인지 확인하고 macOS 저전력/열/부하 상태를 함께 비교해야 한다.
Wry의 `backgroundThrottling`은 window 밖/비활성 webview 정책이므로 foreground
30Hz 해결책으로 임의 적용하지 않는다. private WebKit preference를 쓰는 우회는
깨지기 쉽고 배포 안정성이 없어 현재 범위에서는 채택하지 않는다.

브라우저 실화면에서 laneCols=3일 때 각 레인의 세 dot column, debug shortcut
on/off, 툴바 성능 버튼 미존재를 확인했다. viewport helper 6 tests,
svelte-check, production build와 diff/format check가 통과했다.
