; DEV-264: 설치 시점에 바로 ~/.openguild/ 에 skills(+docs) 를 복사해둔다.
;
; 배경: gui/src/lib.rs 의 sync_bundled_docs / sync_bundled_skill_marketplace 는
; "앱을 최소 1회 실행"해야만 도는 런타임 동기화라, CLI 만 설치하고 GUI 를 한
; 번도 안 띄운 사용자는 ~/.openguild/skill-marketplace 가 영영 안 생긴다.
; installer 가 직접 복사하면 설치 직후 바로 `/plugin marketplace add
; ~/.openguild/skill-marketplace` 가 가능 — Claude Code 를 켜기도 전에.
;
; $PROFILE 은 NSIS 내장 상수 — 현재 사용자 프로필 디렉토리(%USERPROFILE%).
; 이미 $INSTDIR\skills, $INSTDIR\docs 는 "-Core" 섹션의 resources 복사로
; 항상 설치돼 있음(GUI/CLI/Server 선택과 무관).
!macro NSIS_HOOK_POSTINSTALL
  CreateDirectory "$PROFILE\.openguild\skill-marketplace"
  CopyFiles /SILENT "$INSTDIR\skills\*.*" "$PROFILE\.openguild\skill-marketplace"

  CreateDirectory "$PROFILE\.openguild\docs"
  CopyFiles /SILENT "$INSTDIR\docs\*.*" "$PROFILE\.openguild\docs"
!macroend
