<#
.SYNOPSIS
    BUG-171: CHANGELOG.md 에서 한 버전의 절을 뽑아 릴리스 노트 파일로 쓴다.

.DESCRIPTION
    `release.yml` 의 windows / linux 잡이 각각 릴리스를 만들거나 갱신하는데,
    둘 다 `generate_release_notes: true` 를 쓰면 나중 잡이 "기존 본문 + 새로
    생성한 노트" 로 덮어써서 같은 내용이 두 번 나온다(액션 규칙: body 가
    있으면 자동 생성 노트 앞에 붙는다). 본문을 **명시**하면 나중 잡이 같은
    내용으로 덮어쓰므로 잡 순서와 무관하게 항상 하나다.

    덤으로, 자동 생성 노트는 PR 목록 기반이라 PR 없이 직접 커밋하는 이 저장소
    에서는 compare 링크 한 줄만 남았다 — CHANGELOG 에 정리한 변경 내역이
    릴리스 페이지에 전혀 표시되지 않았다. 이 스크립트가 그 절을 그대로 싣는다.

    ubuntu 러너에도 pwsh 가 있으므로 두 잡이 이 스크립트 하나를 공용한다
    (OS 별로 추출 로직을 두 번 쓰지 않는다).

.PARAMETER Tag
    릴리스 태그 (예: `v0.4.1-beta`). 선행 `v` 는 떼고 CHANGELOG 헤딩과 맞춘다.

.PARAMETER Repo
    `owner/name` — compare 링크 생성용. 생략하면 링크를 붙이지 않는다.

.PARAMETER ChangelogPath
    기본 `CHANGELOG.md` (저장소 루트 기준).

.PARAMETER OutFile
    기본 `release-notes.md`.

.EXAMPLE
    pwsh scripts/extract-release-notes.ps1 -Tag v0.4.1-beta -Repo owner/repo
#>
param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [string]$Repo = '',
    [string]$ChangelogPath = 'CHANGELOG.md',
    [string]$OutFile = 'release-notes.md'
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $ChangelogPath)) {
    throw "CHANGELOG 없음: $ChangelogPath"
}

$version = $Tag -replace '^v', ''

# 상대 경로 → 현재 위치 기준 절대 경로. `[IO.Path]::GetFullPath(a, b)` 2-인자
# 오버로드는 .NET Framework(=Windows PowerShell 5.1)에 없어 쓰지 않는다.
function Resolve-FullPath([string]$p) {
    if ([System.IO.Path]::IsPathRooted($p)) { $p } else { Join-Path (Get-Location).Path $p }
}

# 인코딩 명시 — `Get-Content` 는 Windows PowerShell 5.1 에서 ANSI 코드페이지로
# 읽어 한글이 깨진다(CHANGELOG.md 는 BOM 없는 UTF-8). pwsh 7 은 UTF-8 이 기본
# 이라 CI 에서는 드러나지 않지만 로컬 실행에서 바로 재현된다.
$lines = [System.IO.File]::ReadAllLines(
    (Resolve-FullPath $ChangelogPath),
    (New-Object System.Text.UTF8Encoding($false))
)

# 버전 절의 시작/끝 줄 찾기. 헤딩 형식: `## 0.4.1-beta — 2026-07-26`
# (날짜는 없을 수도 있다 — `## 0.1.0-beta`). 다음 `## ` 헤딩 직전까지가 본문.
# 이어지는 버전 헤딩에서 이전 버전도 함께 얻는다(compare 링크용).
$start = -1
$end = $lines.Count
$prevVersion = $null
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -notmatch '^## ') { continue }
    $heading = $lines[$i].Substring(3).Trim()
    if ($start -lt 0) {
        # 헤딩이 정확히 이 버전으로 시작하는지 — `0.4.1-beta` 가 `0.4.10-beta`
        # 에 걸리지 않도록 뒤에 공백/구분자/끝만 허용.
        if ($heading -match ('^' + [regex]::Escape($version) + '(\s|$)')) {
            $start = $i
        }
        continue
    }
    # 이 버전 절 다음의 첫 헤딩 = 절의 끝 + 이전 버전.
    $end = $i
    if ($heading -notmatch '^Unreleased(\s|$)') {
        $prevVersion = ($heading -split '\s+')[0]
    }
    break
}

if ($start -lt 0) {
    throw "CHANGELOG 에 '## $version' 절이 없음 — 버전 범프/CHANGELOG 갱신 누락?"
}

# 헤딩 줄은 제외(릴리스 제목이 이미 태그다) + 앞뒤 빈 줄 정리.
$bodyLines = @()
if ($end -gt ($start + 1)) {
    $bodyLines = $lines[($start + 1)..($end - 1)]
}
$body = ($bodyLines -join "`n").Trim()

if ([string]::IsNullOrWhiteSpace($body)) {
    throw "'## $version' 절이 비어 있음 — 릴리스 노트 없이 배포하지 않는다"
}

# 절이 곧바로 `### Added` 로 시작하면 요약 문단이 없다는 뜻. 카테고리별 목록만
# 나열하면 릴리스 페이지에서 "이번 버전이 뭘 바꿨는지"가 안 읽힌다 — 사람이
# 직접 써야 하는 부분이라 자동 생성하지 않고 여기서 막는다(release-process 규칙).
if ($body -match '^###\s') {
    throw @"
'## $version' 절에 요약 문단이 없다 — 카테고리 목록(### …) 앞에 이번 릴리스의
요약을 먼저 쓸 것. 형식은 release-process 규칙 참고:
  1) 2~4줄 문단 — 이 릴리스가 무엇을 바꿨는지, 사용자 관점으로.
  2) '**주요 변경점**' + 3~5개 불릿 — 굵은 제목 + 무엇이 달라졌는지 + (퀘스트 ID).
"@
}

# 자동 생성 노트가 붙여주던 compare 링크를 직접 붙인다(이전 버전이 있을 때만).
if ($Repo -and $prevVersion) {
    $body += "`n`n**Full Changelog**: https://github.com/$Repo/compare/v$prevVersion...$Tag"
}

# BOM 없는 UTF-8 — GitHub 릴리스 본문에 BOM 이 그대로 보이지 않도록.
$outPath = Resolve-FullPath $OutFile
[System.IO.File]::WriteAllText(
    $outPath,
    $body + "`n",
    (New-Object System.Text.UTF8Encoding($false))
)

$prevLabel = if ($prevVersion) { $prevVersion } else { '없음' }
Write-Host "릴리스 노트 생성: $outPath ($($body.Length) chars, 이전 버전: $prevLabel)"
