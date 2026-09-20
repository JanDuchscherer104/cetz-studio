/* Optional route proposals. Source rules, search and adoption remain Rust-owned. */
window.CetzRouting = {
  create({app, post, notify, drawOverlay, worldToSvg}) {
    const $=id=>document.getElementById(id);
    const pane=$('routing-panel');
    if(!pane)return {refresh(){},draw(){},selectionChanged(){}};
    let status=null, proposal=null, identity='', generation=0, timer=null, requesting=false;
    // Drag routing is deliberately display-only. It uses the already measured SVG
    // rectangles and never sends browser geometry to the source-editing backend.
    // The normal Rust proposal remains the only route that can be adopted.
    let liveRoutes=[], liveFrame=0;
    const checked=new Set();
    const signature=()=>`${app.sessionId}:${app.snapshot?.revision}`;
    const matches=data=>data?.identity?.session_id===app.sessionId&&data?.identity?.revision===app.snapshot?.revision;
    function selectedIds(){
      if($('routing-scope').value==='all')return (status?.eligibility||[]).filter(e=>e.eligible).map(e=>e.edge);
      if($('routing-scope').value==='checked')return [...checked];
      return app.selection?.kind==='edge'?[app.selection.id]:[];
    }
    function paint(){
      const running=!!status?.routing?.running;
      const ids=selectedIds(), eligibility=new Map((status?.eligibility||[]).map(e=>[e.edge,e]));
      const unsupported=ids.some(id=>!eligibility.get(id)?.eligible);
      const tooMany=ids.length>32;
      $('routing-clearance').disabled=requesting||running;
      $('routing-preview').disabled=app.fixture||app.busy||requesting||running||!status?.available||!app.basis||!ids.length||tooMany||unsupported;
      $('routing-apply').disabled=app.busy||requesting||!proposal||!matches(proposal);
      $('routing-discard').disabled=requesting||(!running&&!proposal);
      $('routing-choices').hidden=$('routing-scope').value!=='checked';
      const failure=status?.error||status?.routing?.error;
      $('routing-status').textContent=failure|| (running ? (status.routing.cancelled?'Discarded; measurement worker is finishing.':'Measuring and routing… Canvas navigation stays available.') : proposal ?
        `${proposal.plan.routes.length} proposed route(s) · ${proposal.plan.search_ms.toFixed(1)} ms search. Apply to validate; Save writes later.` :
        !status?.available?'Automatic routing requires a current, single-page Fletcher preview.':'Nodes stay fixed. Routes are independent; crossings and labels are not avoided.');
      $('routing-panel').classList.toggle('has-proposal',!!proposal);
      const selected=(status?.eligibility||[]).find(e=>e.edge===app.selection?.id);
      $('routing-reason').textContent=tooMany?'Select at most 32 edges per routing proposal. Use Checked edges to choose a smaller batch.':$('routing-scope').value==='selected'&&selected&&!selected.eligible?selected.reason||'Unsupported edge':'';
    }
    function choices(){
      const list=$('routing-choices');list.replaceChildren();
      for(const item of status?.eligibility||[]){
        const label=document.createElement('label'), input=document.createElement('input');
        input.type='checkbox';input.value=item.edge;input.disabled=!item.eligible;input.checked=checked.has(item.edge);
        input.onchange=()=>{if(input.checked)checked.add(item.edge);else checked.delete(item.edge);paint();};
        label.append(input,document.createTextNode(item.edge+(item.eligible?'':` · ${item.reason}`)));list.append(label);
      }
    }
    const rect=(box,dx=0,dy=0)=>({minX:box.x+dx,minY:box.y+dy,maxX:box.x+box.w+dx,maxY:box.y+box.h+dy});
    const inside=(p,r)=>p.x>r.minX&&p.x<r.maxX&&p.y>r.minY&&p.y<r.maxY;
    function clear(a,b,obstacles){
      if(Math.abs(a.x-b.x)>1e-5&&Math.abs(a.y-b.y)>1e-5)return false;
      return !obstacles.some(r=>{
        if(Math.abs(a.x-b.x)<1e-5)return a.x>r.minX&&a.x<r.maxX&&Math.min(a.y,b.y)<r.maxY&&Math.max(a.y,b.y)>r.minY;
        return a.y>r.minY&&a.y<r.maxY&&Math.min(a.x,b.x)<r.maxX&&Math.max(a.x,b.x)>r.minX;
      });
    }
    // A small deterministic visibility-grid A* for ephemeral drag feedback.
    // It is intentionally bounded to incident edges and is not a substitute for
    // the Rust planner's measured, source-patchable proposal.
    function livePath(from,to,obstacles){
      const xs=[from.x,to.x],ys=[from.y,to.y];
      for(const r of obstacles){xs.push(r.minX,r.maxX);ys.push(r.minY,r.maxY);}
      const x=[...new Set(xs.map(n=>Math.round(n*100)/100))].sort((a,b)=>a-b),y=[...new Set(ys.map(n=>Math.round(n*100)/100))].sort((a,b)=>a-b);
      if(x.length*y.length>2400)return null;
      const key=(i,j,d)=>`${i}:${j}:${d}`, locate=(values,v)=>values.findIndex(n=>Math.abs(n-v)<.02);
      const sx=locate(x,from.x),sy=locate(y,from.y),tx=locate(x,to.x),ty=locate(y,to.y);
      if([sx,sy,tx,ty].some(n=>n<0))return null;
      const open=[{i:sx,j:sy,d:-1,g:0}],best=new Map([[key(sx,sy,-1),0]]),previous=new Map();let goal=null,steps=0;
      while(open.length&&steps++<3000){
        open.sort((a,b)=>(a.g+Math.abs(x[a.i]-to.x)+Math.abs(y[a.j]-to.y))-(b.g+Math.abs(x[b.i]-to.x)+Math.abs(y[b.j]-to.y)));
        const current=open.shift(),id=key(current.i,current.j,current.d);if(best.get(id)!==current.g)continue;
        if(current.i===tx&&current.j===ty){goal=current;break;}
        for(const [ni,nj,d] of [[current.i+1,current.j,0],[current.i-1,current.j,1],[current.i,current.j+1,2],[current.i,current.j-1,3]]){
          if(ni<0||nj<0||ni>=x.length||nj>=y.length||[1,0,3,2][current.d]===d)continue;
          const a={x:x[current.i],y:y[current.j]},b={x:x[ni],y:y[nj]};if(!clear(a,b,obstacles)||inside(b,obstacles.find(r=>inside(b,r))||{}))continue;
          const g=current.g+Math.abs(a.x-b.x)+Math.abs(a.y-b.y)+(current.d>=0&&d!==current.d?24:0),next=key(ni,nj,d);
          if(g<(best.get(next)??Infinity)){best.set(next,g);previous.set(next,id);open.push({i:ni,j:nj,d,g});}
        }
      }
      if(!goal)return null;
      const points=[];for(let at=key(goal.i,goal.j,goal.d);at;at=previous.get(at)){const [i,j]=at.split(':').map(Number);points.push({x:x[i],y:y[j]});}points.reverse();
      return points.filter((p,i,a)=>i===0||i===a.length-1||(p.x-a[i-1].x)*(a[i+1].y-p.y)!==(p.y-a[i-1].y)*(a[i+1].x-p.x));
    }
    function endpointNode(vertex,nodes){
      if(vertex?.kind!=='anchor')return null;
      return nodes.find(n=>vertex.name===n.id||vertex.name.startsWith(`${n.id}.`))?.id||null;
    }
    function updateLive(node,dx,dy){
      if(!app.basis||!app.snapshot?.capabilities?.graph_gestures)return;
      const diagram=app.snapshot.diagram,nodes=diagram.nodes,clearance=Math.max(5,Math.abs(worldToSvg({x:2,y:0}).x-worldToSvg({x:0,y:0}).x));
      const obstacles=nodes.map(n=>{
        const box=app.nodeRects.get(n.id);return box&&rect(box,n.id===node?dx:0,n.id===node?dy:0);
      }).filter(Boolean).map(r=>({minX:r.minX-clearance,minY:r.minY-clearance,maxX:r.maxX+clearance,maxY:r.maxY+clearance}));
      liveRoutes=[];
      for(const edge of diagram.edges){
        const points=app.edgePoints.get(edge.id),start=endpointNode(edge.vertices[0],nodes),end=endpointNode(edge.vertices.at(-1),nodes);
        if(!points||edge.route==='bezier'||!(start===node||end===node))continue;
        const from={...points[0]},to={...points.at(-1)};if(start===node){from.x+=dx;from.y+=dy;}if(end===node){to.x+=dx;to.y+=dy;}
        // Endpoint boxes may be crossed only at their attached port.
        const scoped=obstacles.filter((_,i)=>nodes[i]?.id!==start&&nodes[i]?.id!==end);
        const path=livePath(from,to,scoped);if(path)liveRoutes.push({edge:edge.id,points:path});
      }
      drawOverlay();
    }
    function accept(data){
      if(!matches(data))return;
      status=data;proposal=data.routing?.proposal||null;
      choices();paint();drawOverlay();
    }
    async function poll(){
      const own=++generation;
      try{
        const response=await fetch('/api/routing/status');const data=await response.json();
        if(!response.ok)throw new Error(data.error||'Cannot read routing status');
        if(own===generation)accept(data);
      }catch(error){if(own===generation){$('routing-status').textContent=error.message;}}
      clearTimeout(timer);
      // Keep the external-edit guard live while a proposal awaits adoption.
      if(status?.routing?.running||proposal)timer=setTimeout(poll,350);
    }
    async function request(path,body={}){
      if(requesting)return;
      requesting=true;paint();
      const own=++generation;
      try{
        const response=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json','X-Cetz-Studio-Token':app.token},
          body:JSON.stringify({session_id:app.sessionId,revision:app.snapshot.revision,...body})});
        const data=await response.json();
        if(!response.ok)throw new Error(data.error||'Routing request failed');
        if(own===generation)accept(data);
      }catch(error){notify(error.message,true);}
      finally{requesting=false;paint();poll();}
    }
    $('routing-preview').onclick=()=>request('/api/routing/propose',{edges:selectedIds(),clearance_mm:Number($('routing-clearance').value)});
    $('routing-discard').onclick=()=>request('/api/routing/discard');
    $('routing-apply').onclick=()=>{if(proposal&&matches(proposal))post('/api/routing/apply',{proposal_id:proposal.id});};
    $('routing-scope').onchange=paint;
    $('routing-clearance').onchange=()=>{
      // A displayed proposal retains its original clearance; changing controls
      // explicitly discards it rather than relabelling old geometry.
      if(proposal||status?.routing?.running)request('/api/routing/discard');
    };
    return {
      refresh(){
        if(!app.snapshot)return;
        if(identity!==signature()){
          identity=signature();generation++;clearTimeout(timer);proposal=null;status=null;checked.clear();
          if(!app.fixture)poll();
        }
        paint();
      },
      selectionChanged(){paint();},
      draw(overlay){
        if(!app.basis)return;
        for(const route of liveRoutes){
          const line=document.createElementNS('http://www.w3.org/2000/svg','polyline');
          line.setAttribute('points',route.points.map(p=>`${p.x},${p.y}`).join(' '));
          line.setAttribute('class','routing-live');line.setAttribute('data-live-edge',route.edge);
          line.setAttribute('stroke-width',String(2/app.zoom));overlay.append(line);
        }
        if(!proposal||!matches(proposal))return;
        for(const route of proposal.plan.routes){
          const line=document.createElementNS('http://www.w3.org/2000/svg','polyline');
          line.setAttribute('points',route.points.map(worldToSvg).map(p=>`${p.x},${p.y}`).join(' '));
          line.setAttribute('class','routing-proposal');line.setAttribute('data-proposed-edge',route.edge);
          line.setAttribute('stroke-width',String(2.5/app.zoom));overlay.append(line);
        }
      },
      dragMoved(node,original,current){
        cancelAnimationFrame(liveFrame);
        liveFrame=requestAnimationFrame(()=>{
          const a=worldToSvg(original),b=worldToSvg(current);updateLive(node,b.x-a.x,b.y-a.y);
        });
      },
      dragEnded(){cancelAnimationFrame(liveFrame);liveRoutes=[];drawOverlay();},
    };
  },
};
