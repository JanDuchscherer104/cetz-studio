/* Optional routing controller. The Rust snapshot owns source eligibility; this
 * module owns disposable measurements, worker lifetime, and proposed overlays. */
(() => {
  'use strict';
  window.CetzRouting={install({app,diagram,svgToWorld,worldToSvg,el,post,notify,drawOverlay}) {
    const $=id=>document.getElementById(id);
    const element=(tag,text)=>{const e=document.createElement(tag);if(text)e.textContent=text;return e;};
    const panel=element('details');panel.id='routing-panel';
    const title=element('summary','Automatic routing');panel.append(title);
    const intro=element('p','Fixed nodes · Preview first · Apply is one undo step.');intro.className='field-note';panel.append(intro);
    const scopeLabel=element('label','Edges to route'),scope=element('select');scope.id='route-scope';
    for(const [value,text] of [['selected','Selected edge'],['checked','Explicit selection'],['all','All eligible edges']]) {
      const o=element('option',text);o.value=value;scope.append(o);
    }
    scopeLabel.append(scope);panel.append(scopeLabel);
    const choices=element('div');choices.id='route-choices';choices.hidden=true;panel.append(choices);
    const clearanceLabel=element('label','Clearance · mm'),clearance=element('input');
    clearance.id='route-clearance';clearance.type='number';clearance.min='0';clearance.max='20';clearance.step='0.5';clearance.value='2';
    clearanceLabel.append(clearance);panel.append(clearanceLabel);
    const preview=element('button','Preview route'),apply=element('button','Apply route'),cancel=element('button','Discard proposal');
    preview.id='route-preview';apply.id='route-apply';cancel.id='route-cancel';
    preview.className=apply.className=cancel.className='fullwidth';panel.append(preview,apply,cancel);
    const status=element('p','Select an edge, then preview. No source is changed by routing.');
    status.id='route-status';status.setAttribute('role','status');status.className='field-note';panel.append(status);
    const limits=element('p','Up to 80 nodes / 32 edges. Routes avoid measured node boxes, not other edges or labels. Inspect the compiled result before Save.');
    limits.className='field-note';panel.append(limits);
    document.querySelector('.right .guardrail').before(panel);
    const toggle=element('button','Route edges');toggle.id='route-toggle';toggle.onclick=()=>{panel.open=!panel.open;sync();};$('render').before(toggle);
    let worker=null,timer=null,generation=0,work=null,proposal=null,lastMetrics=null,choiceKey='';
    function identity(snapshot=app.snapshot) {
      return snapshot?{revision:snapshot.revision,session_id:snapshot.session_id,filename:snapshot.filename,source:snapshot.source,svg:snapshot.svg}:null;
    }
    function matches(a,b=identity()) {
      return a&&b&&['revision','session_id','filename','source','svg'].every(k=>a[k]===b[k]);
    }
    function stop() { worker?.terminate();worker=null;clearTimeout(timer);timer=null;work=null; }
    function invalidate(message='Source or preview changed; route proposal discarded.') {
      const active=!!(work||proposal);generation++;stop();proposal=null;
      if(active)status.textContent=message;
      sync();
    }
    function sync() {
      preview.disabled=app.busy||!!work||app.fixture;
      apply.disabled=app.busy||!proposal||!matches(proposal.identity);
      cancel.disabled=!work&&!proposal;
      scope.disabled=clearance.disabled=!!work||app.busy;
      choices.hidden=scope.value!=='checked';
      const caps=app.snapshot?.routing||[];
      const key=JSON.stringify([app.snapshot?.session_id,app.snapshot?.revision,caps]);
      if(key!==choiceKey) {
        choiceKey=key;choices.replaceChildren();
        for(const cap of caps) {
          const row=element('label'),checkbox=element('input');checkbox.type='checkbox';checkbox.value=cap.edge;
          checkbox.disabled=!!cap.reason;row.title=cap.reason||'Eligible';
          row.append(checkbox,document.createTextNode(` ${cap.edge}${cap.reason?' · '+cap.reason:''}`));choices.append(row);
        }
      }
    }
    function changed(snapshot) {
      if((work&&!matches(work.identity,identity(snapshot))) || (proposal&&!matches(proposal.identity,identity(snapshot))))invalidate();
    }
    function measurements(selected) {
      if(!app.basis||!app.snapshot?.capabilities?.graph_gestures)throw new Error('Routing requires a verified single-page graph preview. Computed or unsupported figures stay source-owned.');
      const b=app.basis;
      if(Math.abs(b.x.y)>1e-7||Math.abs(b.y.x)>1e-7)throw new Error('Routing requires an axis-aligned preview calibration. Rotated/sheared figures stay manual.');
      if(app.locked.size)throw new Error('Routing is disabled because source/preview handles did not match.');
      const nodes=diagram().nodes.map(n=>{
        const r=app.nodeRects.get(n.id);if(!r)throw new Error(`Missing measured bounds for ${n.id}; source retained.`);
        const corners=[{x:r.x,y:r.y},{x:r.x+r.w,y:r.y+r.h}].map(svgToWorld);
        const x=Math.min(...corners.map(p=>p.x)),y=Math.min(...corners.map(p=>p.y));
        return {id:n.id,x,y,w:Math.abs(corners[1].x-corners[0].x),h:Math.abs(corners[1].y-corners[0].y)};
      });
      const edges=selected.map(cap=>{
        if(cap.reason)throw new Error(`${cap.edge}: ${cap.reason}`);
        const points=app.edgePoints.get(cap.edge);if(!points)throw new Error(`${cap.edge}: measured attachment ports are unavailable.`);
        return {...cap,start:{...cap.start,point:svgToWorld(points[0])},end:{...cap.end,point:svgToWorld(points.at(-1))}};
      });
      return {nodes,edges,clearance:Number(clearance.value)};
    }
    function selectedCapabilities() {
      const caps=app.snapshot?.routing||[];
      if(scope.value==='all')return caps.filter(cap=>!cap.reason);
      const ids=scope.value==='checked'?[...choices.querySelectorAll('input:checked')].map(c=>c.value):app.selection?.kind==='edge'?[app.selection.id]:[];
      return ids.map(id=>{const cap=caps.find(c=>c.edge===id);if(!cap)throw new Error(`${id}: no verified source-routing capability.`);return cap;});
    }
    async function compute() {
      if(app.busy||work)return;
      try {
        invalidate('Preparing a new proposal. The accepted draft is unchanged.');drawOverlay();
        if(!clearance.reportValidity()||!clearance.value.trim())return;
        const selected=selectedCapabilities();
        if(!selected.length)throw new Error(scope.value==='all'?'No eligible edges. Computed objects, unsupported options, and unverified layouts stay source-owned.':'Select an edge or check an explicit selection first.');
        const input=measurements(selected);
        invalidate('Computing a proposal. The accepted draft is unchanged.');drawOverlay();
        const job=++generation,captured=identity();work={job,identity:captured};
        worker=new Worker('/routing-worker.js');
        status.textContent='Computing in a worker. You can still pan, zoom, or edit; an edit cancels this proposal.';sync();
        timer=setTimeout(()=>{
          if(generation!==job)return;
          invalidate('Routing timed out; the previous draft is unchanged.');drawOverlay();
        },1500);
        worker.onerror=()=>{if(generation===job){invalidate('Routing worker failed; the previous draft is unchanged.');drawOverlay();}};
        worker.onmessage=event=>{
          if(event.data.job!==generation||!matches(captured))return;
          stop();
          if(event.data.error) { status.textContent=`${event.data.error} The previous draft is unchanged.`;sync();return; }
          proposal={identity:captured,...event.data.result};lastMetrics={elapsed_ms:proposal.elapsed_ms,expansions:proposal.expansions,checks:proposal.checks};
          const skipped=scope.value==='all'?(app.snapshot.routing.length-selected.length):0;
          status.textContent=`Proposed ${proposal.routes.length} route(s) in ${proposal.elapsed_ms.toFixed(2)} ms${skipped?`; ${skipped} unsupported edge(s) skipped`:''}. Inspect the dashed paths, then Apply. No source changed.`;
          sync();drawOverlay();
        };
        worker.postMessage({job,input});
      } catch(error) {stop();status.textContent=error.message;notify(error.message,true);sync();}
    }
    preview.onclick=compute;
    apply.onclick=async()=>{
      if(!proposal||app.busy)return;
      if(!matches(proposal.identity)){invalidate();drawOverlay();return;}
      const {identity:captured,routes}=proposal;
      const command={kind:'route_edges',routes:routes.map(({edge,points})=>({edge,points}))};
      // Use the proposal revision, NEVER the latest revision from a later edit.
      await post('/api/edit',{revision:captured.revision,session_id:captured.session_id,command});
    };
    cancel.onclick=()=>{invalidate('Proposal discarded. The accepted draft is unchanged.');drawOverlay();};
    scope.onchange=()=>{invalidate('Routing scope changed. Preview again before applying.');drawOverlay();};
    clearance.oninput=()=>{invalidate('Clearance changed. Preview again before applying.');drawOverlay();};
    choices.onchange=()=>{invalidate('Selection changed. Preview again before applying.');drawOverlay();};
    document.addEventListener('keydown',e=>{if(e.key==='Escape'){invalidate('Proposal discarded.');drawOverlay();}});
    window.addEventListener('pagehide',()=>stop());
    function draw(overlay) {
      if(!proposal||!matches(proposal.identity))return;
      for(const route of proposal.routes) {
        const points=route.preview.map(worldToSvg);
        overlay.append(el('polyline',{points:points.map(p=>`${p.x},${p.y}`).join(' '),
          'data-route-proposal':route.edge,fill:'none',stroke:'#735bd6','stroke-width':2/app.zoom,
          'stroke-dasharray':`${5/app.zoom} ${3/app.zoom}`,'pointer-events':'none'}));
      }
    }
    // Observation only, like cetzStudioDebug; commands still go through the UI.
    window.cetzStudioRoutingDebug=()=>({working:!!work,proposal:proposal?{revision:proposal.identity.revision,routes:proposal.routes}:null,lastMetrics});
    sync();return {changed,invalidate,sync,draw};
  }};
})();
