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

/** 앱 토스트를 알림함에 남긴다 — `showToast` 가 부른다. */
export function recordAppNotice(text: string, error: boolean, href?: string) {
	const item: AppNotice = {
		key: `app-${seq++}`,
		at: new Date().toISOString(),
		text,
		error,
		href,
		count: 1
	};
	appNotices.update((list) => {
		// 같은 오류가 잇달아 나면 한 줄로 — 맨 위 것을 새 시각으로 올리고 횟수를 센다(토스트의 ×N 과 같다).
		const top = list[0];
		if (top && top.text === text && top.error === error && top.href === href) {
			return [{ ...top, at: item.at, count: top.count + 1 }, ...list.slice(1)];
		}
		return [item, ...list].slice(0, MAX_APP_NOTICES);
	});
}

/** 길드 알림을 다시 읽는다. 못 읽으면(길드 없음 · 서버 끊김) 있던 것을 둔다. */
export async function refreshInbox(): Promise<void> {
	try {
		guild.set(await noticesApi.get());
	} catch {
		/* 알림함은 덤이다 — 다른 화면을 막지 않는다. */
	}
}

function fromGuild(n: Notice, i: number, unread: number): InboxItem {
	return {
		key: `g-${n.at}-${i}`,
		at: n.at,
		source: n.kind === 'problem' ? 'problem' : 'plugin',
		plugin: n.plugin,
		text: n.text,
		error: n.kind === 'problem',
		href: noticeHref(n),
		unread: i < unread,
		count: 1
	};
}

/** 합친 목록 — 최근 것이 먼저. */
export const inboxItems = derived([appNotices, appSeen, guild], ([$app, $seen, $guild]) => {
	const items: InboxItem[] = [
		...$guild.notices.map((n, i) => fromGuild(n, i, $guild.unread)),
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

/** 시험용 — 상태를 비운다. */
export function resetInboxForTest() {
	appNotices.set([]);
	appSeen.set(0);
	guild.set({ notices: [], unread: 0, unread_problems: 0 });
	seq = 1;
}
