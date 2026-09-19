/* Optional route proposals. Source rules, search and adoption remain Rust-owned. */
window.CetzRouting = {
  create({app, post, notify, drawOverlay, worldToSvg}) {
    const $=id=>document.getElementById(id);
    const pane=$('routing-panel');
    if(!pane)return {refresh(){},draw(){},selectionChanged(){}};
    let status=null, proposal=null, identity='', generation=0, timer=null, requesting=false;
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
      $('routing-clearance').disabled=requesting||running;
      $('routing-preview').disabled=app.fixture||app.busy||requesting||running||!status?.available||!app.basis||selectedIds().length===0;
      $('routing-apply').disabled=app.busy||requesting||!proposal||!matches(proposal);
      $('routing-discard').disabled=requesting||(!running&&!proposal);
      $('routing-choices').hidden=$('routing-scope').value!=='checked';
      const failure=status?.error||status?.routing?.error;
      $('routing-status').textContent=failure|| (running ? (status.routing.cancelled?'Discarded; measurement worker is finishing.':'Measuring and routing… Canvas navigation stays available.') : proposal ?
        `${proposal.plan.routes.length} proposed route(s) · ${proposal.plan.search_ms.toFixed(1)} ms search. Apply to validate; Save writes later.` :
        !status?.available?'Automatic routing requires a current, single-page Fletcher preview.':'Nodes stay fixed. Routes are independent; crossings and labels are not avoided.');
      $('routing-panel').classList.toggle('has-proposal',!!proposal);
      const selected=(status?.eligibility||[]).find(e=>e.edge===app.selection?.id);
      $('routing-reason').textContent=$('routing-scope').value==='selected'&&selected&&!selected.eligible?selected.reason||'Unsupported edge':'';
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
        if(!proposal||!matches(proposal)||!app.basis)return;
        for(const route of proposal.plan.routes){
          const line=document.createElementNS('http://www.w3.org/2000/svg','polyline');
          line.setAttribute('points',route.points.map(worldToSvg).map(p=>`${p.x},${p.y}`).join(' '));
          line.setAttribute('class','routing-proposal');line.setAttribute('data-proposed-edge',route.edge);
          line.setAttribute('stroke-width',String(2.5/app.zoom));overlay.append(line);
        }
      },
    };
  },
};
