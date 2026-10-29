import { describe, expect, it } from 'vitest';
import {
	PLUGIN_INSTEAD_TAG,
	isPluginInstead,
	pluginInsteadFromHttp,
	pluginInsteadFromInvoke
} from './pluginInstead';

describe('DEV-465: 플러그인이 대신 처리함', () => {
	it('앱의 꼬리표 문자열을 푼다', () => {
		const e = pluginInsteadFromInvoke(`${PLUGIN_INSTEAD_TAG}세션을 엽니다`);
		expect(isPluginInstead(e)).toBe(true);
		expect(e?.message).toBe('세션을 엽니다');
		expect(pluginInsteadFromInvoke('그냥 오류')).toBeNull();
	});

	it('서버의 202 본문을 푼다 — 다른 코드나 다른 모양은 아니다', () => {
		expect(pluginInsteadFromHttp(202, { instead: '세션을 엽니다' })?.message).toBe('세션을 엽니다');
		expect(pluginInsteadFromHttp(200, { instead: 'x' })).toBeNull();
		expect(pluginInsteadFromHttp(202, { id: 1 })).toBeNull();
		expect(pluginInsteadFromHttp(202, null)).toBeNull();
	});
});
