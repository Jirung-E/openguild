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

; DEV-327: 지울 때 ~/.openguild 정리. **길드(.guild 가 든 폴더)는 사용자 문서다 — 어디에 있든 건드리지
; 않는다.** ~/.openguild 안에는 길드가 없다(최근 목록은 경로만 적는다).
;
; - 언제나: 위에서 설치가 복사해 둔 사본(skill-marketplace · docs) — 앱 파일이다.
; - '앱 데이터 삭제' 를 골랐을 때만: 이 기계의 설정 — 최근 길드 · 언어 · 테마 · 플러그인 허용 · 플러그인
;   설정값(토큰 등 비밀값이 들어 있다) · 플러그인 소스 · 서버 설정 · 오류 기록 · 복사 임시 폴더.
; - 남긴다: plugin-data — 플러그인이 남긴 기록(삭제 감사 로그 · 받은 편지함 등)은 사용자가 만든 자료다.
;   비어 있으면 ~/.openguild 도 지운다.
;
; 업데이트(/UPDATE)는 새 버전을 깔기 전에 옛것을 지우는 것이라 아무것도 안 건드린다 — 새 설치가 사본을
; 다시 복사하고, 설정은 그대로 이어져야 한다.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    RMDir /r "$PROFILE\.openguild\skill-marketplace"
    RMDir /r "$PROFILE\.openguild\docs"
    ${If} $DeleteAppDataCheckboxState = 1
      Delete "$PROFILE\.openguild\recents.json"
      Delete "$PROFILE\.openguild\locale.json"
      Delete "$PROFILE\.openguild\themes.json"
      Delete "$PROFILE\.openguild\plugin-consent.json"
      Delete "$PROFILE\.openguild\plugin-values.json"
      Delete "$PROFILE\.openguild\plugin-sources.json"
      Delete "$PROFILE\.openguild\openguild-server.toml"
      Delete "$PROFILE\.openguild\panic.log"
      RMDir /r "$PROFILE\.openguild\clipboard"
      ${If} ${FileExists} "$PROFILE\.openguild\plugin-data\*.*"
        DetailPrint "Kept $PROFILE\.openguild\plugin-data (files your plugins wrote)."
      ${EndIf}
    ${EndIf}
    ; 비어 있을 때만 지워진다(/r 없이).
    RMDir "$PROFILE\.openguild"
  ${EndIf}
!macroend
