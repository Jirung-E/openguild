// BUG-322(admin): 설정 퀵메뉴가 열린 채로 **타이틀바를 눌러도 안 닫혔다.**
//
// 두 가지가 겹쳤다. 타이틀바는 이 메뉴의 투명 오버레이보다 위에 있어서 오버레이가 그
// 클릭을 못 받고, Tauri 의 드래그 스크립트가 `mousedown` 에서 `stopImmediatePropagation()`
// 을 부른다. 그래서 capture 단계에서 직접 받는다 — 여기서 지키는 것이 그 성질이다.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import SettingsQuickMenu from './SettingsQuickMenu.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));

function mount(onclose: () => void) {
	// 실제와 같은 자리 — ⚙ 버튼과 메뉴가 한 껍데기 안에 있다.
	const wrap = document.createElement('div');
	wrap.className = 'settings-wrap';
	const gear = document.createElement('button');
	gear.className = 'btn-settings';
	wrap.appendChild(gear);
	document.body.appendChild(wrap);
	render(SettingsQuickMenu, { props: { onclose }, target: wrap });
	return { wrap, gear };
}

function down(el: Element, opts: { swallow?: boolean } = {}) {
	if (opts.swallow) {
		// Tauri 드래그 스크립트 흉내 — 버블 단계에서 뒤엣것을 전부 막는다.
		el.addEventListener('mousedown', (e) => e.stopImmediatePropagation(), { once: true });
	}
	el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
}

describe('BUG-322 설정 퀵메뉴 바깥 클릭', () => {
	beforeEach(() => {
		document.body.innerHTML = '';
	});

	it('바깥을 누르면 닫힌다', async () => {
		const onclose = vi.fn();
		mount(onclose);
		await tick();
		const outside = document.createElement('div');
		document.body.appendChild(outside);
		down(outside);
		expect(onclose).toHaveBeenCalled();
	});

	// 이것이 핵심이다 — 타이틀바는 이벤트를 삼킨다.
	it('이벤트를 삼키는 곳(타이틀바)을 눌러도 닫힌다', async () => {
		const onclose = vi.fn();
		mount(onclose);
		await tick();
		const titlebar = document.createElement('div');
		titlebar.setAttribute('data-tauri-drag-region', '');
		document.body.appendChild(titlebar);
		down(titlebar, { swallow: true });
		expect(onclose, '삼켜도 capture 는 먼저 돈다').toHaveBeenCalled();
	});

	// ⚙ 를 다시 누르는 것은 그 버튼이 접는다 — 여기서 또 닫으면 눌러도 안 닫히는 것처럼 된다.
	it('⚙ 버튼을 누르는 것은 여기서 안 닫는다', async () => {
		const onclose = vi.fn();
		const { gear } = mount(onclose);
		await tick();
		down(gear);
		expect(onclose).not.toHaveBeenCalled();
	});

	it('메뉴 안을 누르면 안 닫힌다', async () => {
		const onclose = vi.fn();
		mount(onclose);
		await tick();
		down(document.querySelector('.qm')!);
		expect(onclose).not.toHaveBeenCalled();
	});
});
