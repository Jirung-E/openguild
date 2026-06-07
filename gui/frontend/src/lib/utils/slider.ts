/**
 * DEV-101 fix3: CustomSlider 의 pure math 추출 — clamp + step snap + value↔트랙
 * 좌표 변환. 컴포넌트 안 인라인 함수보다 회귀 테스트 가능하고 다른 슬라이더에서
 * 재사용 가능.
 */

/**
 * 값을 `[min, max]` 로 clamp + step 격자에 스냅. step 의 소수 자리수에 맞춰
 * 부동소수 누적 오차 정리 (예: step=0.01, 결과 1.0000000000000002 → 1).
 */
export function clampToStep(v: number, min: number, max: number, step: number): number {
	const clamped = Math.max(min, Math.min(max, v));
	const stepped = Math.round((clamped - min) / step) * step + min;
	const decimals = (step.toString().split('.')[1] ?? '').length;
	return Number(stepped.toFixed(decimals));
}

/**
 * 트랙 안의 픽셀 위치 (트랙 좌측 0 ~ 우측 trackWidth) → 값.
 * 트랙 밖은 양 끝으로 clamp.
 */
export function valueFromTrackPx(
	pxFromLeft: number,
	trackWidth: number,
	min: number,
	max: number,
	step: number
): number {
	if (trackWidth <= 0) return min;
	const ratio = Math.max(0, Math.min(1, pxFromLeft / trackWidth));
	return clampToStep(min + ratio * (max - min), min, max, step);
}

/**
 * 트랙 너비 / (max - min) 로 픽셀당 단위 — drag 시작 시 한 번 측정해두면
 * drag 중 layout 변경에도 매핑 일관 (DEV-101 fix3 의 핵심).
 */
export function pixelsPerUnit(trackWidth: number, min: number, max: number): number {
	const range = max - min;
	return range > 0 ? trackWidth / range : 1;
}
