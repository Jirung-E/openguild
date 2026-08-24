import { describe, it, expect } from 'vitest';
import { byLastOpenedDesc } from './recent-sort';

const at = (last_opened: string, name = '') => ({ last_opened, name });

describe('byLastOpenedDesc', () => {
	it('최근이 먼저 온다', () => {
		const list = [at('2026-08-15T09:45:39Z', '오래됨'), at('2026-08-17T00:12:33Z', '최근')];
		expect(list.sort(byLastOpenedDesc).map((x) => x.name)).toEqual(['최근', '오래됨']);
	});

	it('밀리초 유무가 섞여도 시각으로 비교한다 — 문자열 비교면 뒤집힌다', () => {
		// 원격(밀리초)이 1초 더 최근인데, 문자열 비교로는 '.'(0x2E) < 'Z'(0x5A) 라
		// 오래된 것으로 밀렸다.
		const local = at('2026-08-17T00:12:33Z', '로컬');
		const remote = at('2026-08-17T00:12:34.500Z', '원격');
		expect([local, remote].sort(byLastOpenedDesc).map((x) => x.name)).toEqual(['원격', '로컬']);
		// 같은 초라면 밀리초가 있는 쪽이 더 최근이다.
		const sameSec = at('2026-08-17T00:12:33.900Z', '원격-같은초');
		expect([local, sameSec].sort(byLastOpenedDesc).map((x) => x.name)).toEqual([
			'원격-같은초',
			'로컬'
		]);
	});

	it('같은 시각이면 0 — 비일관 비교자는 정렬을 흐트러뜨린다', () => {
		expect(byLastOpenedDesc(at('2026-08-17T00:12:33Z'), at('2026-08-17T00:12:33Z'))).toBe(0);
	});

	it('파싱 불가한 값은 맨 뒤로', () => {
		const list = [at('', '빈값'), at('2026-08-17T00:12:33Z', '정상'), at('nonsense', '깨짐')];
		const sorted = list.sort(byLastOpenedDesc).map((x) => x.name);
		expect(sorted[0]).toBe('정상');
		expect(sorted.slice(1).sort()).toEqual(['깨짐', '빈값'].sort());
	});
});
