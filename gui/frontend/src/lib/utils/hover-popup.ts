// DEV-417: 문서 링크에 마우스를 올렸을 때의 **뜸/닫힘 타이밍**을 한 곳에서 정한다.
//
// 본문의 크로스링크(MarkdownView)와 관계 목록의 링크(DocLink)가 같은 팝업을 쓰는데, 타이밍을
// 각자 들고 있으면 한쪽만 고쳐져 "본문에서는 바로 뜨는데 목록에서는 안 뜬다" 가 된다.
//
// 값의 뜻:
// - 여는 지연 — 마우스가 **지나가는 것**과 **머무는 것**을 가른다. 없으면 글을 읽으려고
//   커서를 옮기는 동안 팝업이 줄줄이 떴다 사라진다.
// - 닫는 지연 — 링크에서 팝업으로 마우스를 옮기는 **그 사이**를 견딘다. 없으면 팝업 위로
//   가는 동안 닫혀서 그 안의 버튼을 누를 수가 없다.

export const HOVER_OPEN_MS = 280;
export const HOVER_CLOSE_MS = 300;

/** 여는 타이머와 닫는 타이머 한 벌. 컴포넌트가 하나씩 들고 쓴다. */
export class HoverTimers {
	private openTimer: ReturnType<typeof setTimeout> | null = null;
	private closeTimer: ReturnType<typeof setTimeout> | null = null;

	/** 머물렀다고 판단되면 부른다. 이미 예약돼 있으면 새로 잡는다. */
	scheduleOpen(fn: () => void, ms = HOVER_OPEN_MS): void {
		this.cancelOpen();
		this.openTimer = setTimeout(() => {
			this.openTimer = null;
			fn();
		}, ms);
	}

	scheduleClose(fn: () => void, ms = HOVER_CLOSE_MS): void {
		this.cancelClose();
		this.closeTimer = setTimeout(() => {
			this.closeTimer = null;
			fn();
		}, ms);
	}

	cancelOpen(): void {
		if (this.openTimer) {
			clearTimeout(this.openTimer);
			this.openTimer = null;
		}
	}

	cancelClose(): void {
		if (this.closeTimer) {
			clearTimeout(this.closeTimer);
			this.closeTimer = null;
		}
	}

	/** 언마운트 때 — 남은 타이머가 사라진 컴포넌트를 건드리지 않게. */
	clear(): void {
		this.cancelOpen();
		this.cancelClose();
	}
}
