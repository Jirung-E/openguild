export const BOARD_NODE_WIDTH = 284;
export const BOARD_NODE_HEIGHT = 80;

export type BoardPoint = { x: number; y: number };
export type BoardViewport = { pan: BoardPoint; zoom: number };
export type BoardBox = { x1: number; y1: number; x2: number; y2: number; w: number; h: number };
export type BoardElementDefinition =
	| { group?: 'nodes'; data: Record<string, unknown>; position: BoardPoint }
	| { group?: 'edges'; data: Record<string, unknown>; position?: never };

export class BoardNode {
	readonly length: number;
	private graph: BoardGraph | null;
	private values: Record<string, unknown>;
	private point: BoardPoint;
	private styles: Record<string, string | number> = { display: 'element', 'z-index': 10 };
	private isSelected = false;
	private animationRaf: number | null = null;

	constructor(
		graph: BoardGraph | null,
		values: Record<string, unknown> = {},
		point: BoardPoint = { x: 0, y: 0 },
		length = 1
	) {
		this.graph = graph;
		this.values = { ...values };
		this.point = { ...point };
		this.length = length;
	}

	id(): string {
		return String(this.values.id ?? '');
	}
	data<T = unknown>(key: string): T;
	data(key: string, value: unknown): this;
	data<T = unknown>(key: string, value?: unknown): T | this {
		if (arguments.length === 1) return this.values[key] as T;
		if (this.length > 0) {
			this.values[key] = value;
			this.graph?.notifyGraphChange();
		}
		return this;
	}
	position(): BoardPoint;
	position(next: BoardPoint): this;
	position(next?: BoardPoint): BoardPoint | this {
		if (!next) return { ...this.point };
		if (this.length > 0) {
			this.point = { ...next };
			this.graph?.notifyNodePosition(this);
		}
		return this;
	}
	style<T extends string | number = string>(key: string): T;
	style(key: string, value: string | number): this;
	style<T extends string | number = string>(key: string, value?: string | number): T | this {
		if (arguments.length === 1) return this.styles[key] as T;
		if (this.length > 0) {
			this.styles[key] = value!;
			this.graph?.notifyGraphChange();
		}
		return this;
	}
	selected(): boolean {
		return this.isSelected;
	}
	select(): this {
		if (this.length > 0 && !this.isSelected) {
			this.isSelected = true;
			this.graph?.notifyGraphChange();
		}
		return this;
	}
	unselect(): this {
		if (this.length > 0 && this.isSelected) {
			this.isSelected = false;
			this.graph?.notifyGraphChange();
		}
		return this;
	}
	boundingBox(): BoardBox {
		const { x, y } = this.point;
		return {
			x1: x - BOARD_NODE_WIDTH / 2,
			y1: y - BOARD_NODE_HEIGHT / 2,
			x2: x + BOARD_NODE_WIDTH / 2,
			y2: y + BOARD_NODE_HEIGHT / 2,
			w: BOARD_NODE_WIDTH,
			h: BOARD_NODE_HEIGHT
		};
	}
	renderedPosition(): BoardPoint {
		const viewport = this.graph?.viewportState() ?? { pan: { x: 0, y: 0 }, zoom: 1 };
		return {
			x: this.point.x * viewport.zoom + viewport.pan.x,
			y: this.point.y * viewport.zoom + viewport.pan.y
		};
	}
	animate(options: { position?: BoardPoint; duration?: number }): this {
		if (!options.position || this.length === 0) return this;
		if (this.animationRaf !== null) cancelAnimationFrame(this.animationRaf);
		const from = { ...this.point };
		const to = options.position;
		const duration = Math.max(0, options.duration ?? 0);
		if (duration === 0) {
			this.position(to);
			return this;
		}
		const started = performance.now();
		const tick = (now: number) => {
			const progress = Math.min(1, (now - started) / duration);
			const eased = 1 - Math.pow(1 - progress, 3);
			this.position({
				x: from.x + (to.x - from.x) * eased,
				y: from.y + (to.y - from.y) * eased
			});
			if (progress < 1) this.animationRaf = requestAnimationFrame(tick);
			else this.animationRaf = null;
		};
		this.animationRaf = requestAnimationFrame(tick);
		return this;
	}
	destroy(): void {
		if (this.animationRaf !== null) cancelAnimationFrame(this.animationRaf);
		this.animationRaf = null;
	}
}

export class BoardEdge {
	readonly length = 1;
	private values: Record<string, unknown>;
	constructor(
		private graph: BoardGraph,
		values: Record<string, unknown>
	) {
		this.values = { ...values };
	}
	id(): string {
		return String(this.values.id ?? '');
	}
	data<T = unknown>(key: string): T;
	data(key: string, value: unknown): this;
	data<T = unknown>(key: string, value?: unknown): T | this {
		if (arguments.length === 1) return this.values[key] as T;
		this.values[key] = value;
		this.graph.notifyGraphChange();
		return this;
	}
	source(): BoardNode {
		return this.graph.getElementById(String(this.values.source));
	}
	target(): BoardNode {
		return this.graph.getElementById(String(this.values.target));
	}
}

export class BoardNodeCollection {
	constructor(readonly items: BoardNode[]) {}
	get length(): number {
		return this.items.length;
	}
	forEach(fn: (node: BoardNode, index: number) => void): void {
		this.items.forEach(fn);
	}
	filter(fn: (node: BoardNode, index: number) => boolean): BoardNodeCollection {
		return new BoardNodeCollection(this.items.filter(fn));
	}
	toArray(): BoardNode[] {
		return [...this.items];
	}
	data(key: string, value: unknown): this {
		this.items.forEach((node) => node.data(key, value));
		return this;
	}
	style(key: string, value: string | number): this {
		this.items.forEach((node) => node.style(key, value));
		return this;
	}
	select(): this {
		this.items.forEach((node) => node.select());
		return this;
	}
	unselect(): this {
		this.items.forEach((node) => node.unselect());
		return this;
	}
}

export class BoardEdgeCollection {
	constructor(readonly items: BoardEdge[]) {}
	get length(): number {
		return this.items.length;
	}
	forEach(fn: (edge: BoardEdge, index: number) => void): void {
		this.items.forEach(fn);
	}
	toArray(): BoardEdge[] {
		return [...this.items];
	}
	data(key: string, value: unknown): this {
		this.items.forEach((edge) => edge.data(key, value));
		return this;
	}
}

class BoardElements {
	constructor(private graph: BoardGraph) {}
	nonempty(): boolean {
		return this.graph.nodes().length > 0;
	}
	boundingBox(): BoardBox {
		return this.graph.boundingBox();
	}
	unselect(): this {
		this.graph.nodes().unselect();
		return this;
	}
}

export class BoardGraph {
	private nodeItems: BoardNode[] = [];
	private edgeItems: BoardEdge[] = [];
	private missingNode = new BoardNode(null, {}, { x: 0, y: 0 }, 0);
	private viewportValue: BoardViewport = { pan: { x: 0, y: 0 }, zoom: 1 };
	private minZoomValue = 0.02;
	private readonly maxZoomValue = 2;

	constructor(
		elements: BoardElementDefinition[],
		private getSize: () => { width: number; height: number },
		private onPosition: (node: BoardNode) => void,
		private onChange: () => void,
		private onViewport: () => void
	) {
		this.add(elements);
	}
	notifyNodePosition(node: BoardNode): void {
		this.onPosition(node);
	}
	notifyGraphChange(): void {
		this.onChange();
	}
	nodes(selector = '[questId]'): BoardNodeCollection {
		let nodes = [...this.nodeItems];
		const statusMatch = selector.match(/\[statusId\s*=\s*(\d+)\]/);
		if (statusMatch) {
			const statusId = Number(statusMatch[1]);
			nodes = nodes.filter((node) => node.data<number>('statusId') === statusId);
		}
		if (selector.includes(':selected')) nodes = nodes.filter((node) => node.selected());
		return new BoardNodeCollection(nodes);
	}
	edges(): BoardEdgeCollection {
		return new BoardEdgeCollection([...this.edgeItems]);
	}
	getElementById(id: string): BoardNode {
		return this.nodeItems.find((node) => node.id() === id) ?? this.missingNode;
	}
	add(definition: BoardElementDefinition | BoardElementDefinition[]): BoardNode | BoardEdge | null {
		const definitions = Array.isArray(definition) ? definition : [definition];
		let last: BoardNode | BoardEdge | null = null;
		for (const item of definitions) {
			if (item.position) {
				const node = new BoardNode(this, item.data, item.position);
				this.nodeItems.push(node);
				last = node;
			} else {
				const edge = new BoardEdge(this, item.data);
				this.edgeItems.push(edge);
				last = edge;
			}
		}
		this.onChange();
		return last;
	}
	batch(fn: () => void): void {
		fn();
	}
	elements(): BoardElements {
		return new BoardElements(this);
	}
	boundingBox(): BoardBox {
		const visible = this.nodeItems.filter((node) => node.style('display') !== 'none');
		if (visible.length === 0) return { x1: 0, y1: 0, x2: 1, y2: 1, w: 1, h: 1 };
		const boxes = visible.map((node) => node.boundingBox());
		const x1 = Math.min(...boxes.map((box) => box.x1));
		const y1 = Math.min(...boxes.map((box) => box.y1));
		const x2 = Math.max(...boxes.map((box) => box.x2));
		const y2 = Math.max(...boxes.map((box) => box.y2));
		return { x1, y1, x2, y2, w: x2 - x1, h: y2 - y1 };
	}
	pan(): BoardPoint {
		return { ...this.viewportValue.pan };
	}
	panBy(delta: BoardPoint): void {
		this.viewportValue.pan = {
			x: this.viewportValue.pan.x + delta.x,
			y: this.viewportValue.pan.y + delta.y
		};
		this.onViewport();
	}
	zoom(): number;
	zoom(options: { level: number; renderedPosition: BoardPoint }): void;
	zoom(options?: { level: number; renderedPosition: BoardPoint }): number | void {
		if (!options) return this.viewportValue.zoom;
		const oldZoom = this.viewportValue.zoom;
		const level = Math.max(this.minZoomValue, Math.min(this.maxZoomValue, options.level));
		const worldX = (options.renderedPosition.x - this.viewportValue.pan.x) / oldZoom;
		const worldY = (options.renderedPosition.y - this.viewportValue.pan.y) / oldZoom;
		this.viewportValue = {
			zoom: level,
			pan: {
				x: options.renderedPosition.x - worldX * level,
				y: options.renderedPosition.y - worldY * level
			}
		};
		this.onViewport();
	}
	viewport(next: BoardViewport): void {
		this.viewportValue = {
			pan: { ...next.pan },
			zoom: Math.max(this.minZoomValue, Math.min(this.maxZoomValue, next.zoom))
		};
		this.onViewport();
	}
	viewportState(): BoardViewport {
		return { pan: { ...this.viewportValue.pan }, zoom: this.viewportValue.zoom };
	}
	minZoom(value: number): void {
		this.minZoomValue = value;
		if (this.viewportValue.zoom < value) {
			this.viewportValue.zoom = value;
			this.onViewport();
		}
	}
	fit(_elements?: unknown, padding = 0): void {
		const box = this.boundingBox();
		const size = this.getSize();
		const availableW = Math.max(1, size.width - padding * 2);
		const availableH = Math.max(1, size.height - padding * 2);
		const zoom = Math.max(
			this.minZoomValue,
			Math.min(this.maxZoomValue, availableW / box.w, availableH / box.h)
		);
		this.viewportValue = {
			zoom,
			pan: {
				x: size.width / 2 - ((box.x1 + box.x2) / 2) * zoom,
				y: size.height / 2 - ((box.y1 + box.y2) / 2) * zoom
			}
		};
		this.onViewport();
	}
	center(node: BoardNode): void {
		if (node.length === 0) return;
		const size = this.getSize();
		const point = node.position();
		this.viewportValue.pan = {
			x: size.width / 2 - point.x * this.viewportValue.zoom,
			y: size.height / 2 - point.y * this.viewportValue.zoom
		};
		this.onViewport();
	}
	destroy(): void {
		this.nodeItems.forEach((node) => node.destroy());
		this.nodeItems = [];
		this.edgeItems = [];
	}
}
