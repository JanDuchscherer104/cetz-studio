import {BaseGraph, EdgeStyle, ManhattanConnectorConfig} from '@maxgraph/core';

const finite = value => Number.isFinite(value) && Math.abs(value) <= 10000;
const validPoint = point => point && finite(point.x) && finite(point.y);
const same = (a, b) => Math.abs(a.x - b.x) < 0.11 && Math.abs(a.y - b.y) < 0.11;
const directions = new Set(['east', 'west', 'north', 'south']);
const horizontal = direction => direction === 'east' || direction === 'west';
const outward = (a, b, direction) => horizontal(direction)
  ? Math.abs(a.y-b.y) < 0.001 && (b.x-a.x)*(direction === 'east' ? 1 : -1) > 0
  : Math.abs(a.x-b.x) < 0.001 && (b.y-a.y)*(direction === 'south' ? 1 : -1) > 0;

// Validation, not path search. maxGraph can fall back to a connector that does
// not avoid obstacles; a failed or unsafe result must not become a display hint.
function clear(a, b, obstacles) {
  const vertical = Math.abs(a.x - b.x) < 0.001;
  const isHorizontal = Math.abs(a.y - b.y) < 0.001;
  if (!vertical && !isHorizontal) return false;
  return !obstacles.some(r => vertical
    ? a.x > r.minX && a.x < r.maxX && Math.min(a.y,b.y) < r.maxY && Math.max(a.y,b.y) > r.minY
    : a.y > r.minY && a.y < r.maxY && Math.min(a.x,b.x) < r.maxX && Math.max(a.x,b.x) > r.minX);
}

/** A disposable graph projection, never a second source document or history. */
export function routePreview(from, to, obstacles, ports = {}) {
  if (!ports || typeof ports !== 'object') return null;
  if (!validPoint(from) || !validPoint(to) || !Array.isArray(obstacles) || obstacles.length > 128 ||
      obstacles.some(r => !r || ![r.minX,r.minY,r.maxX,r.maxY].every(finite) || r.minX > r.maxX || r.minY > r.maxY)) return null;
  const xs = [from.x,to.x,...obstacles.flatMap(r => [r.minX,r.maxX])];
  const ys = [from.y,to.y,...obstacles.flatMap(r => [r.minY,r.maxY])];
  if (Math.max(...xs)-Math.min(...xs) > 4096 || Math.max(...ys)-Math.min(...ys) > 4096) return null;
  const dx = to.x-from.x, dy = to.y-from.y;
  const defaultStart = Math.abs(dx) >= Math.abs(dy) ? (dx >= 0 ? 'east' : 'west') : (dy >= 0 ? 'south' : 'north');
  const opposite = {east:'west',west:'east',north:'south',south:'north'};
  const start = ports.start ?? defaultStart, end = ports.end ?? opposite[defaultStart];
  if (!directions.has(start) || !directions.has(end)) return null;
  const saved = {...ManhattanConnectorConfig};
  const graph = new BaseGraph({container: document.createElement('div'), plugins: []});
  graph.getView().rendering = false;
  try {
    Object.assign(ManhattanConnectorConfig, {
      step: 10, maxLoops: 1000, startDirections: [start], endDirections: [end],
    });
    const anchors = {east:[1,0.5],west:[0,0.5],north:[0.5,0],south:[0.5,1]};
    const terminal = (point, direction) => {
      const [x,y] = anchors[direction];
      // Non-degenerate terminal geometry keeps upstream fallback port inference
      // well-defined; its requested side is exactly the measured endpoint.
      return graph.insertVertex({position:[point.x-x*4,point.y-y*4], size:[4,4], style:{portConstraint:direction}});
    };
    let source, target;
    graph.batchUpdate(() => {
      for (const r of obstacles) graph.insertVertex({position:[r.minX,r.minY], size:[r.maxX-r.minX,r.maxY-r.minY]});
      source = terminal(from,start);
      target = terminal(to,end);
    });
    const edge = graph.insertEdge({source, target, style:{
      edgeStyle:EdgeStyle.ManhattanConnector, sourcePortConstraint:start, targetPortConstraint:end,
      exitX:anchors[start][0], exitY:anchors[start][1],
      entryX:anchors[end][0], entryY:anchors[end][1], exitPerimeter:false, entryPerimeter:false,
    }});
    const raw = graph.getView().getState(edge)?.absolutePoints;
    if (!raw || raw.length < 2 || raw.length > 128 || !raw.every(validPoint) ||
        !same(raw[0],from) || !same(raw.at(-1),to)) return null;
    const points = raw.map(p => ({x:p.x,y:p.y}));
    points[0] = {...from}; points[points.length-1] = {...to};
    // maxGraph rounds projected points to 0.1 units. Restore only the adjacent
    // attachment coordinate, then validate every segment against the obstacles.
    if (points.length > 2) {
      const startAxis = horizontal(start) ? 'y' : 'x', endAxis = horizontal(end) ? 'y' : 'x';
      if (Math.abs(points[1][startAxis]-from[startAxis]) > 0.11 ||
          Math.abs(points[points.length-2][endAxis]-to[endAxis]) > 0.11) return null;
      points[1][horizontal(start) ? 'y' : 'x'] = from[horizontal(start) ? 'y' : 'x'];
      points[points.length-2][horizontal(end) ? 'y' : 'x'] = to[horizontal(end) ? 'y' : 'x'];
    }
    if (!outward(from,points[1],start) || !outward(to,points.at(-2),end)) return null;
    return points.slice(1).every((p,i) => clear(points[i],p,obstacles)) ? points : null;
  } catch {
    // Optional preview only: an upstream geometry failure cannot stop dragging
    // or trigger a fallback handwritten search/source change.
    return null;
  } finally {
    graph.destroy();
    Object.assign(ManhattanConnectorConfig, saved);
  }
}
