'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const {routeBatch,intersect,LIMITS}=require('../web/routing-worker.js');
const nodes=[{id:'a',x:0,y:0,w:10,h:10},{id:'obstacle',x:30,y:-5,w:20,h:20},{id:'b',x:70,y:0,w:10,h:10}];
const edge={edge:'e0',start:{node:'a',port:'east'},end:{node:'b',port:'west'},minimum_points:0};
const input=(extra={})=>({nodes:structuredClone(nodes),edges:[structuredClone(edge)],clearance:2,...extra});
function avoids(route,box,clearance) {
  const r={left:box.x-clearance,right:box.x+box.w+clearance,top:box.y-clearance,bottom:box.y+box.h+clearance};
  for(let i=0;i+1<route.preview.length;i++)assert.equal(intersect(route.preview[i],route.preview[i+1],r),false);
}

test('avoids an intervening node with explicit ports and clearance',()=>{
  const r=routeBatch(input()).routes[0];
  assert.deepEqual(r.preview[0],{x:10,y:5});assert.deepEqual(r.preview.at(-1),{x:70,y:5});
  assert.ok(r.preview[1].x>10);assert.ok(r.preview.at(-2).x<70);
  avoids(r,nodes[1],2);
  for(let i=0;i+1<r.preview.length;i++)assert.ok(r.preview[i].x===r.preview[i+1].x||r.preview[i].y===r.preview[i+1].y);
});
test('does not mutate node positions, inputs, or labels',()=>{
  const request=input();request.edges[0].label='$Q_h$';const before=JSON.stringify(request);
  routeBatch(request);assert.equal(JSON.stringify(request),before);
});
test('is deterministic and supports automatic ports',()=>{
  const request=input();request.edges[0].start.port='auto';request.edges[0].end.port='auto';
  assert.deepEqual(routeBatch(request).routes,routeBatch(request).routes);
  avoids(routeBatch(request).routes[0],nodes[1],2);
});
test('honours a measured non-central cardinal port',()=>{
  const request=input();request.edges[0].start.point={x:10,y:3};
  const r=routeBatch(request).routes[0];assert.deepEqual(r.preview[0],{x:10,y:3});assert.equal(r.preview[1].y,3);
});
test('blocked explicit escape is impossible, not a fallback to another port',()=>{
  const request=input();request.nodes.push({id:'block',x:10.5,y:0,w:3,h:10});
  assert.throws(()=>routeBatch(request),e=>e.code==='impossible');
});
test('bounded expansion, collision and time exhaustion',()=>{
  for(const limits of [{expansions:0},{checks:0},{milliseconds:0}])assert.throws(()=>routeBatch(input({limits})),e=>e.code==='exhausted');
});
test('all-or-nothing batch: no partial return on impossible edge',()=>{
  const request=input();request.edges.push({...edge,edge:'e1',end:{node:'obstacle',port:'west'}});
  request.nodes.push({id:'block',x:10.5,y:0,w:3,h:10});
  assert.throws(()=>routeBatch(request),e=>e.code==='impossible');
});
test('retains enough collinear vertices to reuse existing source tuples',()=>{
  const request=input();request.edges[0].minimum_points=20;
  const r=routeBatch(request).routes[0];assert.equal(r.points.length,20);avoids(r,nodes[1],2);
  assert.ok(r.points.every((p,i)=>!i||p[0]!==r.points[i-1][0]||p[1]!==r.points[i-1][1]));
});
test('rejects unsupported, malformed, duplicate and oversized requests',()=>{
  for(const clearance of [-1,21,NaN,Infinity])assert.throws(()=>routeBatch(input({clearance})));
  for(const port of ['north-east','unresolved']) { const request=input();request.edges[0].start.port=port;assert.throws(()=>routeBatch(request),e=>e.code==='unsupported'); }
  assert.throws(()=>routeBatch(input({edges:[edge,edge]})),e=>e.code==='invalid');
  assert.throws(()=>routeBatch(input({nodes:[...nodes,nodes[0]]})),e=>e.code==='invalid');
  assert.throws(()=>routeBatch(input({edges:Array.from({length:LIMITS.edges+1},(_,i)=>({...edge,edge:String(i)}))})),e=>e.code==='limit');
  const request=input();request.edges[0].end.node='a';assert.throws(()=>routeBatch(request),e=>e.code==='unsupported');
});
test('routes a selected batch and does not return other edges',()=>{
  const request=input();request.edges.push({...edge,edge:'e1',start:{node:'a',port:'south'},end:{node:'b',port:'south'}});
  const r=routeBatch(request);assert.deepEqual(r.routes.map(e=>e.edge),['e0','e1']);
  r.routes.forEach(route=>avoids(route,nodes[1],2));
});
test('ports directed away from each other go around endpoint obstacles',()=>{
  const request=input();request.edges[0].start.port='west';request.edges[0].end.port='east';
  const r=routeBatch(request).routes[0];assert.ok(r.preview[1].x<0);assert.ok(r.preview.at(-2).x>80);avoids(r,nodes[1],2);
});
test('representative latency evidence (not a universal performance threshold)',()=>{
  const samples=[];
  for(let i=0;i<30;i++)samples.push(routeBatch(input()).elapsed_ms);
  samples.sort((a,b)=>a-b);
  console.log(JSON.stringify({benchmark:'3 nodes, 1 explicit-port obstacle route',runs:samples.length,median_ms:samples[15],p95_ms:samples[28],node:process.version}));
});

test('shared escape point works without manufacturing a duplicate waypoint',()=>{
  const request={nodes:[{id:'a',x:0,y:0,w:10,h:10},{id:'b',x:14.0002,y:0,w:10,h:10}],edges:[edge],clearance:2};
  const r=routeBatch(request).routes[0];assert.equal(r.points.length,1);
});
test('80-node inputs still honor the collision budget',()=>{
  const many=Array.from({length:80},(_,i)=>({id:`n${i}`,x:i*20,y:i*17,w:10,h:10}));
  const e={edge:'e0',start:{node:'n0',port:'auto'},end:{node:'n79',port:'auto'}};
  // Automatic escapes add lanes, but all work is still bounded, even when this
  // particular grid fits. A zero collision budget cannot allocate an unbounded search.
  assert.throws(()=>routeBatch({nodes:many,edges:[e],clearance:2,limits:{checks:0}}),e=>e.code==='exhausted');
});
test('late batch failure does not return an earlier successful route',()=>{
  const request=input();request.nodes.push({id:'block',x:-3,y:0,w:2,h:10});
  request.edges.push({...edge,edge:'e1',start:{node:'a',port:'west'}});
  assert.throws(()=>routeBatch(request),e=>e.code==='impossible');
});
