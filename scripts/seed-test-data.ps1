# DEV-075: 테스트 데이터 자동 주입 스크립트.
#
# 운영 규칙: `.guild/rules/test-data-script.md` 참조.
#
# 사용:
#   cd <빈 폴더>
#   pwsh -File <openguild repo>/scripts/seed-test-data.ps1
#   pwsh -File ...\seed-test-data.ps1 -BinDir E:\home\workspace\lab   # 바이너리 위치 지정
#
# 동작:
#   1. cwd 에 .guild 가 있으면 에러 후 종료. (실수 방지)
#   2. openguild init 실행.
#   3. 다양한 시간대 / 진행률 / 타입의 campaign + quest 데이터 주입.
#      Home 페이지의 carousel / conveyor / 최근 퀘스트 UI 를 한 번에 검증.
#   4. DEV-076: 일부 quest 에 희망/필수 기한 설정 — Home 의 "마감 임박" / Overdue
#      뱃지 검증.
#   5. DEV-094/099/102: 첫 quest 에 댓글 (top + reply) + 메모 — DB 캐시 sync
#      + DEV-156/170: 첫 quest 에 첨부파일 1개 (본문 아래 첨부 섹션 데모)
#      + snapshot 백업 회귀.
#   6. DEV-016 (multi-file): sample 길드 규칙 생성 — Rules 페이지 검증.
#
# 바이너리 선택 (첫 위치 인자 = 바이너리 폴더):
#   - 인자 없음            → PATH 의 'openguild' 사용 (기본).
#   - .\seed-test-data.ps1 .          → 현재 폴더의 openguild.exe (스크립트와 동봉 시).
#   - .\seed-test-data.ps1 <폴더>     → 그 폴더의 openguild.exe.
#   - -BinDir <폴더> 로 명시 지정도 가능.
#   PATH 설치본이 outdated 라 신규 subcommand(quest comment 등)가 없을 때 최신 빌드
#   위치를 직접 지정. (길드 이름은 -Name 으로, 기본 'test-guild'.)

[CmdletBinding()]
param(
    # 바이너리(openguild.exe)가 들어있는 폴더. 첫 위치 인자. 미지정 시 PATH 사용.
    [Parameter(Position = 0)]
    [string]$BinDir = "",
    # 생성할 길드 이름. 이름을 줄 땐 -Name 으로 (positional 0 은 BinDir).
    [Parameter(Position = 1)]
    [string]$Name = "test-guild"
)

$ErrorActionPreference = "Stop"

# Windows PowerShell 5.1 은 native exe 의 stdout / stdin 을 OEM (cp949 등) 으로
# 처리. openguild 는 UTF-8 출력 / 입력. encoding 맞추지 않으면 한글 깨짐 +
# ConvertFrom-Json 실패. (pwsh 7 은 default UTF-8 이므로 no-op.)
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
try { & chcp 65001 | Out-Null } catch {}

# ── 바이너리 경로 결정 ────────────────────────────────────────
function Resolve-OpenguildBin {
    param([string]$BinDir)
    # -BinDir 인자가 있으면 그 폴더의 openguild.exe 를 사용. 없으면 PATH 의 openguild.
    if ($BinDir) {
        $candidate = Join-Path $BinDir "openguild.exe"
        if (-not (Test-Path $candidate)) {
            throw "지정한 위치에 openguild.exe 가 없음: $candidate"
        }
        return $candidate
    }
    $cmd = Get-Command openguild -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    throw "PATH 에서 openguild 를 찾을 수 없음. -BinDir <폴더> 로 위치를 지정하거나 PATH 에 등록."
}

$bin = Resolve-OpenguildBin -BinDir $BinDir
Write-Host "[seed] openguild binary: $bin" -ForegroundColor Cyan

# ── 안전장치: 이미 초기화된 폴더에서는 실행 거부 ──────────────
if (Test-Path ".guild") {
    Write-Error ".guild 폴더가 이미 존재합니다. 이 스크립트는 빈 디렉토리에서만 실행 가능. (cwd: $(Get-Location))"
    exit 1
}

# ── 헬퍼 ─────────────────────────────────────────────────────
function Invoke-Og {
    param([Parameter(ValueFromRemainingArguments)][string[]]$Args)
    Write-Host "[og] $($Args -join ' ')" -ForegroundColor DarkGray
    & $bin @Args
    if ($LASTEXITCODE -ne 0) {
        throw "openguild 명령 실패 (exit $LASTEXITCODE): $($Args -join ' ')"
    }
}

function Day { param([int]$Offset) (Get-Date).AddDays($Offset).ToString("yyyy-MM-dd") }

# ── 1) init ─────────────────────────────────────────────────
Write-Host "`n=== [1/8] init ===" -ForegroundColor Green
Invoke-Og init --name $Name

# ── 2) Quest 생성 (다양한 타입 / 상태) ────────────────────────
Write-Host "`n=== [2/8] Quests ===" -ForegroundColor Green

# 최근 추가된 퀘스트 목록 (Home 하단) 검증용. 10개 이상 만들어
# slice(0, 10) 잘림 확인.
$questPlan = @(
    @{ type = "DEV"; title = "API 엔드포인트 추가";   urgency = 2 },
    @{ type = "DEV"; title = "데이터베이스 스키마 변경"; urgency = 3 },
    @{ type = "DEV"; title = "리팩토링: 인증 모듈";    urgency = 4 },
    @{ type = "BUG"; title = "로그인 후 리다이렉트 실패"; urgency = 1 },
    @{ type = "BUG"; title = "모바일 메뉴 스크롤 잠김"; urgency = 2 },
    @{ type = "BUG"; title = "타임존 변환 오류";       urgency = 3 },
    @{ type = "REQ"; title = "다크 모드 토글 요청";    urgency = 4 },
    @{ type = "REQ"; title = "PDF 내보내기 기능";       urgency = 3 },
    @{ type = "DEV"; title = "캐시 무효화 전략";       urgency = 3 },
    @{ type = "DEV"; title = "CI 빌드 시간 최적화";    urgency = 4 },
    @{ type = "BUG"; title = "검색 결과 정렬 깨짐";    urgency = 3 },
    @{ type = "DEV"; title = "WebSocket 재연결 로직"; urgency = 2 }
)

$createdQuests = @()
foreach ($q in $questPlan) {
    Invoke-Og quest new --type $q.type --title $q.title --urgency $q.urgency
    Start-Sleep -Milliseconds 50
}

# DEV-140: 본문 cross-link 데모 — 위 퀘스트들을 [[ID]] 위키문법으로 참조.
# 실재 ID (DEV-001 / BUG-001) 는 파란 링크, 미존재 (DEV-404) 는 빨간 링크로
# MarkdownView 가 렌더하는지 확인용. 편집기에서 ID 타이핑 시 자동완성도 확인.
$xlinkDesc = "관련 작업: [[DEV-001]] 의 API 위에서 진행. [[BUG-001]] 리다이렉트 이슈와 연관. " +
    "아직 없는 [[DEV-404]] 는 빨간 링크로 표시되어야 함."
Invoke-Og quest new --type DEV --title "본문 cross-link 데모 (DEV-140)" --urgency 3 --description $xlinkDesc
Start-Sleep -Milliseconds 50

# 일부는 상태 변경해서 다양성 확보.
Write-Host "`n=== [3/8] Quest 상태 전환 ===" -ForegroundColor Green
# 가장 최신 슬러그를 모르므로 list 로 가져옴.
$listOut = & $bin quest list --json 2>$null
$quests = $listOut | ConvertFrom-Json
# 처음 2개는 in_progress, 다음 1개는 done.
if ($quests.Count -ge 3) {
    Invoke-Og quest move $quests[0].quest_id in_progress
    Invoke-Og quest move $quests[1].quest_id in_progress
    Invoke-Og quest move $quests[2].quest_id on_hold
}

# ── 4) DEV-076: 희망 / 필수 기한 (Home 임박 / Overdue 검증) ────
Write-Host "`n=== [4/8] Quest 기한 설정 (DEV-076) ===" -ForegroundColor Green
# Home 의 "마감 임박" 뱃지 / Overdue 표시 / 정렬 검증.
# - 과거 일자 (Overdue) 1개
# - 1~3일 내 (Critical 임박) 2개
# - 1주 이내 (Warning 임박) 2개
# - 미래 (정보성) 일부
if ($quests.Count -ge 6) {
    # Overdue — 어제까지 필수.
    Invoke-Og quest due $quests[3].quest_id --required (Day -1)
    # Critical 임박 — 내일 / 모레.
    Invoke-Og quest due $quests[4].quest_id --required (Day 1)
    Invoke-Og quest due $quests[5].quest_id --required (Day 2)
    # Warning 임박 — 1주 이내.
    if ($quests.Count -ge 8) {
        Invoke-Og quest due $quests[6].quest_id --required (Day 5)
        Invoke-Og quest due $quests[7].quest_id --desired (Day 6) --required (Day 10)
    }
    # 정보성 — 희망만 멀리.
    if ($quests.Count -ge 10) {
        Invoke-Og quest due $quests[9].quest_id --desired (Day 30)
    }
}

# ── 5) Campaign 생성 (Home carousel / conveyor 모두 검증) ────
Write-Host "`n=== [5/8] Campaigns ===" -ForegroundColor Green

# 진행 중 캠페인 (carousel): 5개 — 자동 회전 + dots / 화살표 검증.
$activeCampaigns = @(
    @{ title = "겨울 시즌 전체 점검";   start = Day -10; end = Day 5;   progress = 0.4 },
    @{ title = "보안 감사 1차";         start = Day -5;  end = Day 2;   progress = 0.8 },
    @{ title = "성능 개선 스프린트";    start = Day -20; end = Day 10;  progress = 0.25 },
    @{ title = "문서화 작업";           start = Day -3;  end = Day 14;  progress = 1.0 }, # 100% → 초록
    @{ title = "장기 마이그레이션";     start = Day -30; end = Day 60;  progress = 0.5 }
)

# 곧 시작 캠페인 (conveyor): 1주 이내 시작 — marquee 임계값 검증.
# CARD_W=200 + GAP=12 → 6개 = 1272px. 1100px viewport → marquee 발동.
# 3개 = 636px → marquee X (정적).
$upcomingCampaigns = @(
    @{ title = "여름 시즌 캠페인";       start = Day 2;  end = Day 30 },
    @{ title = "외부 보안 점검";         start = Day 3;  end = Day 7 },
    @{ title = "API v2 베타 테스트";     start = Day 4;  end = Day 20 },
    @{ title = "UI 리뉴얼 페이즈 1";     start = Day 5;  end = Day 40 },
    @{ title = "사용자 인터뷰 라운드";    start = Day 6;  end = Day 13 },
    @{ title = "오픈 베타 모집";         start = Day 6;  end = Day 21 },
    @{ title = "마케팅 캠페인";          start = Day 7;  end = Day 28 }
)

# 곧 시작 fallback (1주 이상 뒤) — within 비어있을 때 가장 빠른 1개 표시 검증용
# 은 위 세트가 채우므로 생략. 미래 캠페인 1개만 보너스로.
$futureCampaign = @{ title = "내년 1분기 기획"; start = Day 30; end = Day 120 }

function New-CampaignWithChecklist {
    param([string]$Title, [string]$Start, [string]$End, [double]$Progress = 0.0, [int]$Items = 5)

    $json = & $bin campaign new --title $Title --start $Start --end $End --json 2>$null
    if ($LASTEXITCODE -ne 0) { throw "campaign new 실패: $Title" }
    $obj = $json | ConvertFrom-Json
    $slug = $obj.campaign_slug

    # 체크리스트 채움.
    for ($i = 1; $i -le $Items; $i++) {
        Invoke-Og campaign checklist add $slug "단계 $i"
    }
    # 진행률에 맞춰 체크.
    $checkCount = [Math]::Round($Items * $Progress)
    for ($i = 1; $i -le $checkCount; $i++) {
        Invoke-Og campaign checklist check $slug $i
    }
    # active 로 (campaign new 는 planned 로 만듦).
    Invoke-Og campaign start $slug
    return $slug
}

foreach ($c in $activeCampaigns) {
    New-CampaignWithChecklist -Title $c.title -Start $c.start -End $c.end -Progress $c.progress -Items 5 | Out-Null
}
foreach ($c in $upcomingCampaigns) {
    New-CampaignWithChecklist -Title $c.title -Start $c.start -End $c.end -Progress 0.0 -Items 4 | Out-Null
}
New-CampaignWithChecklist -Title $futureCampaign.title -Start $futureCampaign.start -End $futureCampaign.end -Progress 0.0 -Items 3 | Out-Null

# ── 6) 캠페인 ↔ 퀘스트 연결 (Quest Detail 의 Campaigns 섹션 검증) ──
Write-Host "`n=== [6/8] Campaign ↔ Quest 연결 ===" -ForegroundColor Green
$campList = & $bin campaign list --status active --json 2>$null | ConvertFrom-Json
$questList = & $bin quest list --json 2>$null | ConvertFrom-Json

if ($campList.Count -ge 2 -and $questList.Count -ge 3) {
    Invoke-Og campaign link $campList[0].campaign_slug $questList[0].quest_id
    Invoke-Og campaign link $campList[0].campaign_slug $questList[1].quest_id
    Invoke-Og campaign link $campList[1].campaign_slug $questList[2].quest_id
}

# ── 6b) 관계 / 태그 / soft-delete / 템플릿 ──────────────────────
#   parent·prereq 가 없으면 보드 엣지 / 트리 모드 / 의존성 그래프 / candidates 가
#   빈 상태로 데모됨 — 핵심 시각화 검증용 관계를 만든다. 태그(칩/필터),
#   soft-delete(삭제목록/복원), 템플릿(NewQuestModal 드롭다운)도 함께 시딩.
Write-Host "`n=== [6b] 관계 / 태그 / soft-delete / 템플릿 ===" -ForegroundColor Green
if ($questList.Count -ge 6) {
    # 하위 퀘스트 (Sub-quests / 트리 모드)
    Invoke-Og quest parent $questList[1].quest_id $questList[0].quest_id
    Invoke-Og quest parent $questList[2].quest_id $questList[0].quest_id
    # 선행 관계 (의존성 그래프 / 보드 엣지 / candidates)
    Invoke-Og quest prereq add $questList[3].quest_id $questList[0].quest_id
    Invoke-Og quest prereq add $questList[4].quest_id $questList[3].quest_id
    # 태그 (tag chip / 필터)
    Invoke-Og quest tag add $questList[0].quest_id backend api
    Invoke-Og quest tag add $questList[3].quest_id bug regression
    # soft delete 1개 (deleted 목록 / 복원 검증) — 가장 오래된 quest.
    Invoke-Og quest delete $questList[$questList.Count - 1].quest_id --yes
}
# 템플릿 1개 — NewQuestModal 의 템플릿 드롭다운이 비어있지 않게 (DEV-060/158).
Write-Host "[og] template new bug-report" -ForegroundColor DarkGray
"## 재현 절차`n`n## 기대 / 실제`n" | & $bin template new bug-report --type BUG --title "[버그] " --urgency 2
if ($LASTEXITCODE -ne 0) { throw "template new 실패" }

# ── 7) DEV-099 / DEV-102: 댓글 + 메모 (CLI + DB cache sync) ──
Write-Host "`n=== [7/8] 댓글 / 메모 (DEV-094/099/102) ===" -ForegroundColor Green

# DEV-094 entry 단위 댓글 + 답글, DEV-099 CLI, DEV-102 DB 캐시 + snapshot 백업.
# Quest Detail 의 댓글 섹션 / 답글 / 메모 영역 + drift::auto_resync 도 검증.
$questForComments = ($questList | Select-Object -First 1).quest_id
if ($questForComments) {
    Write-Host "[og] quest comment add $questForComments (alice / 최상위)" -ForegroundColor DarkGray
    "이 캠페인의 진행 흐름 정리해보자." | & $bin quest comment add $questForComments --author alice
    if ($LASTEXITCODE -ne 0) { throw "quest comment add 실패" }

    # 답글 — add 직후라 부모 entry id 가 1.
    Write-Host "[og] quest comment add (bob / 답글)" -ForegroundColor DarkGray
    "동의. 다음 마일스톤 후 다시 보자." | & $bin quest comment add $questForComments --author bob --parent-id 1
    if ($LASTEXITCODE -ne 0) { throw "quest comment add (reply) 실패" }

    # 메모 — set 으로 한 번에 본문 교체.
    Write-Host "[og] quest memo set $questForComments" -ForegroundColor DarkGray
    "본인 한정 메모 — 검토 시 참고용." | & $bin quest memo set $questForComments
    if ($LASTEXITCODE -ne 0) { throw "quest memo set 실패" }

    # DEV-156/170: 본문 아래 첨부 섹션 데모 — 임시 파일 1개를 첫 quest 에 첨부.
    Write-Host "[og] quest attach add $questForComments" -ForegroundColor DarkGray
    $attachTmp = Join-Path ([System.IO.Path]::GetTempPath()) "openguild-seed-note.md"
    "# 첨부 데모`n`n시드 스크립트가 생성한 예시 첨부 파일 (DEV-156/170)." | Out-File -Encoding utf8 $attachTmp
    Invoke-Og quest attach add $questForComments $attachTmp --name "seed-note.md"
    Remove-Item $attachTmp -ErrorAction SilentlyContinue

    # DEV-142/148/149: 토론 댓글 — 미해결 1개(홈 '토론 댓글' 섹션 + 완료 게이트)
    # + 해결 1개(✓ 표시). DEV-185 의 CLI discussion/resolved 토글 사용.
    Write-Host "[og] 토론 댓글 (discussion 미해결 + 해결)" -ForegroundColor DarkGray
    "설계 확정 전까지 완료 막아두자. (미해결 토론)" | & $bin quest comment add $questForComments --author carol
    if ($LASTEXITCODE -ne 0) { throw "discussion comment add 실패" }
    Invoke-Og quest comment discussion $questForComments 3   # id 3 = 방금 추가 → 미해결 토론
    "이건 합의됨 — 해결로 표시." | & $bin quest comment add $questForComments --author dave
    if ($LASTEXITCODE -ne 0) { throw "resolved comment add 실패" }
    Invoke-Og quest comment discussion $questForComments 4   # id 4 → 토론
    Invoke-Og quest comment resolved $questForComments 4     # → 해결

    # DEV-069/156: 이미지 첨부 — 인라인 미리보기/임베드 경로 검증 (PNG 생성).
    Write-Host "[og] 이미지 첨부 (PNG)" -ForegroundColor DarkGray
    $imgTmp = Join-Path ([System.IO.Path]::GetTempPath()) "openguild-seed-image.png"
    try {
        Add-Type -AssemblyName System.Drawing -ErrorAction Stop
        $bmp = New-Object System.Drawing.Bitmap 64, 64
        $gfx = [System.Drawing.Graphics]::FromImage($bmp)
        $gfx.Clear([System.Drawing.Color]::SteelBlue)
        $gfx.Dispose(); $bmp.Save($imgTmp, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
    } catch {
        # System.Drawing 불가 환경 — 1x1 PNG base64 fallback.
        [IO.File]::WriteAllBytes($imgTmp, [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="))
    }
    Invoke-Og quest attach add $questForComments $imgTmp --name "preview.png"
    Remove-Item $imgTmp -ErrorAction SilentlyContinue

    # 비미디어 첨부파일 1개 더 (.json) — 다운로드 링크 아이콘 검증.
    Write-Host "[og] 추가 첨부파일 (.json)" -ForegroundColor DarkGray
    $jsonTmp = Join-Path ([System.IO.Path]::GetTempPath()) "openguild-seed-config.json"
    '{ "demo": true, "note": "시드 첨부 예시" }' | Out-File -Encoding utf8 $jsonTmp
    Invoke-Og quest attach add $questForComments $jsonTmp --name "config.json"
    Remove-Item $jsonTmp -ErrorAction SilentlyContinue
}

# 두 번째 quest 에도 댓글 + 메모 — 목록/홈 집계 다양성.
$secondQuest = if ($questList.Count -ge 2) { $questList[1].quest_id } else { $null }
if ($secondQuest -and $secondQuest -ne $questForComments) {
    Write-Host "[og] 두 번째 quest 댓글/메모 ($secondQuest)" -ForegroundColor DarkGray
    "재현 단계 정리했습니다. 로그는 메모 참고." | & $bin quest comment add $secondQuest --author alice
    if ($LASTEXITCODE -ne 0) { throw "second quest comment 실패" }
    "메모: 관련 로그 / 재현 환경 기록 예정." | & $bin quest memo set $secondQuest
    if ($LASTEXITCODE -ne 0) { throw "second quest memo 실패" }
}

# ── 8) DEV-016 (multi-file): sample 길드 규칙 (Rules 페이지 검증) ──
Write-Host "`n=== [8/10] 길드 규칙 (DEV-016 multi-file) ===" -ForegroundColor Green

# 짧은 sample 들 — 다중 파일 sidebar / 선택 / 편집 / 신규 / 이름변경 / 삭제
# 의 좌측 목록 정렬 / 선택 동작 검증. 본문은 의미 있는 minimal markdown 으로.
$ruleSamples = @{
    "branch-policy"   = "# 브랜치 정책`n`n- branch 이름 = quest_id.`n- ``feature/`` 같은 prefix 금지.`n- 머지된 feature 브랜치 삭제 금지 (히스토리 보존).`n- FF 가능하면 FF (``git merge`` 기본). ``--no-ff`` 강제 금지.`n"
    "code-review"     = "# 코드 리뷰 체크리스트`n`n- [ ] 새 quest 의 본문에 작업 의도 / 변경 사항 / 검증 명시.`n- [ ] ``cargo test`` / ``npm test`` / ``npm run check`` 통과.`n- [ ] 신규 migration 시 backward-compat 고려 (BUG-041 참조).`n- [ ] 사용자 노출 message 의 영/한 wording 확인.`n"
    "release-checklist" = "# 릴리즈 짧은 체크리스트`n`n자세한 절차는 ``release-process`` 참조.`n`n1. develop 의 testing → done 정리.`n2. 버전 동기화 6 파일.`n3. ``cargo tauri build`` 통과 확인.`n4. tag + GitHub release + ``latest.json`` attach.`n"
}

foreach ($slug in $ruleSamples.Keys) {
    # CLI 가 stdin 으로 본문 읽음. DEV-231: top-level 은 `rule` 단수형만.
    $body = $ruleSamples[$slug]
    Write-Host "[og] rule new $slug" -ForegroundColor DarkGray
    $body | & $bin rule new $slug
    if ($LASTEXITCODE -ne 0) { throw "rule new 실패: $slug" }
}

# ── 9) DEV-215~218, DEV-239: 도서관 (Library 페이지 + 폴더 + cross-link 검증) ──
Write-Host "`n=== [9/10] 도서관 (DEV-215~218, DEV-239) ===" -ForegroundColor Green

# BOOK-001: cross-link 대상 — quest 본문/댓글에서 [[BOOK-001]] 로 참조 검증.
# BOOK-002: 목록 정렬/선택 + 빈 본문 문서의 '+ 작성' 흐름 검증.
$bookBody1 = @"
# 설계 결정 기록

프로젝트의 주요 설계 결정 모음.

- 파일이 진리원, index.db 는 캐시.
- 관련 quest: [[$questForComments]]
"@
$tmpBook = Join-Path $env:TEMP "og-seed-book-$PID.md"
Set-Content -Path $tmpBook -Value $bookBody1 -Encoding utf8
Write-Host "[og] library new (BOOK-001)" -ForegroundColor DarkGray
& $bin library new --title "설계 결정 기록" --file $tmpBook | Out-Null
if ($LASTEXITCODE -ne 0) { throw "library new 실패 (1)" }
Remove-Item $tmpBook -Force -ErrorAction SilentlyContinue

Write-Host "[og] library new (BOOK-002, 빈 본문)" -ForegroundColor DarkGray
& $bin library new --title "온보딩 가이드 (작성 예정)" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "library new 실패 (2)" }

# 첫 quest 댓글에서 도서관 문서 참조 — [[BOOK-001]] 렌더/자동완성 검증.
"참고 문서 정리함: [[BOOK-001]] 확인." | & $bin quest comment add $questForComments --author alice
if ($LASTEXITCODE -ne 0) { throw "book cross-link comment 실패" }

# DEV-239: 폴더 — 트리/탐색기 보기 토글, 폴더 안 문서 배치, 경로 기반
# cross-link 자동완성([[아키텍처/ 타이핑) 검증용.
Write-Host "[og] library folder new (아키텍처)" -ForegroundColor DarkGray
& $bin library folder new "아키텍처" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "library folder new 실패" }

Write-Host "[og] library new (BOOK-003, 폴더 안)" -ForegroundColor DarkGray
& $bin library new --title "라우터 설계" --path "아키텍처" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "library new 실패 (3, path)" }

# ── 10) DEV-167: 작업 기록 (HOME 히트맵 카드 + /worklog 상세 검증) ──
Write-Host "`n=== [10/10] 작업 기록 (DEV-167) ===" -ForegroundColor Green

# 활동(생성/상태변경/댓글)은 이 스크립트 실행 자체가 오늘 날짜로 잔뜩 만들어
# 놓음 — 히트맵의 오늘 칸 + 타임라인이 저절로 채워짐. 노트만 추가로:
# 오늘(일 뷰 기본) + 이틀 전(주 뷰의 일별 노트 나열 검증).
$today = Get-Date -Format "yyyy-MM-dd"
$past = (Get-Date).AddDays(-2).ToString("yyyy-MM-dd")
$tmpNote = Join-Path $env:TEMP "og-seed-note-$PID.md"
Set-Content -Path $tmpNote -Value "시드 데이터 주입 완료. 세부는 [[$questForComments]] 참고." -Encoding utf8
Write-Host "[og] worklog note set $today" -ForegroundColor DarkGray
& $bin worklog note set $today --file $tmpNote | Out-Null
if ($LASTEXITCODE -ne 0) { throw "worklog note set 실패 (today)" }
Set-Content -Path $tmpNote -Value "이틀 전 노트 — 주/월 뷰의 일별 노트 나열 검증용." -Encoding utf8
& $bin worklog note set $past --file $tmpNote | Out-Null
if ($LASTEXITCODE -ne 0) { throw "worklog note set 실패 (past)" }
Remove-Item $tmpNote -Force -ErrorAction SilentlyContinue

# ── 완료 요약 ────────────────────────────────────────────────
Write-Host "`n=== 완료 ===" -ForegroundColor Green
Write-Host "Guild   : $Name ($(Get-Location))"
Write-Host "Quests  : $($quests.Count + 0) 개 (목록 첫 10개만 Home 에 표시)"
Write-Host "Active  : $($activeCampaigns.Count) 개 캠페인 (carousel 회전)"
Write-Host "Upcoming: $($upcomingCampaigns.Count) 개 (1주 내 시작 — marquee 임계값 테스트)"
Write-Host "Future  : 1개 (1주 이후 fallback — 위 set 가 채우므로 노출은 안 됨)"
Write-Host "Due     : 일부 quest 에 과거/임박/미래 기한 — Home 임박 뱃지 / Overdue 검증."
Write-Host "Comments: 첫 quest 댓글 4 (top+reply+토론 미해결/해결) + 둘째 quest 댓글 1 — DB 캐시 sync."
Write-Host "Memo    : 2 quest 에 메모."
Write-Host "토론    : 미해결 1 (홈 토론 섹션/완료 게이트) + 해결 1 (DEV-142/148/185)."
Write-Host "Attach  : 첫 quest 에 3개 — .md / 이미지 .png(미리보기) / .json (DEV-156/170)."
Write-Host "관계    : 하위 2 + 선행 2 — 보드 엣지 / 트리 / 의존성 그래프 / candidates 검증."
Write-Host "Tags    : 2 quest 에 태그 — 칩 / 필터 검증."
Write-Host "Deleted : 1 quest soft-delete — 삭제 목록 / 복원 검증."
Write-Host "Template: bug-report 1 개 — NewQuestModal 드롭다운 검증 (DEV-060/158)."
Write-Host "Rules   : $($ruleSamples.Count) 개 sample (branch-policy / code-review / release-checklist)"
Write-Host "Library : 3 개 (BOOK-001 본문+cross-link / BOOK-002 빈 본문 / BOOK-003 폴더 안) + 폴더 1(아키텍처) + 댓글의 [[BOOK-001]] 참조."
Write-Host "Worklog : 노트 2 (오늘/이틀 전) — 활동은 이 스크립트 실행 자체가 오늘 날짜로 생성."
Write-Host ""
Write-Host "GUI 열어서 Home / Rules 페이지 확인:"
Write-Host "  cd `"$(Get-Location)`""
Write-Host "  openguild-gui  # 또는 설치된 OpenGuild 앱"

