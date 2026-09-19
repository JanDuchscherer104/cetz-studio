/* Obstacle routing v1. No layout, dependencies, DOM, or source rewriting.
 * Coordinates are downward-positive millimetres. See docs/edge-routing.md. */
'use strict';

const LIMITS = Object.freeze({nodes:80, edges:32, grid:30000, points:96, totalPoints:1536,
  expansions:100000, checks:8000000, milliseconds:750});
const EPS = 0.0001; // Larger than six-decimal source serialization error.
const PORTS = Object.freeze({east:[1,0], south:[0,1], west:[-1,0], north:[0,-1]});
const round = n => Math.round(n * 1e6) / 1e6;
const same = (a,b) => a.x === b.x && a.y === b.y;
const axis = (a,b) => a.y === b.y ? 0 : 1;
const length = (a,b) => Math.abs(a.x-b.x) + Math.abs(a.y-b.y);
const inside = (p,r) => p.x > r.left && p.x < r.right && p.y > r.top && p.y < r.bottom;

class RoutingError extends Error {
  constructor(code,message) { super(message); this.code=code; }
}
function fail(code,message) { throw new RoutingError(code,message); }
function finite(n) { return Number.isFinite(n) && Math.abs(n)<=100000; }
function intersect(a,b,r) {
  if(a.y===b.y) return a.y>r.top && a.y<r.bottom && Math.max(a.x,b.x)>r.left && Math.min(a.x,b.x)<r.right;
  if(a.x===b.x) return a.x>r.left && a.x<r.right && Math.max(a.y,b.y)>r.top && Math.min(a.y,b.y)<r.bottom;
  return true; // A diagonal is never a valid visibility edge.
}

// Stable binary heap. Sequence order breaks equal-cost ties reproducibly.
class Heap {
  constructor() { this.items=[]; this.sequence=0; }
  less(a,b) { return a.f<b.f || (a.f===b.f && a.order<b.order); }
  push(item) {
    item.order=this.sequence++;
    const a=this.items; let i=a.length; a.push(item);
    while(i>0) { const p=(i-1)>>1; if(!this.less(item,a[p]))break; a[i]=a[p]; i=p; }
    a[i]=item;
  }
  pop() {
    const a=this.items, first=a[0], last=a.pop();
    if(a.length) {
      let i=0;
      while(2*i+1<a.length) {
        let c=2*i+1; if(c+1<a.length && this.less(a[c+1],a[c]))c++;
        if(!this.less(a[c],last))break; a[i]=a[c]; i=c;
      }
      a[i]=last;
    }
    return first;
  }
  get size() { return this.items.length; }
}

class Budget {
  constructor(options={}) {
    this.expansions=0; this.checks=0; this.started=performance.now();
    this.maxExpansions=options.expansions??LIMITS.expansions;
    this.maxChecks=options.checks??LIMITS.checks;
    this.milliseconds=options.milliseconds??LIMITS.milliseconds;
    for(const [value,cap] of [[this.maxExpansions,LIMITS.expansions], [this.maxChecks,LIMITS.checks], [this.milliseconds,LIMITS.milliseconds]]) {
      if(!Number.isFinite(value)||value<0||value>cap)fail('invalid','Invalid routing work limit.');
    }
  }
  tick(expand=false) {
    if(expand)this.expansions++; else this.checks++;
    if(this.expansions>this.maxExpansions || this.checks>this.maxChecks ||
      ((expand || this.checks%128===0) && performance.now()-this.started>this.milliseconds)) {
      fail('exhausted','Routing work limit reached; reduce the selection or simplify the diagram.');
    }
  }
}

function clearSegment(a,b,obstacles,budget,except=null) {
  for(const r of obstacles) { budget.tick(); if(r.id!==except && intersect(a,b,r))return false; }
  return true;
}
function prepareNodes(nodes,clearance) {
  if(!Array.isArray(nodes)||!nodes.length||nodes.length>LIMITS.nodes)fail('limit',`Routing supports 1–${LIMITS.nodes} measured nodes.`);
  const ids=new Set();
  return nodes.map(n=>{
    if(typeof n.id!=='string'||ids.has(n.id)||![n.x,n.y,n.w,n.h].every(finite)||n.w<=0||n.h<=0)fail('invalid','Missing, duplicate, or invalid node bounds.');
    ids.add(n.id);
    const r={id:n.id, x:n.x, y:n.y, w:n.w, h:n.h,
      left:Math.floor((n.x-clearance-EPS)*1e6)/1e6,
      right:Math.ceil((n.x+n.w+clearance+EPS)*1e6)/1e6,
      top:Math.floor((n.y-clearance-EPS)*1e6)/1e6,
      bottom:Math.ceil((n.y+n.h+clearance+EPS)*1e6)/1e6};
    if(![r.left,r.right,r.top,r.bottom].every(finite))fail('invalid','Inflated node bounds exceed the coordinate limit.');
    return r;
  });
}

function escapes(endpoint,nodes,budget) {
  const r=nodes.find(n=>n.id===endpoint?.node);
  if(!r)fail('invalid','Cannot resolve an attachment node.');
  const names=endpoint.port==='auto'?Object.keys(PORTS):[endpoint.port];
  return names.flatMap(port=>{
    const normal=PORTS[port]; if(!normal)fail('unsupported','Only automatic and cardinal ports can be routed.');
    let p={x:round(r.x+r.w/2+normal[0]*r.w/2),y:round(r.y+r.h/2+normal[1]*r.h/2)};
    // Explicit anchors are measured by the real compiler, not inferred from IDs.
    if(endpoint.port!=='auto' && endpoint.point) {
      if(![endpoint.point.x,endpoint.point.y].every(finite))fail('invalid','Invalid measured port.');
      p={x:round(endpoint.point.x),y:round(endpoint.point.y)};
      if(p.x<r.x-EPS || p.x>r.x+r.w+EPS || p.y<r.y-EPS || p.y>r.y+r.h+EPS) {
        fail('unsupported','The measured attachment is outside its node bounds.');
      }
    }
    const q={x:normal[0]>0?r.right:normal[0]<0?r.left:p.x,
      y:normal[1]>0?r.bottom:normal[1]<0?r.top:p.y};
    if(same(p,q)||!clearSegment(p,q,nodes,budget,r.id))return [];
    return [{port,point:p,escape:q,axis:normal[0]?0:1,cost:length(p,q)}];
  });
}
function unique(values) { return [...new Set(values)].sort((a,b)=>a-b); }
function simplify(points) {
  const out=[];
  for(const p of points) {
    if(out.length && same(out.at(-1),p))continue;
    while(out.length>1 && axis(out.at(-2),out.at(-1))===axis(out.at(-1),p))out.pop();
    out.push(p);
  }
  return out;
}

function routeEdge(edge,nodes,budget,bendPenalty) {
  if(edge.start?.node===edge.end?.node)fail('unsupported','Self loops require manual routing.');
  const starts=escapes(edge.start,nodes,budget),ends=escapes(edge.end,nodes,budget);
  if(!starts.length||!ends.length)fail('impossible',`${edge.edge}: no clear escape from the selected port at this clearance.`);
  const ports=[...starts,...ends],xs=unique([...nodes.flatMap(r=>[r.left,r.right]),...ports.map(p=>p.escape.x)]),
    ys=unique([...nodes.flatMap(r=>[r.top,r.bottom]),...ports.map(p=>p.escape.y)]);
  // An outer lane permits routing around the complete diagram, never clipping to
  // the old page bounds. Typst may grow the page without moving any source node.
  const margin=2;
  xs.unshift(round(xs[0]-margin)); xs.push(round(xs.at(-1)+margin));
  ys.unshift(round(ys[0]-margin)); ys.push(round(ys.at(-1)+margin));
  if(![...xs,...ys].every(finite))fail('invalid','Routing lanes exceed the coordinate limit.');
  const width=xs.length,count=width*ys.length;
  if(count>LIMITS.grid)fail('exhausted','Visibility grid exceeds the routing work limit.');
  const xIndex=new Map(xs.map((x,i)=>[x,i])),yIndex=new Map(ys.map((y,i)=>[y,i]));
  const index=p=>yIndex.get(p.y)*width+xIndex.get(p.x);
  const point=i=>({x:xs[i%width],y:ys[Math.floor(i/width)]});
  const blocked=new Int8Array(count); blocked.fill(-1);
  function free(i) {
    if(blocked[i]===-1) {
      const p=point(i); blocked[i]=0;
      for(const r of nodes) { budget.tick(); if(inside(p,r)){blocked[i]=1;break;} }
    }
    return blocked[i]===0;
  }
  // Cache visibility of lattice segments; collision checks are shared between
  // the two heading states. No raster step or pixel-resolution dependency.
  const horizontal=new Int8Array(count),vertical=new Int8Array(count);
  horizontal.fill(-1);vertical.fill(-1);
  function visible(a,b,dir) {
    const key=Math.min(a,b),cache=dir===0?horizontal:vertical;
    if(cache[key]===-1)cache[key]=clearSegment(point(a),point(b),nodes,budget)?1:0;
    return cache[key]===1;
  }
  const costs=new Float64Array(count*2);costs.fill(Infinity);
  const parent=new Int32Array(count*2);parent.fill(-1);
  const origin=new Int8Array(count*2);origin.fill(-1);
  const heap=new Heap();
  const heuristic=p=>Math.min(...ends.map(e=>length(p,e.escape)+e.cost));
  starts.forEach((s,j)=>{
    const i=index(s.escape),state=i*2+s.axis;
    if(free(i)&&s.cost<costs[state]) {
      costs[state]=s.cost;origin[state]=j;
      heap.push({state,g:s.cost,f:s.cost+heuristic(s.escape)});
    }
  });
  const targets=ends.map(e=>index(e.escape));
  let best=null;
  while(heap.size) {
    budget.tick(true);
    const item=heap.pop();
    if(item.g!==costs[item.state])continue;
    if(best && item.f>=best.cost)break;
    const i=Math.floor(item.state/2),heading=item.state%2,p=point(i);
    ends.forEach((e,j)=>{
      if(i!==targets[j])return;
      const cost=item.g+e.cost+(heading===e.axis?0:bendPenalty);
      if(!best||cost<best.cost)best={state:item.state,end:j,cost};
    });
    const x=i%width,y=Math.floor(i/width);
    for(const [next,dir,ok] of [[i+1,0,x+1<width],[i+width,1,y+1<ys.length],[i-1,0,x>0],[i-width,1,y>0]]) {
      if(!ok||!free(next)||!visible(i,next,dir))continue;
      const state=next*2+dir,g=item.g+length(p,point(next))+(dir===heading?0:bendPenalty);
      if(g>=costs[state])continue;
      costs[state]=g;parent[state]=item.state;origin[state]=-1;
      heap.push({state,g,f:g+heuristic(point(next))});
    }
  }
  if(!best)fail('impossible',`${edge.edge}: no obstacle-free route for the selected ports and clearance.`);
  const lattice=[];let state=best.state;
  while(parent[state]!==-1) { lattice.push(point(Math.floor(state/2)));state=parent[state]; }
  lattice.push(point(Math.floor(state/2)));lattice.reverse();
  const start=starts[origin[state]],end=ends[best.end];
  let interior=simplify(lattice);
  // Escape points are retained even when collinear, so bare <node> anchors attach
  // on the chosen face. Existing source tuples are reused, never deleted.
  const minimum=Math.max(1,edge.minimum_points??0);
  if(!Number.isInteger(minimum)||minimum>LIMITS.points)fail('limit','Too many existing waypoints to route safely.');
  while(interior.length<minimum) {
    let longest=-1,at=-1;
    for(let i=0;i+1<interior.length;i++) { const d=length(interior[i],interior[i+1]);if(d>longest){longest=d;at=i;} }
    if(at<0||longest<4e-6)fail('impossible','The route is too short to retain its existing waypoint literals.');
    const a=interior[at],b=interior[at+1],middle={x:round((a.x+b.x)/2),y:round((a.y+b.y)/2)};
    interior.splice(at+1,0,middle);
  }
  if(interior.length>LIMITS.points)fail('exhausted','Route exceeds the waypoint limit.');
  const preview=[start.point,...interior,end.point];
  // Check the serialized result, not just search vertices. This is also a guard
  // against regressions in simplification/resampling.
  for(let i=0;i+1<preview.length;i++) {
    const except=i===0?edge.start.node:i===preview.length-2?edge.end.node:null;
    if(!clearSegment(preview[i],preview[i+1],nodes,budget,except))fail('impossible','Rounded route failed obstacle verification.');
  }
  return {edge:edge.edge,points:interior.map(p=>[p.x,p.y]),preview,start_port:start.port,end_port:end.port};
}

function routeBatch(input) {
  const started=performance.now();
  const budget=new Budget(input.limits);
  if(!Number.isFinite(input.clearance)||input.clearance<0||input.clearance>20)fail('invalid','Clearance must be between 0 and 20 mm.');
  const penalty=input.bend_penalty??5;
  if(!Number.isFinite(penalty)||penalty<0||penalty>100)fail('invalid','Invalid bend penalty.');
  const nodes=prepareNodes(input.nodes,input.clearance);
  if(!Array.isArray(input.edges)||!input.edges.length||input.edges.length>LIMITS.edges)fail('limit',`Select 1–${LIMITS.edges} eligible edges.`);
  const ids=new Set();
  const routes=input.edges.map(edge=>{
    if(typeof edge.edge!=='string'||ids.has(edge.edge))fail('invalid','Duplicate or invalid edge identity.');
    ids.add(edge.edge);return routeEdge(edge,nodes,budget,penalty);
  });
  if(routes.reduce((n,r)=>n+r.points.length,0)>LIMITS.totalPoints)fail('limit','Route batch exceeds the waypoint transport limit; reduce the selection.');
  return {routes,elapsed_ms:performance.now()-started,expansions:budget.expansions,checks:budget.checks};
}

// The same pure engine is exercised by Node's built-in test runner and the real
// browser Worker. No npm packages or production build step are needed.
if(typeof module!=='undefined' && module.exports)module.exports={routeBatch,LIMITS,intersect};
if(typeof WorkerGlobalScope!=='undefined' && self instanceof WorkerGlobalScope) {
  self.onmessage=event=>{
    const {job,input}=event.data;
    try { self.postMessage({job,result:routeBatch(input)}); }
    catch(error) { self.postMessage({job,error:error.message,code:error.code||'failed'}); }
  };
}
