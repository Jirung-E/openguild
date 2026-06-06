<!--
  DEV-011: 새 캠페인 생성 페이지 (/campaigns/new).
   - 제목 (필수) / 시작일 / 종료일 / 본문 markdown (선택)
   - 생성 후 detail 페이지로 이동
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { campaignsApi } from '$lib/api/campaigns';

	let title = $state('');
	let startedAt = $state('');
	let endedAt = $state('');
	let description = $state('');
	let saving = $state(false);
	let error = $state<string | null>(null);

	async function submit() {
		const t = title.trim();
		if (!t) {
			error = '제목을 입력하세요.';
			return;
		}
		saving = true;
		error = null;
		try {
			const created = await campaignsApi.create({
				title: t,
				description: description.trim() || null,
				started_at: startedAt || null,
				ended_at: endedAt || null
			});
			// detail 페이지로 이동 (생성 직후 body markdown 도 description 으로 들어감)
			// description 이 있다면 별도 update — create 가 description 안 받음. 별도 호출.
			if (description.trim()) {
				await campaignsApi.update(created.campaign_slug, {
					description: description.trim()
				});
			}
			goto(`/campaigns/${encodeURIComponent(created.campaign_slug)}`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'failed to create';
			saving = false;
		}
	}
</script>

<div class="page">
	<div class="header">
		<button class="back" onclick={() => history.back()}>← 뒤로</button>
		<h1>새 캠페인</h1>
	</div>

	<form
		onsubmit={(e) => {
			e.preventDefault();
			submit();
		}}
	>
		<label>
			<span class="lab">제목 *</span>
			<input type="text" bind:value={title} placeholder="예: v1.0 출시" disabled={saving} />
		</label>

		<div class="period-row">
			<label>
				<span class="lab">시작일</span>
				<input type="date" bind:value={startedAt} disabled={saving} />
			</label>
			<label>
				<span class="lab">종료일</span>
				<input type="date" bind:value={endedAt} disabled={saving} />
			</label>
		</div>

		<label>
			<span class="lab">본문 (markdown, 선택)</span>
			<textarea
				bind:value={description}
				rows="10"
				disabled={saving}
				placeholder="기획 내용 / 메모...\n\n## 체크리스트\n- [ ] 할 일 A\n- [ ] 할 일 B"
			></textarea>
		</label>

		{#if error}<div class="error">{error}</div>{/if}

		<div class="actions">
			<button type="button" class="btn-cancel" onclick={() => history.back()} disabled={saving}>
				취소
			</button>
			<button type="submit" class="btn-primary" disabled={saving || !title.trim()}>
				{saving ? '저장 중…' : '생성'}
			</button>
		</div>
	</form>
</div>

<style>
	.page { padding: 1.25rem 1.5rem; max-width: 760px; margin: 0 auto; }
	.header { display: flex; align-items: center; gap: 0.75rem; margin-bottom: 1rem; }
	.header h1 { font-size: 1.2rem; color: #c9d1d9; margin: 0; }
	.back {
		background: transparent;
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 6px;
		padding: 0.3rem 0.7rem;
		font-size: 0.825rem;
		cursor: pointer;
	}
	.back:hover { background: var(--bg-subtle); }

	form { display: flex; flex-direction: column; gap: 0.85rem; }
	label { display: flex; flex-direction: column; gap: 0.3rem; }
	.lab { font-size: 0.825rem; color: #8b949e; }
	input, textarea {
		background: var(--bg);
		border: 1px solid #30363d;
		color: #c9d1d9;
		border-radius: 6px;
		padding: 0.45rem 0.6rem;
		font-size: 0.9rem;
		font-family: inherit;
	}
	input:focus, textarea:focus { outline: none; border-color: #58a6ff; }
	textarea { font-family: 'JetBrains Mono', ui-monospace, monospace; resize: vertical; min-height: 120px; }

	.period-row { display: grid; grid-template-columns: 1fr 1fr; gap: 0.75rem; }

	.error {
		background: color-mix(in srgb, var(--danger) 18%, transparent);
		border: 1px solid #f85149;
		color: #ff7b72;
		padding: 0.4rem 0.65rem;
		border-radius: 4px;
		font-size: 0.825rem;
	}

	.actions { display: flex; gap: 0.5rem; justify-content: flex-end; margin-top: 0.5rem; }
	.btn-cancel, .btn-primary {
		padding: 0.4rem 0.95rem;
		border-radius: 6px;
		font-size: 0.875rem;
		cursor: pointer;
	}
	.btn-cancel { background: transparent; border: 1px solid var(--border); color: var(--text); }
	.btn-cancel:hover { background: var(--bg-subtle); }
	.btn-primary { background: var(--btn-primary-bg); border: 1px solid var(--btn-primary-border); color: var(--btn-primary-text); }
	.btn-primary:hover:not(:disabled) { background: var(--btn-primary-bg-hover); border-color: var(--btn-primary-border-hover); }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
