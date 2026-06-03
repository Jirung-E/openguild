# DEV-075: 테스트 데이터 자동 주입 스크립트.
#
# 운영 규칙: `.guild/rules/test-data-script.md` 참조.
#
# 사용:
#   cd <빈 폴더>
#   pwsh -File <openguild repo>/scripts/seed-test-data.ps1
#
# 동작:
#   1. cwd 에 .guild 가 있으면 에러 후 종료. (실수 방지)
#   2. openguild init 실행.
#   3. 다양한 시간대 / 진행률 / 타입의 campaign + quest 데이터 주입.
#      Home 페이지의 carousel / conveyor / 최근 퀘스트 UI 를 한 번에 검증.
#   4. DEV-076: 일부 quest 에 희망/필수 기한 설정 — Home 의 "마감 임박" / Overdue
#      뱃지 검증.
#   5. DEV-016 (multi-file): sample 길드 규칙 생성 — Rules 페이지 검증.
#
# 환경:
#   - $env:OPENGUILD_BIN 으로 바이너리 경로 override 가능.
#   - 기본은 PATH 의 'openguild', 없으면 repo의 target/release/openguild.exe.

[CmdletBinding()]
param(
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
    # 우선순위: OPENGUILD_BIN > repo target/release > repo target/debug > PATH.
    # repo build 를 PATH 보다 우선. 시스템 설치본은 종종 outdated → campaign 같은
    # 신규 subcommand 누락. 개발 중에는 항상 최신 빌드 우선.
    if ($env:OPENGUILD_BIN) {
        if (-not (Test-Path $env:OPENGUILD_BIN)) {
            throw "OPENGUILD_BIN 지정됨이지만 파일이 없음: $($env:OPENGUILD_BIN)"
        }
        return $env:OPENGUILD_BIN
    }
    # $PSScriptRoot 는 함수 안에서도 스크립트 파일 경로를 가리킴 ($MyInvocation
    # 은 함수 본문을 반환해서 Split-Path 에 잘못된 문자 에러).
    $repoRoot = Split-Path -Parent $PSScriptRoot
    $candidate = Join-Path $repoRoot "target\release\openguild.exe"
    if (Test-Path $candidate) { return $candidate }
    $candidate = Join-Path $repoRoot "target\debug\openguild.exe"
    if (Test-Path $candidate) { return $candidate }
    $cmd = Get-Command openguild -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    throw "openguild 바이너리를 찾을 수 없음. PATH 등록하거나 OPENGUILD_BIN 환경변수 지정."
}

$bin = Resolve-OpenguildBin
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
Write-Host "`n=== [1/7] init ===" -ForegroundColor Green
Invoke-Og init --name $Name

# ── 2) Quest 생성 (다양한 타입 / 상태) ────────────────────────
Write-Host "`n=== [2/7] Quests ===" -ForegroundColor Green

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

# 일부는 상태 변경해서 다양성 확보.
Write-Host "`n=== [3/7] Quest 상태 전환 ===" -ForegroundColor Green
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
Write-Host "`n=== [4/7] Quest 기한 설정 (DEV-076) ===" -ForegroundColor Green
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
Write-Host "`n=== [5/7] Campaigns ===" -ForegroundColor Green

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
Write-Host "`n=== [6/7] Campaign ↔ Quest 연결 ===" -ForegroundColor Green
$campList = & $bin campaign list --status active --json 2>$null | ConvertFrom-Json
$questList = & $bin quest list --json 2>$null | ConvertFrom-Json

if ($campList.Count -ge 2 -and $questList.Count -ge 3) {
    Invoke-Og campaign link $campList[0].campaign_slug $questList[0].quest_id
    Invoke-Og campaign link $campList[0].campaign_slug $questList[1].quest_id
    Invoke-Og campaign link $campList[1].campaign_slug $questList[2].quest_id
}

# ── 7) DEV-016 (multi-file): sample 길드 규칙 (Rules 페이지 검증) ──
Write-Host "`n=== [7/7] 길드 규칙 (DEV-016 multi-file) ===" -ForegroundColor Green

# 짧은 sample 들 — 다중 파일 sidebar / 선택 / 편집 / 신규 / 이름변경 / 삭제
# 의 좌측 목록 정렬 / 선택 동작 검증. 본문은 의미 있는 minimal markdown 으로.
$ruleSamples = @{
    "branch-policy"   = "# 브랜치 정책`n`n- branch 이름 = quest_id.`n- ``feature/`` 같은 prefix 금지.`n- 머지된 feature 브랜치 삭제 금지 (히스토리 보존).`n- FF 가능하면 FF (``git merge`` 기본). ``--no-ff`` 강제 금지.`n"
    "code-review"     = "# 코드 리뷰 체크리스트`n`n- [ ] 새 quest 의 본문에 작업 의도 / 변경 사항 / 검증 명시.`n- [ ] ``cargo test`` / ``npm test`` / ``npm run check`` 통과.`n- [ ] 신규 migration 시 backward-compat 고려 (BUG-041 참조).`n- [ ] 사용자 노출 message 의 영/한 wording 확인.`n"
    "release-checklist" = "# 릴리즈 짧은 체크리스트`n`n자세한 절차는 ``release-process`` 참조.`n`n1. develop 의 testing → done 정리.`n2. 버전 동기화 6 파일.`n3. ``cargo tauri build`` 통과 확인.`n4. tag + GitHub release + ``latest.json`` attach.`n"
}

foreach ($slug in $ruleSamples.Keys) {
    # CLI 가 stdin 으로 본문 읽음.
    $body = $ruleSamples[$slug]
    Write-Host "[og] rules create $slug" -ForegroundColor DarkGray
    $body | & $bin rules create $slug
    if ($LASTEXITCODE -ne 0) { throw "rules create 실패: $slug" }
}

# ── 완료 요약 ────────────────────────────────────────────────
Write-Host "`n=== 완료 ===" -ForegroundColor Green
Write-Host "Guild   : $Name ($(Get-Location))"
Write-Host "Quests  : $($quests.Count + 0) 개 (목록 첫 10개만 Home 에 표시)"
Write-Host "Active  : $($activeCampaigns.Count) 개 캠페인 (carousel 회전)"
Write-Host "Upcoming: $($upcomingCampaigns.Count) 개 (1주 내 시작 — marquee 임계값 테스트)"
Write-Host "Future  : 1개 (1주 이후 fallback — 위 set 가 채우므로 노출은 안 됨)"
Write-Host "Due     : 일부 quest 에 과거/임박/미래 기한 — Home 임박 뱃지 / Overdue 검증."
Write-Host "Rules   : $($ruleSamples.Count) 개 sample (branch-policy / code-review / release-checklist)"
Write-Host ""
Write-Host "GUI 열어서 Home / Rules 페이지 확인:"
Write-Host "  cd `"$(Get-Location)`""
Write-Host "  openguild-gui  # 또는 설치된 OpenGuild 앱"

