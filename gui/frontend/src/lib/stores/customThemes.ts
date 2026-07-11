// DEV-114: 커스텀 테마 — 사용자가 토큰 색을 자유 정의 / 프리셋 저장.
//
// 저장 결정(계획 댓글 #2): `.guild/theme.toml` 이 아닌 **localStorage** —
// 테마는 개인 취향(같은 길드를 다크로 보는 사람/커스텀으로 보는 사람 공존)
// 이라 길드 파일에 넣으면 멀티유저에서 충돌. 프리셋 공유는 export/import
// (JSON 텍스트)가 커버.
//
// 동작 모델:
// - 프리셋 = { name, base: 'dark'|'light', overrides: { '--토큰': '#hex' } }.
// - 활성화 시 `setTheme(base)` 로 기존 테마 경로(<html data-theme>,
//   effectiveTheme, CSS 토큰 기본값)를 그대로 태우고, 그 **위에**
//   documentElement inline style 로 override 만 얹는다 — 기존 테마 시스템
//   (DEV-074)을 변형하지 않는 최소 침습.
// - Quest Board(Cytoscape/SVG)는 CSS var() 를 못 쓰고 themePalette() hex 를
//   직접 읽으므로(DEV-074 fix20), 활성화 시 setPaletteOverrides() 로 같은
//   override 를 palette 에도 주입. 보드는 mount 시 palette 를 읽으므로
//   설정 변경 후 보드 재진입 시 반영(열린 보드의 라이브 갱신은 base 전환
//   이벤트에만 반응 — MVP 한계, 필요 시 후속).
//
// no-hex 규칙과의 관계: 사용자가 고른 hex 는 "데이터"(CSS 소스 아님) —
// check-no-hex 는 컴포넌트 CSS 만 검사하므로 충돌 없음.

import { writable, get } from 'svelte/store';
import {
	setTheme,
	setPaletteOverrides,
	type ThemePalette,
	type EffectiveTheme
} from './theme';

/** 사용자 노출 토큰 정의. palette = themePalette 필드 매핑(보드 연동용). */
export interface TokenDef {
	token: string;
	label: string;
	palette?: keyof ThemePalette;
	/** true = "고급" 토글을 켜야 노출. 기본 노출은 핵심 ~12개만. */
	advanced?: boolean;
}

// 카탈로그 제약: global.css 에 **리터럴 hex** 로 정의된 토큰만 —
// var()/color-mix 파생 토큰(--btn-primary-* 등)은 getComputedStyle 로
// 원시값을 못 읽어 color picker 초기값을 만들 수 없다. 파생 토큰은 원본
// (--success-strong 등)을 바꾸면 따라온다.
export const TOKEN_CATALOG: TokenDef[] = [
	// ─── 핵심 (기본 노출) ───
	{ token: '--bg', label: '배경', palette: 'bg' },
	{ token: '--bg-elevated', label: '배경 (카드)', palette: 'bgElevated' },
	{ token: '--bg-subtle', label: '배경 (강조)' },
	{ token: '--border', label: '테두리' },
	{ token: '--text', label: '텍스트', palette: 'text' },
	{ token: '--text-muted', label: '텍스트 (보조)', palette: 'textMuted' },
	{ token: '--accent', label: '강조색', palette: 'accent' },
	{ token: '--success', label: '성공', palette: 'success' },
	{ token: '--success-strong', label: '성공 (진함/주 버튼)', palette: 'successStrong' },
	{ token: '--warning', label: '경고', palette: 'warning' },
	{ token: '--danger', label: '위험', palette: 'danger' },
	{ token: '--nav-bg', label: '내비게이션 배경' },
	// ─── 고급 ───
	{ token: '--text-strong', label: '텍스트 (진함)', advanced: true },
	{ token: '--text-faint', label: '텍스트 (희미)', palette: 'textFaint', advanced: true },
	{ token: '--border-muted', label: '테두리 (연함)', advanced: true },
	{ token: '--accent-strong', label: '강조색 (진함)', advanced: true },
	{ token: '--accent-secondary', label: '강조색 (보조)', palette: 'accentSecondary', advanced: true },
	{ token: '--orange', label: '오렌지', palette: 'orange', advanced: true },
	{ token: '--nav-border', label: '내비게이션 테두리', advanced: true },
	{ token: '--nav-hover-bg', label: '내비게이션 hover', advanced: true },
	{ token: '--hl-pre', label: '보드: 선행 강조', palette: 'hlPre', advanced: true },
	{ token: '--hl-pre-bg', label: '보드: 선행 배경', palette: 'hlPreBg', advanced: true },
	{ token: '--hl-sub', label: '보드: 서브 강조', palette: 'hlSub', advanced: true },
	{ token: '--hl-sub-bg', label: '보드: 서브 배경', palette: 'hlSubBg', advanced: true },
	{ token: '--hl-next', label: '보드: 후속 강조', palette: 'hlNext', advanced: true },
	{ token: '--hl-next-bg', label: '보드: 후속 배경', palette: 'hlNextBg', advanced: true },
	{ token: '--hl-parent-bg', label: '보드: 부모 배경', palette: 'hlParentBg', advanced: true },
	{ token: '--selected-bg', label: '보드: 선택 배경', palette: 'selectedBg', advanced: true },
	{ token: '--edge-pre', label: '보드: 의존 엣지', palette: 'edgePre', advanced: true }
];

export interface CustomTheme {
	name: string;
	base: EffectiveTheme;
	/** '--토큰' → '#rrggbb'. 없는 토큰은 base 기본값 그대로. */
	overrides: Record<string, string>;
}

const PRESETS_KEY = 'openguild.customThemes';
const ACTIVE_KEY = 'openguild.activeCustomTheme';

// ─── DEV-249: Tauri 모드는 ~/.openguild/themes.json 파일이 진리원 ───
// localStorage(WebView2 LevelDB)는 사람이 열람/백업/이동 불가 — admin 이
// "폴더를 뒤져도 안 보임" 이라 보고한 원인. Tauri 에선 파일에 저장하고
// localStorage 는 브라우저(remote) 모드 fallback + 마이그레이션 소스로만.
function isTauriEnv(): boolean {
	if (typeof window === 'undefined') return false;
	const w = window as unknown as Record<string, unknown>;
	return '__TAURI_INTERNALS__' in w || '__TAURI__' in w;
}

/** 시동 파일 로드가 끝나기 전의 store 변경(마이그레이션 이전 값 등)을 파일에
 *  쓰지 않도록 게이트 — initCustomTheme() 이 로드 완료 후 연다. */
let fileWriteReady = false;
let fileSaveTimer: ReturnType<typeof setTimeout> | null = null;

interface ThemesFile {
	presets: CustomTheme[];
	active: string | null;
}

function scheduleFileSave() {
	if (!isTauriEnv() || !fileWriteReady) return;
	if (fileSaveTimer) clearTimeout(fileSaveTimer);
	fileSaveTimer = setTimeout(async () => {
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const payload: ThemesFile = {
				presets: get(customThemes),
				active: get(activeCustomTheme)
			};
			await invoke('save_custom_themes', { content: JSON.stringify(payload, null, 2) });
		} catch (e) {
			console.warn('[customThemes] themes.json 저장 실패', e);
		}
	}, 250);
}

function loadPresets(): CustomTheme[] {
	if (typeof localStorage === 'undefined') return [];
	try {
		const raw = localStorage.getItem(PRESETS_KEY);
		const arr = raw ? JSON.parse(raw) : [];
		return Array.isArray(arr) ? arr.filter(isValidPreset) : [];
	} catch {
		return [];
	}
}

function isValidPreset(p: unknown): p is CustomTheme {
	if (typeof p !== 'object' || p === null) return false;
	const t = p as Record<string, unknown>;
	return (
		typeof t.name === 'string' &&
		t.name.trim().length > 0 &&
		(t.base === 'dark' || t.base === 'light') &&
		typeof t.overrides === 'object' &&
		t.overrides !== null &&
		Object.values(t.overrides as Record<string, unknown>).every((v) => typeof v === 'string')
	);
}

function loadActive(): string | null {
	if (typeof localStorage === 'undefined') return null;
	try {
		return localStorage.getItem(ACTIVE_KEY) || null;
	} catch {
		return null;
	}
}

export const customThemes = writable<CustomTheme[]>(loadPresets());
export const activeCustomTheme = writable<string | null>(loadActive());

customThemes.subscribe((list) => {
	scheduleFileSave(); // DEV-249: Tauri 는 파일에도 (debounce).
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(PRESETS_KEY, JSON.stringify(list));
	} catch {
		/* 무시 */
	}
});
activeCustomTheme.subscribe((name) => {
	scheduleFileSave();
	if (typeof localStorage === 'undefined') return;
	try {
		if (name) localStorage.setItem(ACTIVE_KEY, name);
		else localStorage.removeItem(ACTIVE_KEY);
	} catch {
		/* 무시 */
	}
});

/** override 를 <html> inline style + themePalette 에 반영. null = 전부 해제. */
function applyOverrides(t: CustomTheme | null) {
	if (typeof document === 'undefined') return;
	const style = document.documentElement.style;
	for (const d of TOKEN_CATALOG) style.removeProperty(d.token);
	const paletteOv: Partial<ThemePalette> = {};
	if (t) {
		for (const d of TOKEN_CATALOG) {
			const v = t.overrides[d.token];
			if (!v) continue;
			style.setProperty(d.token, v);
			if (d.palette) paletteOv[d.palette] = v;
		}
	}
	setPaletteOverrides(paletteOv);
}

function findPreset(name: string): CustomTheme | null {
	return get(customThemes).find((p) => p.name === name) ?? null;
}

/** 프리셋 활성화 — base 테마로 전환 + override 적용. */
export function activatePreset(name: string) {
	const p = findPreset(name);
	if (!p) return;
	activeCustomTheme.set(name);
	setTheme(p.base); // <html data-theme> / effectiveTheme / CSS 기본값 정렬.
	applyOverrides(p);
}

/** 커스텀 해제 — override 제거(이후 테마는 호출측이 setTheme 으로 지정). */
export function deactivateCustom() {
	activeCustomTheme.set(null);
	applyOverrides(null);
}

/** 프리셋 생성(이름 중복 시 덮어씀). 생성 즉시 활성화하지는 않음. */
export function savePreset(p: CustomTheme) {
	customThemes.update((list) => {
		const rest = list.filter((x) => x.name !== p.name);
		return [...rest, p].sort((a, b) => a.name.localeCompare(b.name));
	});
}

export function deletePreset(name: string) {
	customThemes.update((list) => list.filter((x) => x.name !== name));
	if (get(activeCustomTheme) === name) deactivateCustom();
}

/** 활성 프리셋의 토큰 하나를 갱신 + 즉시 적용(live preview). */
export function setActiveOverride(token: string, hex: string) {
	const name = get(activeCustomTheme);
	if (!name) return;
	customThemes.update((list) =>
		list.map((p) => (p.name === name ? { ...p, overrides: { ...p.overrides, [token]: hex } } : p))
	);
	const p = findPreset(name);
	if (p) applyOverrides(p);
}

/** 활성 프리셋의 토큰 override 해제(base 기본값으로) + 즉시 적용. */
export function clearActiveOverride(token: string) {
	const name = get(activeCustomTheme);
	if (!name) return;
	customThemes.update((list) =>
		list.map((p) => {
			if (p.name !== name) return p;
			const { [token]: _removed, ...rest } = p.overrides;
			return { ...p, overrides: rest };
		})
	);
	const p = findPreset(name);
	if (p) applyOverrides(p);
}

/** 프리셋 전체를 JSON 텍스트로 (공유용 export). */
export function exportPresetsJson(): string {
	return JSON.stringify(get(customThemes), null, 2);
}

/**
 * JSON 텍스트에서 프리셋 import — 같은 이름은 덮어씀.
 * 반환: 가져온 개수. 형식 오류 시 throw.
 */
export function importPresetsJson(json: string): number {
	const parsed = JSON.parse(json);
	const arr: unknown[] = Array.isArray(parsed) ? parsed : [parsed];
	const valid = arr.filter(isValidPreset);
	if (valid.length === 0) throw new Error('유효한 프리셋이 없습니다.');
	for (const p of valid) savePreset(p);
	return valid.length;
}

/**
 * 앱 시동 시 로드 + 활성 프리셋 복원 — +layout onMount 에서 1회 호출.
 * (모듈 import 부수효과로 안 하는 이유: theme 적용 순서를 layout 이 제어.)
 *
 * DEV-249: Tauri 모드는 ~/.openguild/themes.json 이 진리원 —
 * - 파일 있음: 파일 내용으로 store 교체(외부에서 직접 편집한 경우도 자연 반영).
 * - 파일 없음 + localStorage 에 프리셋 있음: 1회 마이그레이션(파일로 기록).
 * 브라우저(remote) 모드는 기존 localStorage 경로 그대로.
 */
export async function initCustomTheme() {
	if (isTauriEnv()) {
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			const raw = await invoke<string | null>('load_custom_themes');
			if (raw) {
				const parsed = JSON.parse(raw) as Partial<ThemesFile>;
				const presets = Array.isArray(parsed.presets)
					? parsed.presets.filter(isValidPreset)
					: [];
				customThemes.set(presets);
				activeCustomTheme.set(
					typeof parsed.active === 'string' && parsed.active ? parsed.active : null
				);
			}
			// 파일 없음 → 아래 fileWriteReady 이후 첫 store 변경(또는 지금의
			// localStorage 값)이 마이그레이션으로 기록되도록 즉시 1회 저장.
			fileWriteReady = true;
			if (!raw && get(customThemes).length > 0) {
				scheduleFileSave();
			}
		} catch (e) {
			console.warn('[customThemes] themes.json 로드 실패 — localStorage 값 사용', e);
			fileWriteReady = true;
		}
	}
	const name = get(activeCustomTheme);
	if (!name) return;
	const p = findPreset(name);
	if (!p) {
		// 프리셋이 지워졌는데 active 만 남은 잔존 상태 — 정리.
		activeCustomTheme.set(null);
		return;
	}
	setTheme(p.base);
	applyOverrides(p);
}

/**
 * 현재 base 테마에서 토큰의 계산값(#hex) — color picker 초기값용.
 * override 미적용 원시값을 원하면 호출 전에 removeProperty 가 되어 있어야
 * 하지만, picker 는 "현재 보이는 색"을 보여주는 게 UX 상 맞으므로 그대로 읽음.
 */
export function computedTokenValue(token: string): string {
	if (typeof getComputedStyle === 'undefined') return '#000000';
	const v = getComputedStyle(document.documentElement).getPropertyValue(token).trim();
	// input[type=color] 는 #rrggbb 만 허용 — #rgb 확장, 그 외 형식은 검정 fallback.
	if (/^#[0-9a-fA-F]{6}$/.test(v)) return v;
	if (/^#[0-9a-fA-F]{3}$/.test(v)) {
		return `#${v[1]}${v[1]}${v[2]}${v[2]}${v[3]}${v[3]}`;
	}
	return '#000000';
}
