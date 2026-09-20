import {BaseGraph, EdgeStyle, ManhattanConnectorConfig} from '@maxgraph/core';

const finite = value => Number.isFinite(value) && Math.abs(value) <= 10000;
const validPoint = point => point && finite(point.x) && finite(point.y);
const same = (a, b) => Math.abs(a.x - b.x) < 0.11 && Math.abs(a.y - b.y) < 0.11;

// Validation, not path search. maxGraph may fall back to a connector that does
// not avoid obstacles. Such a path must not be advertised as a safe preview.
function clear(a, b, obstacles) {
  const vertical = Math.abs(a.x - b.x) < 0.001;
  const horizontal = Math.abs(a.y - b.y) < 0.001;
  if (!vertical && !horizontal) return false;
  return !obstacles.some(r => vertical
    ? a.x > r.minX && a.x < r.maxX && Math.min(a.y,b.y) < r.maxY && Math.max(a.y,b.y) > r.minY
    : a.y > r.minY && a.y < r.maxY && Math.min(a.x,b.x) < r.maxX && Math.max(a.x,b.x) > r.minX);
}

/** Bounded, disposable maxGraph projection; no source, history or file writes. */
export function routePreview(from, to, obstacles) {
  if (!validPoint(from) || !validPoint(to) || !Array.isArray(obstacles) || obstacles.length > 128 ||
      obstacles.some(r => ![r.minX,r.minY,r.maxX,r.maxY].every(finite) || r.minX > r.maxX || r.minY > r.maxY)) return null;
  const saved = {...ManhattanConnectorConfig};
  const graph = new BaseGraph({container: document.createElement('div'), plugins: []});
  graph.getView().rendering = false;
  try {
    Object.assign(ManhattanConnectorConfig, {
      step: 10, maxLoops: 1000,
      startDirections: [from.x <= to.x ? 'east' : 'west'],
      endDirections: [from.x <= to.x ? 'west' : 'east'],
    });
    let source, target;
    graph.batchUpdate(() => {
      for (const r of obstacles) graph.insertVertex({position:[r.minX,r.minY], size:[r.maxX-r.minX,r.maxY-r.minY]});
      source = graph.insertVertex({position:[from.x,from.y], size:[0,0]});
      target = graph.insertVertex({position:[to.x,to.y], size:[0,0]});
    });
    const edge = graph.insertEdge({source, target, style:{
      edgeStyle:EdgeStyle.ManhattanConnector, exitX:0.5, exitY:0.5,
      entryX:0.5, entryY:0.5, exitPerimeter:false, entryPerimeter:false,
    }});
    const raw = graph.getView().getState(edge)?.absolutePoints;
    if (!raw || raw.length < 2 || raw.length > 128 || !raw.every(validPoint) ||
        !same(raw[0],from) || !same(raw.at(-1),to)) return null;
    const points = raw.map(p => ({x:p.x,y:p.y}));
    points[0] = {...from}; points[points.length-1] = {...to};
    return points.slice(1).every((p,i) => clear(points[i],p,obstacles)) ? points : null;
  } finally {
    graph.destroy();
    Object.assign(ManhattanConnectorConfig, saved);
  }
}
