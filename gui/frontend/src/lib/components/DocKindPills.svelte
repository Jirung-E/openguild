<script lang="ts">
	// DEV-482: 문서 종류 칩 + 번호 칩 — 검색 팔레트(`SearchPalette` 의 `.pills`)와 같은 모양.
	// 작업 기록처럼 문서를 줄로 늘어놓는 곳에서 "어느 종류의 어느 문서"가 같은 모양으로 읽히게 한다.
	// 규칙은 slug 가 곧 이름이라 번호 칩을 만들지 않는다(팔레트와 같다) — 이름은 쓰는 쪽이 제목 자리에 둔다.
	import { locale, t } from '$lib/stores/locale';

	type Kind = 'quest' | 'campaign' | 'rule' | 'book';
	let { kind, id }: { kind: Kind; id: string } = $props();
</script>

<span class="pills {kind}">
	<span class="ptype">{t(`kind.${kind}`, $locale)}</span>
	{#if kind !== 'rule'}
		<span class="pill mono xs pslug">{id}</span>
	{/if}
</span>

<style>
	.pills {
		display: inline-flex;
		align-items: center;
		gap: var(--sp-6);
		flex: none;
	}
	.pills.quest {
		--kind-c: var(--accent);
	}
	.pills.campaign {
		--kind-c: var(--hl-pre);
	}
	.pills.rule {
		--kind-c: var(--success);
	}
	.pills.book {
		--kind-c: var(--warning);
	}
	/* 팔레트의 `.ptype` 보다 한 단계 작다 — 작업 기록 줄은 글자가 `--fs-sm` 이라 같은 크기면 칩이 앞선다. */
	.ptype {
		flex: none;
		min-width: 3.4rem;
		height: 1.25rem;
		padding: 0 var(--sp-6);
		box-sizing: border-box;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		font-size: var(--fs-xs);
		font-weight: var(--fw-semibold);
		line-height: var(--lh-tight);
		border-radius: var(--r-sm);
		color: var(--kind-c);
		background: color-mix(in srgb, var(--kind-c) 14%, transparent);
	}
	.pill.pslug {
		--pill-fs: 0.7rem;
		--pill-h: 1.25rem;
		--c: var(--kind-c);
		flex: none;
	}
</style>
