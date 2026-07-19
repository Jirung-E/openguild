/**
 * DEV-132 후속(admin 보고): 댓글 커스텀 반응 이모지 입력이 길이 제한 없이
 * 임의 문자열을 그대로 받고 있었다 — 이모지 1개(스킨톤/ZWJ 합성 시퀀스 또는
 * 국기 포함)만 허용하도록 검증.
 */

// Extended_Pictographic 하나 + variation selector(U+FE0F)/ZWJ 합성/스킨톤
// modifier 반복, 또는 regional indicator 2개(국기) — 전체 문자열이 이 형태
// 하나여야 통과(앞뒤에 다른 문자·이모지가 더 있으면 거부).
const EMOJI_ONLY_RE = new RegExp(
	'^(\\p{Extended_Pictographic}(\\uFE0F|\\u200D\\p{Extended_Pictographic}|[\\u{1F3FB}-\\u{1F3FF}])*' +
		'|\\p{Regional_Indicator}\\p{Regional_Indicator})$',
	'u'
);

export function isSingleEmoji(s: string): boolean {
	return EMOJI_ONLY_RE.test(s);
}
