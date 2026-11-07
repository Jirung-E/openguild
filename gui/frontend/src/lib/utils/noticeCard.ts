// DEV-478: 알림 카드 — 오른쪽 아래 토스트와 알림함이 같은 카드를 쓴다.
//
// 토스트(지금 떠 있는 것)와 알림함 줄(지나간 것까지)을 카드 하나의 모양으로 맞추고, 알림함을 열 때 둘을 섞는다.
// 지금 떠 있는 카드는 **같은 key** 를 그대로 써서 제자리에 남고, 그 사이 사라졌던 줄만 끼어 들어온다.
import type { InboxItem } from '$lib/stores/inbox';
import type { ToastNotif } from '$lib/stores/toast';

/** 아이콘과 색 — 일반 알림(종, 파랑) · 무엇이 생긴 것(plus-circle, 파랑) · 실패(경고, 빨강). */
export type Tone = 'notify' | 'created' | 'problem';

/** 종류 줄의 앞말. */
export type KindLabel = 'plugin' | 'problem' | 'app' | 'error';

export interface NoticeCardData {
	key: string;
	tone: Tone;
	kind: KindLabel;
	/** 보낸 플러그인. */
	who?: string;
	at?: string;
	title: string;
	/** 플러그인 실패면 실패한 줄 이름 — 제목을 "'줄' 실행 실패" 로 쓴다. */
	failedLine?: string;
	body?: string;
	href?: string;
	/** 갈 곳의 번호(DEV-001 · C-003 · BOOK-002). */
	target?: string;
	unread: boolean;
	count: number;
	/** 지금 떠 있는 토스트. */
	toast?: ToastNotif;
	/** 알림함의 줄. */
	item?: InboxItem;
}

/** 글을 제목(첫 줄)과 본문(나머지)으로. */
function split(text: string): { title: string; body?: string } {
	const [first, ...rest] = text.split('\n');
	const body = rest.join('\n').trim();
	return { title: first.trim(), body: body || undefined };
}

/** 코어의 실패 문장 `플러그인 'p' 줄 — 이유` 를 나눈다. 모양이 다르면 글 그대로. */
const PROBLEM = /^(?:플러그인|plugin) '([^']+)' (.+?) — ([\s\S]+)$/;

function problemParts(text: string): Pick<NoticeCardData, 'who' | 'title' | 'failedLine' | 'body'> {
	const m = PROBLEM.exec(text);
	if (!m) return { ...split(text) };
	const [, who, middle, detail] = m;
	// 줄 이름(`생성` · `#1`)이면 "'생성' 실행 실패", 문장이면 그대로 제목.
	const isLine = middle.length <= 24 && !/\s/.test(middle);
	return isLine
		? { who, title: middle, failedLine: middle, body: detail.trim() }
		: { who, title: middle, body: detail.trim() };
}

/** 앱 안 주소에서 문서 번호를 꺼낸다 — `/quests/DEV-1?comment=2` → `DEV-1`. */
export function targetOf(href: string | undefined): string | undefined {
	if (!href) return undefined;
	const lib = /[?&]id=([^&]+)/.exec(href);
	if (href.startsWith('/library') && lib) return decodeURIComponent(lib[1]);
	const m = /^\/(?:quests|campaigns)\/([^/?#]+)/.exec(href);
	return m ? decodeURIComponent(m[1]) : undefined;
}

export function cardOfItem(item: InboxItem): NoticeCardData {
	const base = {
		key: `i-${item.key}`,
		at: item.at,
		href: item.href,
		target: targetOf(item.href),
		unread: item.unread,
		count: item.count,
		item
	};
	if (item.source === 'problem') {
		return { ...base, tone: 'problem', kind: 'problem', ...problemParts(item.text) };
	}
	if (item.source === 'plugin') {
		const tone = item.icon === 'created' ? 'created' : 'notify';
		return { ...base, tone, kind: 'plugin', who: item.plugin, ...split(item.text) };
	}
	return {
		...base,
		tone: item.error ? 'problem' : 'notify',
		kind: item.error ? 'error' : 'app',
		...split(item.text)
	};
}

export function cardOfToast(t: ToastNotif, item?: InboxItem): NoticeCardData {
	const base = {
		key: t.id,
		at: item?.at ?? t.at,
		href: t.href,
		target: targetOf(t.href),
		unread: item?.unread ?? false,
		count: t.count,
		toast: t,
		item
	};
	if (t.plugin !== undefined) {
		const tone = t.icon === 'created' ? 'created' : 'notify';
		return { ...base, tone, kind: 'plugin', who: t.plugin, ...split(t.text ?? '') };
	}
	const error = t.variant === 'error';
	return {
		...base,
		tone: error ? 'problem' : 'notify',
		kind: error ? 'error' : 'app',
		...split(t.message)
	};
}

/** 토스트마다 알림함의 같은 줄 — 앱 알림은 key 로, 플러그인 알림은 플러그인 · 글 · 갈 곳이 같은 가장 최근 줄로. */
export function matchToasts(toasts: ToastNotif[], items: InboxItem[]): Map<string, InboxItem> {
	const out = new Map<string, InboxItem>();
	const used = new Set<string>();
	// 최근 토스트부터 최근 줄에 — 같은 글이 여러 번 왔으면 차례로 짝짓는다.
	const newestFirst = [...items].sort((a, b) => Date.parse(b.at) - Date.parse(a.at));
	for (const t of [...toasts].reverse()) {
		const hit = newestFirst.find(
			(i) =>
				!used.has(i.key) &&
				(t.inboxKey !== undefined
					? i.key === t.inboxKey
					: t.plugin !== undefined &&
						i.source === 'plugin' &&
						i.plugin === t.plugin &&
						i.text === t.text &&
						i.href === t.href)
		);
		if (hit) {
			used.add(hit.key);
			out.set(t.id, hit);
		}
	}
	return out;
}

/**
 * 스택에 보일 카드 — 위에서 아래(코너)로. 닫혀 있으면 지금 떠 있는 토스트만, 열려 있으면 알림함의 모든 줄과
 * 떠 있는 토스트를 시각 순(오래된 것 위)으로 섞는다. 떠 있는 것은 토스트의 key 를 그대로 쓴다.
 */
export function stackCards(
	toasts: ToastNotif[],
	items: InboxItem[],
	open: boolean,
	seen: Map<string, string> = new Map()
): NoticeCardData[] {
	const matched = matchToasts(toasts, items);
	// 한 번 토스트로 떴던 줄은 그 토스트의 key 를 계속 쓴다 — 알림함을 연 채 토스트 시간이 다 돼도 카드가
	// 빠졌다 다시 들어오지 않는다(같은 알림이다).
	for (const [id, item] of matched) seen.set(item.key, id);
	const shown = toasts.map((t) => cardOfToast(t, matched.get(t.id)));
	if (!open) return shown;
	const taken = new Set([...matched.values()].map((i) => i.key));
	const past = items
		.filter((i) => !taken.has(i.key))
		.map((i) => {
			const c = cardOfItem(i);
			const was = seen.get(i.key);
			return was ? { ...c, key: was } : c;
		});
	const time = (c: NoticeCardData) => (c.at ? Date.parse(c.at) : Number.MAX_SAFE_INTEGER);
	// 시각이 같으면 떠 있는 것을 아래로(코너 쪽) — 열고 닫을 때 자리가 안 바뀐다.
	return [...past, ...shown].sort((a, b) => time(a) - time(b));
}
