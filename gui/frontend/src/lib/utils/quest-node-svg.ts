// DEV-076: Quest Board 의 노드 SVG 를 다른 곳 (Home 마감 임박 / Overdue 섹션)
// 에서도 동일한 모양으로 렌더링하기 위해 추출. 호환성 위해 동일한 dimension /
// font / palette 사용.
//
// QuestBoard.svelte 의 makeSvgUrl 과 의도적으로 동일 — 단순 이전이 아니라
// duplicate 라 동기화는 수동 (Home 카드 모양이 board 와 살짝 달라도 됨).
//
// 결과: data:image/svg+xml URL. `<img src={url} />` 로 표시.

import { URGENCY_COLOR, URGENCY_LABEL, type Quest } from '../types';

const NODE_W = 284;
const NODE_H = 80;
const TITLE_FONT = '12px system-ui, -apple-system, sans-serif';

let _measureCtx: CanvasRenderingContext2D | null = null;

function getMeasureCtx(): CanvasRenderingContext2D | null {
	if (_measureCtx) return _measureCtx;
	if (typeof document === 'undefined') return null;
	const c = document.createElement('canvas');
	const ctx = c.getContext('2d');
	if (!ctx) return null;
	ctx.font = TITLE_FONT;
	_measureCtx = ctx;
	return ctx;
}

function splitByPixelWidth(s: string, maxPx: number): [string, string] {
	const ctx = getMeasureCtx();
	if (!ctx) {
		// ASCII=6.5px, CJK=12px 추정 fallback (SSR).
		const maxUnits = Math.floor(maxPx / 6.5);
		let acc = 0;
		for (let i = 0; i < s.length; i++) {
			const code = s.charCodeAt(i);
			const w =
				(code >= 0x1100 && code <= 0x11ff) ||
				(code >= 0x2e80 && code <= 0x9fff) ||
				(code >= 0xa960 && code <= 0xa97f) ||
				(code >= 0xac00 && code <= 0xd7af) ||
				(code >= 0xf900 && code <= 0xfaff) ||
				(code >= 0xff00 && code <= 0xff60)
					? 2
					: 1;
			if (acc + w > maxUnits) return [s.slice(0, i), s.slice(i)];
			acc += w;
		}
		return [s, ''];
	}
	if (ctx.measureText(s).width <= maxPx) return [s, ''];
	let lo = 0;
	let hi = s.length;
	while (lo < hi) {
		const mid = (lo + hi + 1) >> 1;
		if (ctx.measureText(s.slice(0, mid)).width <= maxPx) lo = mid;
		else hi = mid - 1;
	}
	return [s.slice(0, lo), s.slice(lo)];
}

function splitByPixelWidthAtWord(s: string, maxPx: number): [string, string] {
	const [hardHead, hardTail] = splitByPixelWidth(s, maxPx);
	if (!hardTail) return [hardHead, ''];
	if (/^\s/.test(hardTail)) {
		return [hardHead.replace(/\s+$/, ''), hardTail.replace(/^\s+/, '')];
	}
	const lastWs = hardHead.search(/\s\S*$/);
	if (lastWs > 0) {
		const head = hardHead.slice(0, lastWs);
		const tail = hardHead.slice(lastWs + 1) + hardTail;
		return [head.replace(/\s+$/, ''), tail];
	}
	return [hardHead, hardTail];
}

export const QUEST_NODE_W = NODE_W;
export const QUEST_NODE_H = NODE_H;

/**
 * Quest Board 의 노드와 동일한 모양으로 quest 를 SVG data URL 로 렌더링.
 *
 * @param overlayColor 옵션. 'overdue' = 빨간 외곽선 강조, undefined = 기본.
 */
export function makeQuestNodeSvgUrl(quest: Quest, overlayColor?: string): string {
	const W = NODE_W;
	const H = NODE_H;
	const uc = URGENCY_COLOR[quest.urgency as 1 | 2 | 3 | 4] ?? '#666';
	const tc = quest.type_color;
	const ul = URGENCY_LABEL[quest.urgency as 1 | 2 | 3 | 4] ?? '?';
	const qid = quest.quest_id;

	const xEsc = (s: string) =>
		s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
	const qidW = Math.ceil(qid.length * 6.4) + 16;
	const ulW = Math.ceil(ul.length * 5.6) + 14;
	const ulX = 10 + qidW + 6;

	const full = quest.title;
	const MAX_PX = 260;
	const [line1, rest1] = splitByPixelWidthAtWord(full, MAX_PX);
	const [rawL2, rest2] = splitByPixelWidthAtWord(rest1, MAX_PX);
	const line2 = rest2.length > 0 ? splitByPixelWidth(rawL2, MAX_PX - 10)[0] + '…' : rawL2;

	const titleY = line2 ? 44 : 52;

	// overlay (overdue 표시 등) — border + glow.
	const overlay = overlayColor
		? `<rect x="1" y="1" width="${W - 2}" height="${H - 2}" rx="6" ry="6"
       fill="none" stroke="${overlayColor}" stroke-width="2.5"/>`
		: '';

	const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}">
  <rect x="0" y="0" width="${W}" height="${H}" rx="6" ry="6" fill="#0d1117" stroke="${uc}" stroke-width="1.5" stroke-opacity="0.9"/>
  <rect x="0" y="0" width="3" height="${H}" rx="1.5" fill="${uc}" opacity="0.9"/>
  <rect x="10" y="9" width="${qidW}" height="17" rx="8.5"
    fill="${tc}" fill-opacity="0.16" stroke="${tc}" stroke-opacity="0.55" stroke-width="1"/>
  <text x="${10 + qidW / 2}" y="21.5" text-anchor="middle"
    fill="${tc}" font-size="10" font-weight="600"
    font-family="'SFMono-Regular',Consolas,monospace">${xEsc(qid)}</text>
  <rect x="${ulX}" y="9" width="${ulW}" height="17" rx="8.5"
    fill="${uc}" fill-opacity="0.16" stroke="${uc}" stroke-opacity="0.55" stroke-width="1"/>
  <text x="${ulX + ulW / 2}" y="21.5" text-anchor="middle"
    fill="${uc}" font-size="10" font-weight="500"
    font-family="system-ui,sans-serif">${xEsc(ul)}</text>
  <text x="10" y="${titleY}" fill="#c9d1d9" font-size="12"
    font-family="system-ui,-apple-system,sans-serif">${xEsc(line1)}</text>
  ${line2 ? `<text x="10" y="${titleY + 16}" fill="#c9d1d9" font-size="12"
    font-family="system-ui,-apple-system,sans-serif">${xEsc(line2)}</text>` : ''}
  ${overlay}
</svg>`;
	return 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
}
