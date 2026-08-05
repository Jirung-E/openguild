import { describe, expect, it, vi } from 'vitest';

import {
	BOARD_NODE_HEIGHT,
	BOARD_NODE_WIDTH,
	BoardGraph,
	type BoardElementDefinition
} from './quest-board-model';

function makeGraph(elements?: BoardElementDefinition[]) {
	const onPosition = vi.fn();
	const onChange = vi.fn();
	const onViewport = vi.fn();
	const graph = new BoardGraph(
		elements ?? [
			{
				data: { id: 'q-1', questId: 1, statusId: 10 },
				position: { x: 100, y: 80 }
			},
			{
				data: { id: 'q-2', questId: 2, statusId: 20 },
				position: { x: 500, y: 280 }
			},
			{
				data: { id: 'pre-1-2', source: 'q-1', target: 'q-2', etype: 'pre' }
			}
		],
		() => ({ width: 1000, height: 600 }),
		onPosition,
		onChange,
		onViewport
	);
	return { graph, onPosition, onChange, onViewport };
}

describe('BoardGraph', () => {
	it('status selector와 selection collection을 제공한다', () => {
		const { graph } = makeGraph();
		expect(
			graph
				.nodes('[statusId = 10]')
				.toArray()
				.map((node) => node.id())
		).toEqual(['q-1']);
		graph.getElementById('q-2').select();
		expect(
			graph
				.nodes('[questId]:selected')
				.toArray()
				.map((node) => node.id())
		).toEqual(['q-2']);
	});

	it('position 변경은 해당 콜백만 호출한다', () => {
		const { graph, onPosition, onChange } = makeGraph();
		onPosition.mockClear();
		onChange.mockClear();
		const node = graph.getElementById('q-1');
		node.position({ x: 140, y: 90 });
		expect(onPosition).toHaveBeenCalledOnce();
		expect(onPosition).toHaveBeenCalledWith(node);
		expect(onChange).not.toHaveBeenCalled();
	});

	it('zoom은 지정한 화면 좌표 아래의 world point를 고정한다', () => {
		const { graph } = makeGraph();
		graph.viewport({ pan: { x: 20, y: 30 }, zoom: 1 });
		const anchor = { x: 220, y: 130 };
		graph.zoom({ level: 2, renderedPosition: anchor });
		const viewport = graph.viewportState();
		expect(viewport.zoom).toBe(2);
		expect(viewport.pan).toEqual({ x: -180, y: -70 });
	});

	it('fit은 node bounds를 viewport 중앙에 맞춘다', () => {
		const { graph } = makeGraph();
		graph.minZoom(0.02);
		graph.fit(undefined, 50);
		const box = graph.boundingBox();
		const viewport = graph.viewportState();
		const renderedCenter = {
			x: ((box.x1 + box.x2) / 2) * viewport.zoom + viewport.pan.x,
			y: ((box.y1 + box.y2) / 2) * viewport.zoom + viewport.pan.y
		};
		expect(renderedCenter.x).toBeCloseTo(500);
		expect(renderedCenter.y).toBeCloseTo(300);
	});

	it('숨긴 node는 bounding box에서 제외한다', () => {
		const { graph } = makeGraph();
		graph.getElementById('q-2').style('display', 'none');
		expect(graph.boundingBox()).toEqual({
			x1: 100 - BOARD_NODE_WIDTH / 2,
			y1: 80 - BOARD_NODE_HEIGHT / 2,
			x2: 100 + BOARD_NODE_WIDTH / 2,
			y2: 80 + BOARD_NODE_HEIGHT / 2,
			w: BOARD_NODE_WIDTH,
			h: BOARD_NODE_HEIGHT
		});
	});

	it('edge source/target과 missing node sentinel을 안전하게 반환한다', () => {
		const { graph } = makeGraph();
		const edge = graph.edges().toArray()[0];
		expect(edge.source().id()).toBe('q-1');
		expect(edge.target().id()).toBe('q-2');
		expect(graph.getElementById('missing').length).toBe(0);
	});

	it('500개 node에서도 viewport 변경은 node/graph 갱신을 유발하지 않는다', () => {
		const elements = Array.from({ length: 500 }, (_, index) => ({
			data: { id: `q-${index}`, questId: index, statusId: index % 5 },
			position: { x: (index % 25) * 320, y: Math.floor(index / 25) * 110 }
		}));
		const { graph, onPosition, onChange, onViewport } = makeGraph(elements);
		onPosition.mockClear();
		onChange.mockClear();
		onViewport.mockClear();

		graph.panBy({ x: 12, y: -8 });
		graph.zoom({ level: 0.8, renderedPosition: { x: 400, y: 240 } });

		expect(onViewport).toHaveBeenCalledTimes(2);
		expect(onPosition).not.toHaveBeenCalled();
		expect(onChange).not.toHaveBeenCalled();
	});
});
