#!/usr/bin/env node
// DEV-075/DEV-320: 테스트 데이터 자동 주입 스크립트 (Node 포트).
//
// 원래 scripts/seed-test-data.ps1 (PowerShell) 이었다 — pwsh 가 로컬 mac/linux
// 개발 환경엔 기본 설치돼 있지 않고(별도 설치 필요, 미문서화), 스크립트
// 자체에도 이식성 버그(`-BinDir` 지정 시 `.exe` 확장자 하드코딩 → mac/linux
// 바이너리는 확장자가 없어 실패, `$env:TEMP` 는 Windows 전용이라 mac/linux 에서
// 비어있음)가 있었다. Node 는 `gui/frontend` 빌드 때문에 이미 필수 의존성이라
// 새 설치가 불필요하고, 컴파일 스텝이 없어 (자주 손대는 스크립트인데) 고칠
// 때마다 재컴파일하는 비용도 없다.
//
// `scripts/extract-release-notes.ps1` 은 건드리지 않았다 — CI(check.yml/
// release.yml) 가 `shell: pwsh` 로 세 OS 러너 전부에서 이미 정상 동작 중이고
// (GitHub Actions 러너엔 pwsh 7 이 기본 설치), 로컬 개발자가 직접 돌릴 일이
// 없어 이식성 문제 자체가 없다.
//
// 운영 규칙: `.guild/rules/test-data-script.md` 참조.
//
// 사용:
//   cd <빈 폴더>
//   node <openguild repo>/scripts/seed-test-data.mjs
//   node .../seed-test-data.mjs /path/to/bin/dir            # 바이너리 위치 지정
//   node .../seed-test-data.mjs /path/to/bin/dir my-guild   # 길드 이름도 지정
//
// 동작:
//   1. cwd 에 .guild 가 있으면 에러 후 종료. (실수 방지)
//   2. openguild init 실행.
//   3. 다양한 시간대 / 진행률 / 타입의 campaign + quest 데이터 주입.
//      Home 페이지의 carousel / conveyor / 최근 퀘스트 UI 를 한 번에 검증.
//   4. DEV-076: 일부 quest 에 희망/필수 기한 설정 — Home 의 "마감 임박" / Overdue
//      뱃지 검증.
//   5. DEV-094/099/102: 첫 quest 에 댓글 (top + reply) + 메모 — DB 캐시 sync
//      + DEV-156/170: 첫 quest 에 첨부파일 1개 (본문 아래 첨부 섹션 데모)
//      + BUG-178: 토론 댓글에 답글 1개 — '토론만' 필터 + '전체 접기' 회귀.
//   6. DEV-016 (multi-file): sample 길드 규칙 생성 — Rules 페이지 검증.
//   7. DEV-288/290: 규칙/BOOK 변경 이력 — create/update/rename 을 일으켜
//      상세의 '변경 이력' 섹션 + rule/library history (CLI·서버·GUI) 검증.
//   8. DEV-306: 백업 스냅샷 1개 — 설정 > 백업 목록/복원 UI 검증 (스냅샷은
//      폴더가 아니라 파일 1개: `.guild/backups/snapshots/{ts}.db`).
//
// 바이너리 선택 (첫 위치 인자 = 바이너리 폴더):
//   - 인자 없음                → PATH 의 'openguild' 사용 (기본).
//   - node seed-test-data.mjs .          → 현재 폴더의 openguild(.exe).
//   - node seed-test-data.mjs <폴더>     → 그 폴더의 openguild(.exe).
//   PATH 설치본이 outdated 라 신규 subcommand(quest comment 등)가 없을 때 최신
//   빌드 위치를 직접 지정. (길드 이름은 둘째 인자로, 기본 'test-guild'.)
//
// 순수 Node 내장 모듈만 사용(child_process/fs/os/path) — npm install 불필요.

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';

const args = process.argv.slice(2);
const binDir = args[0] || '';
const name = args[1] || 'test-guild';

// ── 바이너리 경로 결정 ──────────────────────────────────────
function resolveOpenguildBin(dir) {
	// DEV-320 이식성 버그 수정: 예전엔 `openguild.exe` 로 고정 탐색해 mac/linux
	// (확장자 없음) 에서 항상 실패했다.
	const exeName = process.platform === 'win32' ? 'openguild.exe' : 'openguild';
	if (dir) {
		const candidate = path.join(dir, exeName);
		if (!fs.existsSync(candidate)) {
			throw new Error(`지정한 위치에 ${exeName} 가 없음: ${candidate}`);
		}
		return candidate;
	}
	const pathDirs = (process.env.PATH || '').split(path.delimiter);
	for (const d of pathDirs) {
		const candidate = path.join(d, exeName);
		if (fs.existsSync(candidate)) return candidate;
	}
	throw new Error(
		'PATH 에서 openguild 를 찾을 수 없음. 바이너리가 있는 폴더를 첫 인자로 지정하거나 PATH 에 등록.'
	);
}

const bin = resolveOpenguildBin(binDir);
console.log(`[seed] openguild binary: ${bin}`);

// ── 안전장치: 이미 초기화된 폴더에서는 실행 거부 ──────────────
if (fs.existsSync('.guild')) {
	console.error(
		`.guild 폴더가 이미 존재합니다. 이 스크립트는 빈 디렉토리에서만 실행 가능. (cwd: ${process.cwd()})`
	);
	process.exit(1);
}

// ── 헬퍼 ─────────────────────────────────────────────────────
function invokeOg(...cmdArgs) {
	console.log(`[og] ${cmdArgs.join(' ')}`);
	const res = spawnSync(bin, cmdArgs, { stdio: 'inherit' });
	if (res.status !== 0) {
		throw new Error(`openguild 명령 실패 (exit ${res.status}): ${cmdArgs.join(' ')}`);
	}
}

/** `--json` 붙여 호출하는 쪽 책임 — stdout 을 그대로 JSON 파싱. */
function invokeOgJson(...cmdArgs) {
	const res = spawnSync(bin, cmdArgs, { encoding: 'utf8' });
	if (res.status !== 0) {
		throw new Error(`openguild 명령 실패 (exit ${res.status}): ${cmdArgs.join(' ')}\n${res.stderr}`);
	}
	return JSON.parse(res.stdout);
}

/** 본문을 stdin 으로 흘려보내는 명령 (댓글/메모/규칙/템플릿 등). */
function invokeOgStdin(bodyText, ...cmdArgs) {
	console.log(`[og] ${cmdArgs.join(' ')}`);
	const res = spawnSync(bin, cmdArgs, { input: bodyText, encoding: 'utf8' });
	if (res.status !== 0) {
		throw new Error(`openguild 명령 실패 (exit ${res.status}): ${cmdArgs.join(' ')}\n${res.stderr}`);
	}
	return res.stdout;
}

function day(offset) {
	const d = new Date();
	d.setDate(d.getDate() + offset);
	return d.toISOString().slice(0, 10);
}

/** PowerShell 판의 `Start-Sleep -Milliseconds 50` — 생성 순서대로 timestamp
 * 가 벌어지게. `Atomics.wait` 는 워커가 아닌 메인 스레드에서도 Node 는 허용
 * (브라우저와 달리 UI 스레드 제약이 없음). */
function sleepMs(ms) {
	Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

// ── 1) init ─────────────────────────────────────────────────
console.log('\n=== [1/11] init ===');
invokeOg('init', '--name', name);

// ── 2) Quest 생성 (다양한 타입 / 상태) ────────────────────────
console.log('\n=== [2/11] Quests ===');

// 최근 추가된 퀘스트 목록 (Home 하단) 검증용. 10개 이상 만들어 slice(0, 10) 잘림 확인.
const questPlan = [
	{ type: 'DEV', title: 'API 엔드포인트 추가', urgency: 2 },
	{ type: 'DEV', title: '데이터베이스 스키마 변경', urgency: 3 },
	{ type: 'DEV', title: '리팩토링: 인증 모듈', urgency: 4 },
	{ type: 'BUG', title: '로그인 후 리다이렉트 실패', urgency: 1 },
	{ type: 'BUG', title: '모바일 메뉴 스크롤 잠김', urgency: 2 },
	{ type: 'BUG', title: '타임존 변환 오류', urgency: 3 },
	{ type: 'REQ', title: '다크 모드 토글 요청', urgency: 4 },
	{ type: 'REQ', title: 'PDF 내보내기 기능', urgency: 3 },
	{ type: 'DEV', title: '캐시 무효화 전략', urgency: 3 },
	{ type: 'DEV', title: 'CI 빌드 시간 최적화', urgency: 4 },
	{ type: 'BUG', title: '검색 결과 정렬 깨짐', urgency: 3 },
	{ type: 'DEV', title: 'WebSocket 재연결 로직', urgency: 2 }
];

for (const q of questPlan) {
	invokeOg('quest', 'new', '--type', q.type, '--title', q.title, '--urgency', String(q.urgency));
	sleepMs(50);
}

// DEV-140: 본문 cross-link 데모 — 위 퀘스트들을 [[ID]] 위키문법으로 참조.
// 실재 ID (DEV-001 / BUG-001) 는 파란 링크, 미존재 (DEV-404) 는 빨간 링크로
// MarkdownView 가 렌더하는지 확인용. 편집기에서 ID 타이핑 시 자동완성도 확인.
const xlinkDesc =
	'관련 작업: [[DEV-001]] 의 API 위에서 진행. [[BUG-001]] 리다이렉트 이슈와 연관. ' +
	'아직 없는 [[DEV-404]] 는 빨간 링크로 표시되어야 함.';
invokeOg(
	'quest', 'new',
	'--type', 'DEV',
	'--title', '본문 cross-link 데모 (DEV-140)',
	'--urgency', '3',
	'--description', xlinkDesc
);
sleepMs(50);

// 일부는 상태 변경해서 다양성 확보.
console.log('\n=== [3/11] Quest 상태 전환 ===');
// 가장 최신 슬러그를 모르므로 list 로 가져옴.
const quests = invokeOgJson('quest', 'list', '--json');
// 처음 2개는 in_progress, 다음 1개는 on_hold.
if (quests.length >= 3) {
	invokeOg('quest', 'move', quests[0].quest_id, 'in_progress');
	invokeOg('quest', 'move', quests[1].quest_id, 'in_progress');
	invokeOg('quest', 'move', quests[2].quest_id, 'on_hold');
}

// ── 4) DEV-076: 희망 / 필수 기한 (Home 임박 / Overdue 검증) ────
console.log('\n=== [4/11] Quest 기한 설정 (DEV-076) ===');
// Home 의 "마감 임박" 뱃지 / Overdue 표시 / 정렬 검증.
// - 과거 일자 (Overdue) 1개
// - 1~3일 내 (Critical 임박) 2개
// - 1주 이내 (Warning 임박) 2개
// - 미래 (정보성) 일부
if (quests.length >= 6) {
	// Overdue — 어제까지 필수.
	invokeOg('quest', 'due', quests[3].quest_id, '--required', day(-1));
	// Critical 임박 — 내일 / 모레.
	invokeOg('quest', 'due', quests[4].quest_id, '--required', day(1));
	invokeOg('quest', 'due', quests[5].quest_id, '--required', day(2));
	// Warning 임박 — 1주 이내.
	if (quests.length >= 8) {
		invokeOg('quest', 'due', quests[6].quest_id, '--required', day(5));
		invokeOg('quest', 'due', quests[7].quest_id, '--desired', day(6), '--required', day(10));
	}
	// 정보성 — 희망만 멀리.
	if (quests.length >= 10) {
		invokeOg('quest', 'due', quests[9].quest_id, '--desired', day(30));
	}
}

// ── 5) Campaign 생성 (Home carousel / conveyor 모두 검증) ────
console.log('\n=== [5/11] Campaigns ===');

// 진행 중 캠페인 (carousel): 5개 — 자동 회전 + dots / 화살표 검증.
const activeCampaigns = [
	{ title: '겨울 시즌 전체 점검', start: day(-10), end: day(5), progress: 0.4 },
	{ title: '보안 감사 1차', start: day(-5), end: day(2), progress: 0.8 },
	{ title: '성능 개선 스프린트', start: day(-20), end: day(10), progress: 0.25 },
	{ title: '문서화 작업', start: day(-3), end: day(14), progress: 1.0 }, // 100% → 초록
	{ title: '장기 마이그레이션', start: day(-30), end: day(60), progress: 0.5 }
];

// 곧 시작 캠페인 (conveyor): 1주 이내 시작 — marquee 임계값 검증.
// CARD_W=200 + GAP=12 → 6개 = 1272px. 1100px viewport → marquee 발동.
// 3개 = 636px → marquee X (정적).
const upcomingCampaigns = [
	{ title: '여름 시즌 캠페인', start: day(2), end: day(30) },
	{ title: '외부 보안 점검', start: day(3), end: day(7) },
	{ title: 'API v2 베타 테스트', start: day(4), end: day(20) },
	{ title: 'UI 리뉴얼 페이즈 1', start: day(5), end: day(40) },
	{ title: '사용자 인터뷰 라운드', start: day(6), end: day(13) },
	{ title: '오픈 베타 모집', start: day(6), end: day(21) },
	{ title: '마케팅 캠페인', start: day(7), end: day(28) }
];

// 곧 시작 fallback (1주 이상 뒤) — within 비어있을 때 가장 빠른 1개 표시 검증용
// 은 위 세트가 채우므로 생략. 미래 캠페인 1개만 보너스로.
const futureCampaign = { title: '내년 1분기 기획', start: day(30), end: day(120) };

function newCampaignWithChecklist(title, start, end, progress = 0.0, items = 5) {
	const obj = invokeOgJson('campaign', 'new', '--title', title, '--start', start, '--end', end, '--json');
	const slug = obj.campaign_slug;

	// 체크리스트 채움.
	for (let i = 1; i <= items; i++) {
		invokeOg('campaign', 'checklist', 'add', slug, `단계 ${i}`);
	}
	// 진행률에 맞춰 체크.
	const checkCount = Math.round(items * progress);
	for (let i = 1; i <= checkCount; i++) {
		invokeOg('campaign', 'checklist', 'check', slug, String(i));
	}
	// active 로 (campaign new 는 planned 로 만듦).
	invokeOg('campaign', 'start', slug);
	return slug;
}

for (const c of activeCampaigns) newCampaignWithChecklist(c.title, c.start, c.end, c.progress, 5);
for (const c of upcomingCampaigns) newCampaignWithChecklist(c.title, c.start, c.end, 0.0, 4);
newCampaignWithChecklist(futureCampaign.title, futureCampaign.start, futureCampaign.end, 0.0, 3);

// ── 6) 캠페인 ↔ 퀘스트 연결 (Quest Detail 의 Campaigns 섹션 검증) ──
console.log('\n=== [6/11] Campaign ↔ Quest 연결 ===');
const campList = invokeOgJson('campaign', 'list', '--status', 'active', '--json');
const questList = invokeOgJson('quest', 'list', '--json');

if (campList.length >= 2 && questList.length >= 3) {
	invokeOg('campaign', 'link', campList[0].campaign_slug, questList[0].quest_id);
	invokeOg('campaign', 'link', campList[0].campaign_slug, questList[1].quest_id);
	invokeOg('campaign', 'link', campList[1].campaign_slug, questList[2].quest_id);
}

// ── 6b) 관계 / 태그 / soft-delete / 템플릿 ──────────────────────
//   parent·prereq 가 없으면 보드 엣지 / 트리 모드 / 의존성 그래프 / candidates 가
//   빈 상태로 데모됨 — 핵심 시각화 검증용 관계를 만든다. 태그(칩/필터),
//   soft-delete(삭제목록/복원), 템플릿(NewQuestModal 드롭다운)도 함께 시딩.
console.log('\n=== [6b] 관계 / 태그 / soft-delete / 템플릿 ===');
if (questList.length >= 6) {
	// 하위 퀘스트 (Sub-quests / 트리 모드)
	invokeOg('quest', 'parent', questList[1].quest_id, questList[0].quest_id);
	invokeOg('quest', 'parent', questList[2].quest_id, questList[0].quest_id);
	// 선행 관계 (의존성 그래프 / 보드 엣지 / candidates)
	invokeOg('quest', 'prereq', 'add', questList[3].quest_id, questList[0].quest_id);
	invokeOg('quest', 'prereq', 'add', questList[4].quest_id, questList[3].quest_id);
	// 태그 (tag chip / 필터)
	invokeOg('quest', 'tag', 'add', questList[0].quest_id, 'backend', 'api');
	invokeOg('quest', 'tag', 'add', questList[3].quest_id, 'bug', 'regression');
	// soft delete 1개 (deleted 목록 / 복원 검증) — 가장 오래된 quest.
	invokeOg('quest', 'delete', questList[questList.length - 1].quest_id, '--yes');
}
// 템플릿 1개 — NewQuestModal 의 템플릿 드롭다운이 비어있지 않게 (DEV-060/158).
console.log('[og] template new bug-report');
invokeOgStdin(
	'## 재현 절차\n\n## 기대 / 실제\n',
	'template', 'new', 'bug-report', '--type', 'BUG', '--title', '[버그] ', '--urgency', '2'
);

// ── 7) DEV-099 / DEV-102: 댓글 + 메모 (CLI + DB cache sync) ──
console.log('\n=== [7/11] 댓글 / 메모 (DEV-094/099/102) ===');

// DEV-094 entry 단위 댓글 + 답글, DEV-099 CLI, DEV-102 DB 캐시 + snapshot 백업.
// Quest Detail 의 댓글 섹션 / 답글 / 메모 영역 + drift::auto_resync 도 검증.
const questForComments = questList[0]?.quest_id;
if (questForComments) {
	console.log(`[og] quest comment add ${questForComments} (alice / 최상위)`);
	invokeOgStdin(
		'이 캠페인의 진행 흐름 정리해보자.',
		'quest', 'comment', 'add', questForComments, '--author', 'alice'
	);

	// 답글 — add 직후라 부모 entry id 가 1.
	console.log('[og] quest comment add (bob / 답글)');
	invokeOgStdin(
		'동의. 다음 마일스톤 후 다시 보자.',
		'quest', 'comment', 'add', questForComments, '--author', 'bob', '--parent-id', '1'
	);

	// 메모 — set 으로 한 번에 본문 교체.
	console.log(`[og] quest memo set ${questForComments}`);
	invokeOgStdin('본인 한정 메모 — 검토 시 참고용.', 'quest', 'memo', 'set', questForComments);

	// DEV-156/170: 본문 아래 첨부 섹션 데모 — 임시 파일 1개를 첫 quest 에 첨부.
	console.log(`[og] quest attach add ${questForComments}`);
	const attachTmp = path.join(os.tmpdir(), 'openguild-seed-note.md');
	fs.writeFileSync(
		attachTmp,
		'# 첨부 데모\n\n시드 스크립트가 생성한 예시 첨부 파일 (DEV-156/170).\n',
		'utf8'
	);
	invokeOg('quest', 'attach', 'add', questForComments, attachTmp, '--name', 'seed-note.md');
	fs.rmSync(attachTmp, { force: true });

	// DEV-142/148/149: 토론 댓글 — 미해결 1개(홈 '토론 댓글' 섹션 + 완료 게이트)
	// + 해결 1개(✓ 표시). DEV-185 의 CLI discussion/resolved 토글 사용.
	console.log('[og] 토론 댓글 (discussion 미해결 + 해결)');
	invokeOgStdin(
		'설계 확정 전까지 완료 막아두자. (미해결 토론)',
		'quest', 'comment', 'add', questForComments, '--author', 'carol'
	);
	invokeOg('quest', 'comment', 'discussion', questForComments, '3'); // id 3 = 방금 추가 → 미해결 토론
	invokeOgStdin(
		'이건 합의됨 — 해결로 표시.',
		'quest', 'comment', 'add', questForComments, '--author', 'dave'
	);
	invokeOg('quest', 'comment', 'discussion', questForComments, '4'); // id 4 → 토론
	invokeOg('quest', 'comment', 'resolved', questForComments, '4'); // → 해결

	// BUG-178: '토론만' 필터를 켠 상태에서 '전체 접기' 회귀 데이터.
	// 토론 댓글(id 3) 아래에 답글이 있어야 접힘 여부를 눈으로 확인할 수 있다.
	console.log('[og] 토론 댓글 답글 (BUG-178 전체접기 회귀)');
	invokeOgStdin(
		'그럼 이 스레드에서 결론 내자. (토론 답글)',
		'quest', 'comment', 'add', questForComments, '--author', 'erin', '--parent-id', '3'
	);

	// DEV-069/156: 이미지 첨부 — 인라인 미리보기/임베드 경로 검증.
	// PS1 판은 System.Drawing 으로 PNG 를 직접 그렸으나 실패 시 1x1 PNG
	// base64 fallback 을 썼다 — Node 는 내장 이미지 인코더가 없어 그 fallback
	// 하나로 통일(결과물은 PS1 fallback 경로와 동일, 새 동작 아님).
	console.log('[og] 이미지 첨부 (PNG)');
	const imgTmp = path.join(os.tmpdir(), 'openguild-seed-image.png');
	fs.writeFileSync(
		imgTmp,
		Buffer.from(
			'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
			'base64'
		)
	);
	invokeOg('quest', 'attach', 'add', questForComments, imgTmp, '--name', 'preview.png');
	fs.rmSync(imgTmp, { force: true });

	// 비미디어 첨부파일 1개 더 (.json) — 다운로드 링크 아이콘 검증.
	console.log('[og] 추가 첨부파일 (.json)');
	const jsonTmp = path.join(os.tmpdir(), 'openguild-seed-config.json');
	fs.writeFileSync(jsonTmp, '{ "demo": true, "note": "시드 첨부 예시" }\n', 'utf8');
	invokeOg('quest', 'attach', 'add', questForComments, jsonTmp, '--name', 'config.json');
	fs.rmSync(jsonTmp, { force: true });
}

// 두 번째 quest 에도 댓글 + 메모 — 목록/홈 집계 다양성.
const secondQuest = questList.length >= 2 ? questList[1].quest_id : null;
if (secondQuest && secondQuest !== questForComments) {
	console.log(`[og] 두 번째 quest 댓글/메모 (${secondQuest})`);
	invokeOgStdin(
		'재현 단계 정리했습니다. 로그는 메모 참고.',
		'quest', 'comment', 'add', secondQuest, '--author', 'alice'
	);
	invokeOgStdin('메모: 관련 로그 / 재현 환경 기록 예정.', 'quest', 'memo', 'set', secondQuest);
}

// ── 8) DEV-016 (multi-file): sample 길드 규칙 (Rules 페이지 검증) ──
console.log('\n=== [8/11] 길드 규칙 (DEV-016 multi-file) ===');

// 짧은 sample 들 — 다중 파일 sidebar / 선택 / 편집 / 신규 / 이름변경 / 삭제
// 의 좌측 목록 정렬 / 선택 동작 검증. 본문은 의미 있는 minimal markdown 으로.
const ruleSamples = {
	'branch-policy':
		'# 브랜치 정책\n\n- branch 이름 = quest_id.\n- `feature/` 같은 prefix 금지.\n- 머지된 feature 브랜치 삭제 금지 (히스토리 보존).\n- FF 가능하면 FF (`git merge` 기본). `--no-ff` 강제 금지.\n',
	'code-review':
		'# 코드 리뷰 체크리스트\n\n- [ ] 새 quest 의 본문에 작업 의도 / 변경 사항 / 검증 명시.\n- [ ] `cargo test` / `npm test` / `npm run check` 통과.\n- [ ] 신규 migration 시 backward-compat 고려 (BUG-041 참조).\n- [ ] 사용자 노출 message 의 영/한 wording 확인.\n',
	'release-checklist':
		'# 릴리즈 짧은 체크리스트\n\n자세한 절차는 `release-process` 참조.\n\n1. develop 의 testing → done 정리.\n2. 버전 동기화 6 파일.\n3. `cargo tauri build` 통과 확인.\n4. tag + GitHub release + `latest.json` attach.\n'
};

for (const [slug, body] of Object.entries(ruleSamples)) {
	// CLI 가 stdin 으로 본문 읽음. DEV-231: top-level 은 `rule` 단수형만.
	console.log(`[og] rule new ${slug}`);
	invokeOgStdin(body, 'rule', 'new', slug);
}

// DEV-288/290: 규칙 변경 이력 데모 — create 만으론 이력이 1건이라 규칙 상세의
// '변경 이력' 섹션(.guild/history/{slug}.jsonl, 최신→과거)이 비어 보인다. 한
// 규칙에 create → update → rename 을 모두 일으켜 op 3종을 채운다. branch-policy
// 에도 update 1건.
console.log('[og] rule 변경 이력 데모 (DEV-288/290)');
invokeOgStdin(
	'# 브랜치 정책 (개정)\n\n- branch 이름 = quest_id.\n- 개정: FF-merge 원칙 명문화.\n',
	'rule', 'set', 'branch-policy'
);
invokeOgStdin('# 팀 컨벤션 초안', 'rule', 'new', 'history-demo');
invokeOgStdin('# 팀 컨벤션 초안 (수정)', 'rule', 'set', 'history-demo');
invokeOg('rule', 'rename', 'history-demo', 'team-conventions');

// ── 9) DEV-215~218, DEV-239: 도서관 (Library 페이지 + 폴더 + cross-link 검증) ──
console.log('\n=== [9/11] 도서관 (DEV-215~218, DEV-239) ===');

// BOOK-001: cross-link 대상 — quest 본문/댓글에서 [[BOOK-001]] 로 참조 검증.
// BOOK-002: 목록 정렬/선택 + 빈 본문 문서의 '+ 작성' 흐름 검증.
const bookBody1 = `# 설계 결정 기록\n\n프로젝트의 주요 설계 결정 모음.\n\n- 파일이 진리원, index.db 는 캐시.\n- 관련 quest: [[${questForComments}]]\n`;
const tmpBook = path.join(os.tmpdir(), `og-seed-book-${process.pid}.md`);
fs.writeFileSync(tmpBook, bookBody1, 'utf8');
console.log('[og] library new (BOOK-001)');
invokeOg('library', 'new', '--title', '설계 결정 기록', '--file', tmpBook);
fs.rmSync(tmpBook, { force: true });

console.log('[og] library new (BOOK-002, 빈 본문)');
invokeOg('library', 'new', '--title', '온보딩 가이드 (작성 예정)');

// 첫 quest 댓글에서 도서관 문서 참조 — [[BOOK-001]] 렌더/자동완성 검증.
invokeOgStdin(
	'참고 문서 정리함: [[BOOK-001]] 확인.',
	'quest', 'comment', 'add', questForComments, '--author', 'alice'
);

// DEV-239: 폴더 — 트리/탐색기 보기 토글, 폴더 안 문서 배치, 경로 기반
// cross-link 자동완성([[아키텍처/ 타이핑) 검증용.
console.log('[og] library folder new (아키텍처)');
invokeOg('library', 'folder', 'new', '아키텍처');

console.log('[og] library new (BOOK-003, 폴더 안)');
invokeOg('library', 'new', '--title', '라우터 설계', '--path', '아키텍처');

// DEV-288/290: 도서관 변경 이력 데모 — BOOK-001 제목을 한 번 수정해 문서 상세의
// '변경 이력' 섹션에 create → update 가 쌓이게. (규칙과 동일 사이드카 메커니즘.)
console.log('[og] library update (BOOK-001 변경 이력 데모, DEV-288/290)');
invokeOg('library', 'update', 'BOOK-001', '--title', '설계 결정 기록 (개정)');

// ── 10) DEV-167: 작업 기록 (HOME 히트맵 카드 + /worklog 상세 검증) ──
console.log('\n=== [10/11] 작업 기록 (DEV-167) ===');

// 활동(생성/상태변경/댓글)은 이 스크립트 실행 자체가 오늘 날짜로 잔뜩 만들어
// 놓음 — 히트맵의 오늘 칸 + 타임라인이 저절로 채워짐. 노트만 추가로:
// 오늘(일 뷰 기본) + 이틀 전(주 뷰의 일별 노트 나열 검증).
const today = day(0);
const past = day(-2);
const tmpNote = path.join(os.tmpdir(), `og-seed-note-${process.pid}.md`);
fs.writeFileSync(tmpNote, `시드 데이터 주입 완료. 세부는 [[${questForComments}]] 참고.`, 'utf8');
console.log(`[og] worklog note set ${today}`);
invokeOg('worklog', 'note', 'set', today, '--file', tmpNote);
fs.writeFileSync(tmpNote, '이틀 전 노트 — 주/월 뷰의 일별 노트 나열 검증용.', 'utf8');
invokeOg('worklog', 'note', 'set', past, '--file', tmpNote);
fs.rmSync(tmpNote, { force: true });

// ── 11) DEV-306: 백업 스냅샷 1개 ──────────────────────────────
console.log('\n=== [11/11] 백업 스냅샷 (DEV-306) ===');

// 설정 > 백업 화면이 빈 목록이면 복원/삭제 UI 를 볼 수 없다. 스냅샷 1개를 미리
// 만들어 둔다. DEV-306 이후 스냅샷은 폴더가 아니라 파일 1개(`snapshots/{ts}.db`)
// 이므로, 목록에 뜨는 크기/개수가 파일 기준으로 맞는지도 여기서 확인 가능.
invokeOg('backup', 'new');
const backupList = invokeOgJson('backup', 'list', '--json');
if (backupList.length < 1) {
	throw new Error('backup new 후에도 목록이 비어 있음');
}

// ── 완료 요약 ────────────────────────────────────────────────
console.log('\n=== 완료 ===');
console.log(`Guild   : ${name} (${process.cwd()})`);
console.log(`Quests  : ${quests.length} 개 (목록 첫 10개만 Home 에 표시)`);
console.log(`Active  : ${activeCampaigns.length} 개 캠페인 (carousel 회전)`);
console.log(`Upcoming: ${upcomingCampaigns.length} 개 (1주 내 시작 — marquee 임계값 테스트)`);
console.log('Future  : 1개 (1주 이후 fallback — 위 set 가 채우므로 노출은 안 됨)');
console.log('Due     : 일부 quest 에 과거/임박/미래 기한 — Home 임박 뱃지 / Overdue 검증.');
console.log(
	'Comments: 첫 quest 댓글 5 (top+reply+토론 미해결/해결+토론 답글) + 둘째 quest 댓글 1 — DB 캐시 sync.'
);
console.log('Memo    : 2 quest 에 메모.');
console.log(
	'토론    : 미해결 1 (홈 토론 섹션/완료 게이트) + 해결 1 (DEV-142/148/185) + 미해결 토론에 답글 1 (BUG-178).'
);
console.log(`Backup  : 스냅샷 1개 (${backupList.length}) — 설정 > 백업 목록/복원 (DEV-306, 파일 1개 형식).`);
console.log('Attach  : 첫 quest 에 3개 — .md / 이미지 .png(미리보기) / .json (DEV-156/170).');
console.log('관계    : 하위 2 + 선행 2 — 보드 엣지 / 트리 / 의존성 그래프 / candidates 검증.');
console.log('Tags    : 2 quest 에 태그 — 칩 / 필터 검증.');
console.log('Deleted : 1 quest soft-delete — 삭제 목록 / 복원 검증.');
console.log('Template: bug-report 1 개 — NewQuestModal 드롭다운 검증 (DEV-060/158).');
console.log(
	`Rules   : ${Object.keys(ruleSamples).length} 개 sample + team-conventions (branch-policy / code-review / release-checklist / team-conventions)`
);
console.log(
	'Library : 3 개 (BOOK-001 본문+cross-link / BOOK-002 빈 본문 / BOOK-003 폴더 안) + 폴더 1(아키텍처) + 댓글의 [[BOOK-001]] 참조.'
);
console.log(
	"History : 규칙/BOOK 변경 이력 데모 (DEV-288/290) — team-conventions(create→update→rename) / branch-policy(update) / BOOK-001(update). 상세의 '변경 이력' 섹션 + rule/library history CLI 검증."
);
console.log('Worklog : 노트 2 (오늘/이틀 전) — 활동은 이 스크립트 실행 자체가 오늘 날짜로 생성.');
console.log('');
console.log('GUI 열어서 Home / Rules 페이지 확인:');
console.log(`  cd "${process.cwd()}"`);
console.log('  openguild-gui  # 또는 설치된 OpenGuild 앱');
