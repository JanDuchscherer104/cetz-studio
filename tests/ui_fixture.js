/* Test-only API fixture. This is NOT a Rust parser, a Typst compiler, or a save
 * implementation. It records UI commands and moves synthetic SVG geometry. */
window.CETZ_STUDIO_FIXTURE = true;
(() => {
  const clone=x=>JSON.parse(JSON.stringify(x));
  const names=[['physical',20,12,'Physical inputs','poses · scene statistics'],['target',71,12,'Target relation','candidate-to-target pose'],['state',124,12,'Shared state','scene · target · history'],['trunk',20,40,'Physical trunk','Linear · GELU · norm'],['query',71,40,'Value embedding','uᵢ ∈ ℝᵈ'],['tokens',124,40,'State encoders','Z ∈ ℝ⁵ˣᵈ'],['feasibility',20,65,'Feasibility','auxiliary prediction'],['attention',95,65,'Cross-attention','independent queries'],['concat',95,90,'Concatenate','[u, c, u ⊙ c]'],['decoder',95,113,'Scalar decoder','Linear · GELU · Linear'],['value',144,113,'Conditional value','Qₕ'],['query-fork',71,55,'','']];
  const anchor=name=>({kind:'anchor',name});
  const point=(x,y)=>({kind:'point',point:{x,y,editable:true}});
  const specifications=[['physical','trunk'],['target','query'],['trunk','query'],['state','tokens'],['trunk','feasibility'],['query','query-fork'],['query-fork',[71,65],'attention.west'],['tokens',[124,65],'attention.east'],['query-fork',[52,55],[52,90],'concat.west'],['attention','concat'],['concat','decoder'],['decoder','value']];
  const labels=['','','','','','Nq × d','Q','K, V','uᵢ','cᵢ','Nq × 3d','Nq × 1'];
  let state={filename:'demo.typ',revision:0,disk_hash:'UI_FIXTURE_NOT_A_DISK_HASH',source:window.CETZ_STUDIO_DEMO_SOURCE||'// UI fixture. Open examples/demo.typ with the Rust server.\n',diagram:{nodes:names.map(([id,x,y,title,body],i)=>({id,title,body,kind:id==='query-fork'?'junction':'n',position:{x,y,editable:true},editable:true,line:34+i})),edges:specifications.map((v,i)=>({id:`e${i}`,vertices:v.map(x=>Array.isArray(x)?point(...x):anchor(x)),has_label:!!labels[i],editable:true,label_position:i===8?[1,.5]:[0,.5],line:46+i})),warnings:['UI fixture only. Geometry is synthetic, not compiled from Typst.'],y_scale:1},svg:null,diagnostics:'No Rust or Typst process is running in this fixture.',diff:'',dirty:false,undo:false,redo:false,preview_current:true};
  const history=[],future=[];window.cetzStudioFixtureRequests=[];
  const escape=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&apos;'}[c]));
  function endpoint(v){if(v.kind==='point')return v.point;const n=state.diagram.nodes.filter(n=>v.name===n.id||v.name.startsWith(`${n.id}.`)).sort((a,b)=>b.id.length-a.id.length)[0];if(!n)return{x:0,y:0};const p={...n.position},port=v.name.slice(n.id.length);if(port.includes('west'))p.x-=17;if(port.includes('east'))p.x+=17;if(port.includes('north'))p.y-=6.5;if(port.includes('south'))p.y+=6.5;return p;}
  function render(){
    const S=72/25.4;
    let svg=`<svg xmlns="http://www.w3.org/2000/svg" width="520pt" height="405pt" viewBox="0 0 520 405"><path fill="#ffffff" d="M 0 0v 405 h 520 v -405 Z"/><text x="260" y="24" text-anchor="middle" fill="#26374b" font-family="Georgia,serif" font-weight="bold" font-size="13">Candidate scoring · layout fixture</text><g transform="translate(24 45) scale(${S})">`;
    const rect=(x,y,w,h,color)=>`<rect x="${x-w/2}" y="${y-h/2}" width="${w}" height="${h}" stroke="#${color}" stroke-width="0.00012345" fill="none"/>`;
    svg+=rect(0,0,.02,.02,'a00000')+rect(10,0,.02,.02,'a00001')+rect(0,10,.02,.02,'a00002');
    for(const [i,e] of state.diagram.edges.entries()){
      const p=e.vertices.map(endpoint);
      svg+=`<polyline points="${p.map(p=>`${p.x},${p.y}`).join(' ')}" fill="none" stroke="#536475" stroke-width=".25" stroke-linejoin="round"/>`;
      p.forEach((p,j)=>svg+=rect(p.x,p.y,.02,.02,`a2${(i*256+j).toString(16).padStart(4,'0')}`));
      if(labels[i]){const seg=e.label_position[0],f=e.label_position[1],a=p[seg],b=p[seg+1],x=a.x+(b.x-a.x)*f,y=a.y+(b.y-a.y)*f;svg+=`<rect x="${x-6}" y="${y-2.3}" width="12" height="4.6" fill="white"/><text x="${x}" y="${y+1}" text-anchor="middle" font-family="Georgia,serif" font-style="italic" font-size="2.9" fill="#334155">${escape(labels[i])}</text>`+rect(x,y,12,4.6,`a3${i.toString(16).padStart(4,'0')}`);}
    }
    for(const [i,n] of state.diagram.nodes.entries()){
      const {x,y}=n.position,w=n.kind==='junction'?.7:34,h=n.kind==='junction'?.7:13;
      const fill=n.kind==='junction'?'#334155':i<3?'#d8eadd':[6,10].includes(i)?'#f7ded8':i===8?'#fbf0c8':'#e5dded';
      svg+=`<rect x="${x-w/2}" y="${y-h/2}" width="${w}" height="${h}" rx="1.2" fill="${fill}" stroke="#9ba8ae" stroke-width=".22"/>`;
      if(n.title)svg+=`<text x="${x}" y="${y-.5}" text-anchor="middle" font-family="Georgia,serif" font-size="3.1" font-weight="bold" fill="#27384b">${escape(n.title)}</text><text x="${x}" y="${y+3.2}" text-anchor="middle" font-family="Georgia,serif" font-size="2.65" fill="#465567">${escape(n.body)}</text>`;
      svg+=rect(x,y,w,h,`a1${i.toString(16).padStart(4,'0')}`);
    }
    return svg+'</g><text x="260" y="392" text-anchor="middle" font-family="sans-serif" font-size="8" fill="#788494">Synthetic UI fixture · not a Typst render</text></svg>';
  }
  const response=(data,status=200)=>new Response(JSON.stringify(data),{status,headers:{'Content-Type':'application/json'}});
  function snapshot(){state.svg=render();state.undo=history.length>0;state.redo=future.length>0;return clone(state);}
  window.fetch=async(url,options={})=>{
    if(url==='/api/state')return response({token:'ui-fixture',snapshot:snapshot()});
    const body=JSON.parse(options.body||'{}');window.cetzStudioFixtureRequests.push({url,body});
    if(window.CETZ_STUDIO_TEST_FAIL_NEXT){window.CETZ_STUDIO_TEST_FAIL_NEXT=false;return response({error:'Injected compiler failure · UI test fixture',snapshot:snapshot()},409);}
    if(url==='/api/save')return response({error:'Saving is disabled in the UI fixture.',snapshot:snapshot()},409);
    if(body.revision!==state.revision)return response({error:'Stale fixture revision',snapshot:snapshot()},409);
    if(url==='/api/edit'){
      history.push(clone(state));future.length=0;const c=body.command;
      if(c.kind==='move_node'){const n=state.diagram.nodes.find(n=>n.id===c.id);n.position.x=c.x;n.position.y=c.y;}
      if(c.kind==='move_waypoint'){const p=state.diagram.edges.find(e=>e.id===c.edge).vertices[c.vertex].point;p.x=c.x;p.y=c.y;}
      if(c.kind==='move_segment'){const e=state.diagram.edges.find(e=>e.id===c.edge),a=e.vertices[c.segment].point,b=e.vertices[c.segment+1].point,axis=Math.abs(a.x-b.x)<1e-6?'x':'y';a[axis]+=c.delta;b[axis]+=c.delta;}
      if(c.kind==='set_label')state.diagram.edges.find(e=>e.id===c.edge).label_position=[c.segment,c.fraction];
      if(c.kind==='set_port'){const e=state.diagram.edges.find(e=>e.id===c.edge),v=c.end==='start'?e.vertices[0]:e.vertices.at(-1),n=state.diagram.nodes.filter(n=>v.name===n.id||v.name.startsWith(`${n.id}.`)).sort((a,b)=>b.id.length-a.id.length)[0];v.name=n.id+(c.port==='auto'?'':`.${c.port}`);}
      state.revision++;state.dirty=true;state.diff='@@ UI command log: NOT a Typst source diff @@\n+ '+JSON.stringify(c)+'\n';
    }else if(url==='/api/undo'&&history.length){const rev=state.revision;future.push(clone(state));state=history.pop();state.revision=rev+1;}
    else if(url==='/api/redo'&&future.length){const rev=state.revision;history.push(clone(state));state=future.pop();state.revision=rev+1;}
    return response({snapshot:snapshot()});
  };
})();
