<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import Home from '$lib/components/Home.svelte';
	import QuestBoard from '$lib/components/QuestBoard.svelte';
	import QuestList from '$lib/components/QuestList.svelte';
	import NewQuestModal from '$lib/components/NewQuestModal.svelte';
	import { flashQuestId } from '$lib/stores';
	import type { Quest } from '$lib/types';
	import { detectEnvironment } from '$lib/api/transport';
	import { getRemoteServerUrl, isRemoteSessionActive } from '$lib/stores/remoteServer';

	// DEV-011: Home 추가. ?view 없으면 home 기본.
	type View = 'home' | 'board' | 'list';
	let currentView: View = $derived(($page.url.searchParams.get('view') as View | null) ?? 'home');

	// DEV-084: New Quest 를 상단 nav 에서 각 뷰의 기존 상단 바/툴바 우측 끝으로
	// 이동 (QuestBoard toolbar / QuestList filter-bar). 모달 + 생성 로직은 여기
	// (+page) 에서 소유, 콜백으로 버튼 클릭만 위임.
	let showNewQuest = $state(false);

	async function onCreated(quest: Quest) {
		// 보드 뷰가 아니면 보드로 이동 후 펄스 (생성 결과 위치 확인 편의).
		if (currentView !== 'board') {
			await goto('/?view=board');
		}
		flashQuestId.set(quest.id);
		showNewQuest = false;
	}

	// DEV-052: Tauri 가 인자 없이 시작되면 launch_mode === "welcome".
	// 길드 컨텍스트가 없으므로 / (board) 진입 시 항상 /welcome 으로 bounce.
	// (이전에 sessionStorage 마커로 첫 회만 redirect 했지만, Nav 로고 클릭 등
	// 으로 다시 / 진입 시 빈 보드가 노출되는 버그가 있어서 제거.)
	//
	// DEV-113 후속(사용자 보고: "웹브라우저로는 원격길드 접속이 되는데 GUI
	// 에서는 안된다"): `launch_mode` 는 Rust 의 로컬 Store 상태만 본다 —
	// 원격 서버에 연결(remoteServerUrl)해도 Rust 쪽은 여전히 길드를 안 연
	// 상태(`welcome`)이므로, Welcome 에서 연결 후 `/` 로 이동해도 이 guard
	// 가 즉시 다시 `/welcome` 으로 돌려보내 마치 "연결이 안 되는" 것처럼
	// 보였다(브라우저 모드는 이 invoke 자체가 없어 멀쩩했음). 원격 override
	// 가 활성이면 이 bounce 를 건너뛴다 — 원격 연결도 유효한 "길드 컨텍스트".
	//
	// BUG-095(사용자 보고: "gui를 처음 열때 이전 원격 길드의 홈으로 열리는
	// 현상"): `remoteServerUrl` 만 보고 건너뛰면 localStorage 에 남은 *이전
	// 세션* 의 값으로 콜드 스타트에도 자동 재진입한다 — local 길드는 open_*
	// Tauri command 가 `LaunchInfo.mode` 를 "guild" 로 갱신해줘서(Rust 쪽
	// 진짜 상태) 이런 문제가 없는데, 원격은 그 갱신 메커니즘이 없어 비대칭.
	// `isRemoteSessionActive()`(sessionStorage — 프로세스 재시작마다 빈
	// 상태로 시작)로 "이번 세션에 Welcome 에서 실제로 연결했는지"까지 함께
	// 확인해야 진짜 콜드 스타트와 구분된다.
	onMount(async () => {
		if (detectEnvironment() !== 'tauri') return;
		if (getRemoteServerUrl() && isRemoteSessionActive()) return;
		try {
			const { invoke } = await import('@tauri-apps/api/core');
			// DEV-052 후속 (3회차): launch_mode 가 string → { mode, uninit_path }
			// 객체로 바뀌었음. 이전 string 비교 (mode === 'welcome') 는 영원히
			// false 라 redirect 가 안 되는 버그가 있었음.
			const info = await invoke<{ mode: string; uninit_path: string | null }>('launch_mode');
			if (info.mode === 'welcome' || info.mode === 'uninit') {
				// BUG-100(사용자 보고: "gui를 처음 켰을때도 웰컴페이지에서 뒤로가기
				// 단축키가 동작한다 — 최근에 연 길드로 돌아가려는 것 같다"):
				// 기본 goto 는 history 에 새 항목을 쌓아(push), 콜드 스타트 시
				// ["/", "/welcome"] 두 entry 가 남는다. "/" 자체는 길드 컨텍스트
				// 없이도 board/home UI 를 렌더하므로(빈 placeholder Store),
				// 뒤로가기를 누르면 그 "/" 가 잠깐 보였다가 다시 /welcome 으로
				// bounce — 마치 "다른 길드로 가려다 막힌" 것처럼 보인다.
				// replaceState 로 "/" 항목을 지우고 들어가 history 에 dangling
				// entry 가 남지 않게 한다.
				goto('/welcome', { replaceState: true });
			}
		} catch {
			// invoke 실패 시 redirect 생략 (회귀 방지).
		}
	});
</script>

{#if currentView === 'board'}
	<QuestBoard onNewQuest={() => (showNewQuest = true)} />
{:else if currentView === 'list'}
	<QuestList onNewQuest={() => (showNewQuest = true)} />
{:else}
	<Home />
{/if}

{#if showNewQuest}
	<NewQuestModal onclose={() => (showNewQuest = false)} oncreated={onCreated} />
{/if}
