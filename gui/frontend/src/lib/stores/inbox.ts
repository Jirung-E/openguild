// DEV-478: 알림함 — 잠깐 떴다 사라진 알림을 다시 본다.
//
// 두 갈래를 한 목록으로 보여 준다.
// - **길드 알림**(플러그인 `notify` · 실패): 코어가 `.guild/notices/` 에 적는다. 앱을 껐다 켜도, 같은 기계의
//   서버 · CLI 에서 난 것도 남는다. 읽음 표시도 거기 있다.
// - **앱 알림**(저장 실패 등 토스트): 그 순간의 일이라 앱이 켜진 동안만 여기 들고 있다.
import { derived, get, writable } from 'svelte/store';
import { noticesApi, type Inbox, type Notice } from '$lib/api/notices';

export interface InboxItem {
	/** 목록 key. */
	key: string;
	at: string;
	source: 'app' | 'plugin' | 'problem';
	/** 플러그인 알림이면 그 이름. */
	plugin?: string;
	text: string;
	error: boolean;
	href?: string;
	unread: boolean;
	/** 잇달아 난 같은 앱 알림의 횟수. */
	count: number;
	/** 플러그인이 고른 아이콘(`notify(글, #{ icon })`). */
	icon?: 'created';
}

/** 알림을 눌렀을 때 갈 앱 안 주소 — 번호로 문서 종류를 가른다. */
export function noticeHref({
	open,
	comment
}: {
	open?: string;
	comment?: number;
}): string | undefined {
	if (!open) return undefined;
	const id = encodeURIComponent(open);
	if (/^BOOK-\d+$/.test(open)) return `/library?id=${id}`;
	const base = /^C-\d+$/.test(open) ? `/campaigns/${id}` : `/quests/${id}`;
	return comment === undefined ? base : `${base}?comment=${comment}`;
}

/** 앱 알림은 이만큼만 들고 있는다. */
export const MAX_APP_NOTICES = 100;

interface AppNotice {
	key: string;
	at: string;
	text: string;
	error: boolean;
	href?: string;
	count: number;
}

const appNotices = writable<AppNotice[]>([]);
/** 앱 알림을 어디까지 읽었나(ms). 길드 알림의 읽음은 파일에 있다. */
const appSeen = writable(0);
const guild = writable<Inbox>({ notices: [], unread: 0, unread_problems: 0 });
let seq = 1;

/** 앱 토스트를 알림함에 남긴다 — `showToast` 가 부른다. 돌려주는 것은 그 줄의 key(같은 오류가 잇달으면 묶인 줄). */
export function recordAppNotice(text: string, error: boolean, href?: string): string {
	const item: AppNotice = {
		key: `app-${seq++}`,
		at: new Date().toISOString(),
		text,
		error,
		href,
		count: 1
	};
	let key = item.key;
	appNotices.update((list) => {
		// 같은 오류가 잇달아 나면 한 줄로 — 맨 위 것을 새 시각으로 올리고 횟수를 센다(토스트의 ×N 과 같다).
		const top = list[0];
		if (top && top.text === text && top.error === error && top.href === href) {
			key = top.key;
			return [{ ...top, at: item.at, count: top.count + 1 }, ...list.slice(1)];
		}
		return [item, ...list].slice(0, MAX_APP_NOTICES);
	});
	return key;
}

/** 길드 알림을 다시 읽는다. 못 읽으면(길드 없음 · 서버 끊김) 있던 것을 둔다. */
export async function refreshInbox(): Promise<void> {
	try {
		guild.set(await noticesApi.get());
	} catch {
		/* 알림함은 덤이다 — 다른 화면을 막지 않는다. */
	}
}

/**
 * 길드 알림 줄의 key — 시각과 내용으로 짓는다. 목록 안의 순번으로 지으면 하나를 지우거나 새 알림이 올 때마다 그 뒤
 * 모든 줄의 key 가 바뀌어, 화면의 카드가 한꺼번에 빠졌다 다시 들어왔다. 시각 · 글까지 같은 줄은 차례를 붙인다.
 */
function guildKeys(notices: Notice[]): string[] {
	const seen = new Map<string, number>();
	return notices.map((n) => {
		const base = `g-${n.at}-${n.kind}-${n.plugin ?? ''}-${n.text}`;
		const k = (seen.get(base) ?? 0) + 1;
		seen.set(base, k);
		return k === 1 ? base : `${base}#${k}`;
	});
}

function fromGuild(n: Notice, i: number, unread: number, key: string): InboxItem {
	return {
		key,
		at: n.at,
		source: n.kind === 'problem' ? 'problem' : 'plugin',
		plugin: n.plugin,
		text: n.text,
		error: n.kind === 'problem',
		href: noticeHref(n),
		unread: i < unread,
		count: 1,
		icon: n.icon
	};
}

/** 합친 목록 — 최근 것이 먼저. */
export const inboxItems = derived([appNotices, appSeen, guild], ([$app, $seen, $guild]) => {
	const items: InboxItem[] = [
		...(() => {
			const keys = guildKeys($guild.notices);
			return $guild.notices.map((n, i) => fromGuild(n, i, $guild.unread, keys[i]));
		})(),
		...$app.map((a) => ({
			key: a.key,
			at: a.at,
			source: 'app' as const,
			text: a.text,
			error: a.error,
			href: a.href,
			unread: Date.parse(a.at) > $seen,
			count: a.count
		}))
	];
	return items.sort((a, b) => Date.parse(b.at) - Date.parse(a.at));
});

/** 종에 붙는 표시 — 안 읽은 개수와, 그 가운데 오류가 있는지. */
export const inboxBadge = derived(inboxItems, ($items) => {
	const unread = $items.filter((i) => i.unread);
	return { count: unread.length, error: unread.some((i) => i.error) };
});

/** 모두 읽음. 길드 알림은 가장 최근 것의 시각까지 적는다(다른 창 · CLI 와 같이 본다). */
export async function markAllRead(): Promise<void> {
	appSeen.set(Date.now());
	const g = get(guild);
	if (g.unread === 0 || g.notices.length === 0) return;
	guild.set({ ...g, unread: 0, unread_problems: 0 });
	try {
		await noticesApi.markSeen(g.notices[0].at);
	} catch {
		/* 다음에 다시 읽으면 안 읽은 것으로 돌아온다 — 잃는 것은 없다. */
	}
}

/** 하나 지운다(×). 길드 알림은 파일에서도 지운다 — 다른 창 · CLI 에서도 사라진다. */
export async function dismissItem(item: InboxItem): Promise<void> {
	if (item.source === 'app') {
		appNotices.update((list) => list.filter((a) => a.key !== item.key));
		return;
	}
	const g = get(guild);
	const i = g.notices.findIndex((n) => n.at === item.at && n.text === item.text);
	if (i >= 0) {
		const n = g.notices[i];
		const wasUnread = i < g.unread;
		guild.set({
			notices: g.notices.filter((_, k) => k !== i),
			unread: g.unread - (wasUnread ? 1 : 0),
			unread_problems: g.unread_problems - (wasUnread && n.kind === 'problem' ? 1 : 0)
		});
	}
	try {
		await noticesApi.dismiss(item.at, item.text);
	} catch {
		// 못 지웠다 — 파일에 있는 그대로 다시 보여 준다.
		await refreshInbox();
	}
}

/** 알림함이 열려 있나 — 열리면 오른쪽 아래 알림 스택이 지나간 알림까지 펼쳐 보인다. */
export const inboxOpen = writable(false);

export function openInbox() {
	if (get(inboxOpen)) return;
	inboxOpen.set(true);
	void refreshInbox();
}

/** 닫는다 — 연 동안 본 것은 모두 읽음. */
export function closeInbox() {
	if (!get(inboxOpen)) return;
	inboxOpen.set(false);
	void markAllRead();
}

export function toggleInbox() {
	if (get(inboxOpen)) closeInbox();
	else openInbox();
}

/** 모두 지운다 — 길드 알림은 파일에서도, 앱 알림은 목록에서. */
export async function clearAllNotices(): Promise<void> {
	appNotices.set([]);
	guild.set({ notices: [], unread: 0, unread_problems: 0 });
	try {
		await noticesApi.clear();
	} catch {
		await refreshInbox();
	}
}

/** 시험용 — 상태를 비운다. */
export function resetInboxForTest() {
	appNotices.set([]);
	appSeen.set(0);
	guild.set({ notices: [], unread: 0, unread_problems: 0 });
	inboxOpen.set(false);
	seq = 1;
}
