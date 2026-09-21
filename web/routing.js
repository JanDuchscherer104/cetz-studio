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
    function endpointNode(vertex,nodes){
      if(vertex?.kind!=='anchor')return null;
      return nodes.filter(n=>vertex.name===n.id||vertex.name.startsWith(`${n.id}.`)).sort((a,b)=>b.id.length-a.id.length)[0]?.id||null;
    }
    function updateLive(node,dx,dy){
      liveRoutes=[];
      if(!app.basis||!app.snapshot?.capabilities?.graph_gestures){drawOverlay();return;}
      const diagram=app.snapshot.diagram,nodes=diagram.nodes,clearance=Math.max(5,Math.abs(worldToSvg({x:2,y:0}).x-worldToSvg({x:0,y:0}).x));
      // Missing bounds do not remove an unknown obstacle from a supposedly safe hint.
      if(nodes.some(n=>!app.nodeRects.has(n.id))){drawOverlay();return;}
      const obstacles=nodes.map(n=>{
        const r=rect(app.nodeRects.get(n.id),n.id===node?dx:0,n.id===node?dy:0);
        return {id:n.id,bounds:{minX:r.minX-clearance,minY:r.minY-clearance,maxX:r.maxX+clearance,maxY:r.maxY+clearance}};
      });
      const port=(vertex,id)=>{
        const value=vertex?.name?.slice(id?.length||0).replace(/^\./,'');
        return value||undefined;
      };
      let attempts=0;
      const began=performance.now();
      for(const edge of diagram.edges){
        const points=app.edgePoints.get(edge.id),start=endpointNode(edge.vertices[0],nodes),end=endpointNode(edge.vertices.at(-1),nodes);
        if(!points||!edge.editable||edge.route==='bezier'||!(start===node||end===node))continue;
        if(attempts++>=8||performance.now()-began>24)break;
        const from={...points[0]},to={...points.at(-1)};if(start===node){from.x+=dx;from.y+=dy;}if(end===node){to.x+=dx;to.y+=dy;}
        // The upstream graph uses measured endpoint coordinates and excludes only
        // their own boxes. This is an ephemeral hint, never an adoptable route.
        const scoped=obstacles.filter(item=>item.id!==start&&item.id!==end).map(item=>item.bounds);
        const path=CetzUi.routePreview(from,to,scoped,{start:port(edge.vertices[0],start),end:port(edge.vertices.at(-1),end)});
        if(path)liveRoutes.push({edge:edge.id,points:path});
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
          cancelAnimationFrame(liveFrame);liveRoutes=[];
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
        const observed=signature();
        liveFrame=requestAnimationFrame(()=>{
          if(observed!==signature())return;
          const a=worldToSvg(original),b=worldToSvg(current);updateLive(node,b.x-a.x,b.y-a.y);
        });
      },
      dragEnded(){cancelAnimationFrame(liveFrame);liveRoutes=[];drawOverlay();},
    };
  },
};