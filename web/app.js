/* Small DOM/SVG interaction shell. Rust is authoritative for every source edit.
 * No framework, CDN, Node build, or diagram model duplicated on disk. */
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const NS = 'http://www.w3.org/2000/svg';
  const app = {snapshot:null, token:'', selection:null, list:'nodes', busy:false,
    zoom:1, pan:{x:0,y:0}, size:{w:600,h:400}, svg:null, markers:new Map(),
    nodeRects:new Map(), edgePoints:new Map(), labelRects:new Map(), basis:null,
    locked:new Set(), drag:null, space:false, first:true, page:0, mountedSvg:null,
    mountedGestures:false, mappingError:'', fixture:!!window.CETZ_STUDIO_FIXTURE,
    project:null, sessionId:null, copiedNode:null, pendingProjectAction:null, modalTrigger:null};
  const routing=window.CetzRouting?.create({app,post,notify,drawOverlay,worldToSvg});
  let toastTimer;
  const el = (tag, attrs={}) => {const n=document.createElementNS(NS,tag);for(const [k,v] of Object.entries(attrs))n.setAttribute(k,String(v));return n;};
  const html = (tag, cls, text) => {const n=document.createElement(tag);if(cls)n.className=cls;if(text!==undefined)n.textContent=text;return n;};
  const center = b => ({x:b.x+b.w/2,y:b.y+b.h/2});
  const distance = (a,b) => Math.hypot(a.x-b.x,a.y-b.y);
  const diagram = () => app.snapshot?.diagram || {nodes:[], edges:[], warnings:[]};
  const parameters = () => app.snapshot?.parameters || [];
  const graphGestures = () => app.snapshot?.capabilities?.graph_gestures ?? !!app.snapshot?.diagram;
  const structuralEdits = () => graphGestures() && !!app.snapshot?.preview_current && !app.mappingError;
  const insertionAvailable = () => structuralEdits() && (app.fixture||!!diagram().insert_primitives?.length);
  function notify(message, error=false) {
    $('status').textContent=message;
    if(error){$('toast').textContent=message;$('toast').hidden=false;clearTimeout(toastTimer);toastTimer=setTimeout(()=>$('toast').hidden=true,12000);}
  }
  function applyEnvelope(data){
    if(data.session_id!=null&&app.sessionId!=null&&data.session_id!==app.sessionId){app.selection=null;app.copiedNode=null;app.page=0;app.first=true;app.mountedSvg=null;}
    if(data.session_id!=null)app.sessionId=data.session_id;
    if(data.project!==undefined)app.project=data.project;
    if(data.snapshot)refresh(data.snapshot);else renderProject();
  }
  function capabilityBadge(file){
    if(file.status==='error')return {text:'Error',kind:'error',title:file.error||'Compilation failed'};
    if(file.status!=='ready')return {text:'Unchecked',kind:'unchecked',title:'Select Check to compile this file'};
    if(file.mode==='graph_editing'||file.capabilities?.graph_gestures)return {text:'Layout',kind:'layout',title:'Layout and recognized graph edits'};
    if(file.mode==='parameter_editing'||file.capabilities?.parameters)return {text:'Controls',kind:'controls',title:'Declared controls are editable'};
    return {text:'Preview',kind:'preview',title:'Renders successfully; source structure is read-only'};
  }
  function projectTree(files){
    const root={dirs:new Map(),files:[]};
    for(const file of files){
      const parts=file.path.split('/').filter(Boolean),name=parts.pop()||file.name;let at=root;
      for(const part of parts){if(!at.dirs.has(part))at.dirs.set(part,{dirs:new Map(),files:[]});at=at.dirs.get(part);}
      at.files.push({...file,name:file.name||name});
    }
    return root;
  }
  function renderProjectBranch(node,container,query,depth=0){
    for(const [name,child] of [...node.dirs].sort(([a],[b])=>a.localeCompare(b))){
      const holder=html('details','project-folder');holder.open=depth<1||!!query;
      holder.append(html('summary',null,name));const children=html('div','project-children');renderProjectBranch(child,children,query,depth+1);holder.append(children);
      if(children.childElementCount)container.append(holder);
    }
    for(const file of node.files.sort((a,b)=>a.name.localeCompare(b.name))){
      if(query&&!file.path.toLowerCase().includes(query))continue;
      const badge=capabilityBadge(file),button=html('button',`project-file${file.path===app.project?.active_file?' active':''}`);
      button.dataset.projectFile=file.path;button.title=`${file.path} · ${badge.title}`;
      button.append(html('span','project-file-name',file.name+(file.dirty?' •':'')),html('span',`file-badge ${badge.kind}`,badge.text));
      button.onclick=()=>{if(file.path===app.project?.active_file){notify(`${file.name} is already open.`);return;}requestProjectAction({path:'/api/project/open',body:{path:file.path},label:`Open ${file.name}`});};container.append(button);
    }
  }
  function renderProject(){
    const root=$('project-files');root.replaceChildren();if(document.activeElement!==$('project-root')&&app.project?.root)$('project-root').value=app.project.root;
    if(!app.project?.files){root.append(html('p','project-empty',app.fixture?'Project browsing is unavailable in the browser-only fixture.':'This server does not expose a project. Start Cetz Studio with a project root to browse Typst files.'));$('check-project').disabled=true;return;}
    $('check-project').disabled=app.busy||!app.project.files.some(f=>f.status==='unchecked');
    const query=$('project-filter').value.trim().toLowerCase(),tree=projectTree(app.project.files);renderProjectBranch(tree,root,query);
    if(!root.childElementCount)root.append(html('p','project-empty','No matching Typst files.'));
  }
  function snap(v,event={}) {return $('snap').checked&&!event.altKey?Math.round(v/Number($('grid-step').value))*Number($('grid-step').value):v;}
  function screenToSvg(x,y) {
    const matrix=app.svg?.getScreenCTM();
    if(!matrix)throw new Error('Preview transform is unavailable');
    return new DOMPoint(x,y).matrixTransform(matrix.inverse());
  }
  function worldToSvg(p) {
    const b=app.basis;return {x:b.o.x+b.x.x*p.x+b.y.x*p.y,y:b.o.y+b.x.y*p.x+b.y.y*p.y};
  }
  function svgToWorld(p) {
    const b=app.basis,dx=p.x-b.o.x,dy=p.y-b.o.y,det=b.x.x*b.y.y-b.x.y*b.y.x;
    return {x:(dx*b.y.y-dy*b.y.x)/det,y:(dy*b.x.x-dx*b.x.y)/det};
  }
  function pointerWorld(e){return svgToWorld(screenToSvg(e.clientX,e.clientY));}
  function paintTransform(){ $('paper').style.transform=`translate(${app.pan.x}px, ${app.pan.y}px) scale(${app.zoom})`;$('zoom').textContent=`${Math.round(app.zoom*100)}%`; }
  function fit(){const v=$('viewport');app.zoom=Math.max(.08,Math.min(2.5,(v.clientWidth-75)/app.size.w,(v.clientHeight-75)/app.size.h));app.pan={x:(v.clientWidth-app.size.w*app.zoom)/2,y:(v.clientHeight-app.size.h*app.zoom)/2-5};paintTransform();drawOverlay();}
  function zoomBy(factor,x,y){const v=$('viewport').getBoundingClientRect();x=x??v.width/2;y=y??v.height/2;const z=Math.max(.08,Math.min(6,app.zoom*factor)),r=z/app.zoom;app.pan={x:x-(x-app.pan.x)*r,y:y-(y-app.pan.y)*r};app.zoom=z;paintTransform();drawOverlay();}
  function markerBox(n,svg){
    const b=n.getBBox(),m=svg.getScreenCTM().inverse().multiply(n.getScreenCTM());
    const corners=[[b.x,b.y],[b.x+b.width,b.y],[b.x,b.y+b.height],[b.x+b.width,b.y+b.height]].map(([x,y])=>new DOMPoint(x,y).matrixTransform(m));
    const xs=corners.map(p=>p.x),ys=corners.map(p=>p.y);
    return {x:Math.min(...xs),y:Math.min(...ys),w:Math.max(...xs)-Math.min(...xs),h:Math.max(...ys)-Math.min(...ys)};
  }
  function sanitizeSvg(root){
    for(const n of [...root.querySelectorAll('*')]){
      if(['script','foreignObject','iframe','style','animate','set','animateTransform','animateMotion'].includes(n.localName)){n.remove();continue;}
      for(const a of [...n.attributes]){
        const key=a.localName.toLowerCase();
        if(key.startsWith('on'))n.removeAttributeNode(a);
        if((key==='href'||key==='src')&&!a.value.startsWith('#')&&!a.value.startsWith('data:image/'))n.removeAttributeNode(a);
      }
    }
  }
  function mountSvg(text){
    const doc=new DOMParser().parseFromString(text,'image/svg+xml');
    if(doc.querySelector('parsererror')||doc.documentElement.localName!=='svg')throw new Error('Malformed SVG preview');
    sanitizeSvg(doc.documentElement);
    const svg=document.importNode(doc.documentElement,true);
    $('figure').replaceChildren(svg);app.svg=svg;
    const vb=svg.viewBox.baseVal;
    if(!(vb.width>0&&vb.height>0))throw new Error('SVG has no valid viewBox');
    app.size={w:vb.width,h:vb.height};
    $('paper').style.width=`${vb.width}px`;$('paper').style.height=`${vb.height}px`;$('paper').style.display='block';
    $('overlay').setAttribute('viewBox',`${vb.x} ${vb.y} ${vb.width} ${vb.height}`);
    app.basis=null;app.nodeRects=new Map();app.edgePoints=new Map();app.labelRects=new Map();app.locked=new Set();
    $('empty').hidden=true;
    if(!graphGestures()){
      if(app.first){fit();app.first=false;}else paintTransform();
      return;
    }
    app.markers=new Map();
    for(const n of [...svg.querySelectorAll('[stroke]')]){
      const stroke=n.getAttribute('stroke')?.toLowerCase();
      const width=parseFloat(n.getAttribute('stroke-width')||'0');
      if(!/^#a[0-3][0-9a-f]{4}$/.test(stroke)||Math.abs(width-.00012345)>.000003)continue;
      if(app.markers.has(stroke.slice(1)))throw new Error('Duplicate preview marker');
      app.markers.set(stroke.slice(1),markerBox(n,svg));n.remove();
    }
    const o=app.markers.get('a00000'),x=app.markers.get('a00001'),y=app.markers.get('a00002');
    if(!o||!x||!y)throw new Error('Source/preview fiducials are missing. Editing is disabled; this is not an instrumented render.');
    const c=center(o),cx=center(x),cy=center(y);
    app.basis={o:c,x:{x:(cx.x-c.x)/10,y:(cx.y-c.y)/10},y:{x:(cy.x-c.x)/10,y:(cy.y-c.y)/10}};
    const b=app.basis;
    if(Math.abs(b.x.x*b.y.y-b.x.y*b.y.x)<1e-8)throw new Error('Degenerate preview calibration');
    app.nodeRects=new Map();app.edgePoints=new Map();app.labelRects=new Map();app.locked=new Set();
    diagram().nodes.forEach((n,i)=>{
      const rect=app.markers.get(`a1${i.toString(16).padStart(4,'0')}`);
      if(!rect){app.locked.add(n.id);return;}
      app.nodeRects.set(n.id,rect);
      // Trust measured geometry over an assumed wrapper convention.
      if(n.editable&&n.position&&distance(center(rect),worldToSvg(n.position))>.25)app.locked.add(n.id);
    });
    diagram().edges.forEach((e,i)=>{
      const p=e.vertices.map((_,j)=>app.markers.get(`a2${(i*256+j).toString(16).padStart(4,'0')}`)).map(b=>b?center(b):null);
      if(p.every(Boolean))app.edgePoints.set(e.id,p);
      const label=app.markers.get(`a3${i.toString(16).padStart(4,'0')}`);
      if(label)app.labelRects.set(e.id,label);
      e.vertices.forEach((v,j)=>{
        if(v.kind==='point'&&v.point.editable&&p[j]&&distance(p[j],worldToSvg(v.point))>.25)app.locked.add(`${e.id}:${j}`);
      });
    });
    if(app.first){fit();app.first=false;}else paintTransform();
    $('empty').hidden=true;
  }
  function nodeEditable(n){return n.editable&&!!app.basis&&!app.locked.has(n.id)&&!app.busy;}
  function vertexEditable(e,j){const v=e.vertices[j];return e.editable&&v?.kind==='point'&&v.point.editable&&!app.locked.has(`${e.id}:${j}`)&&!!app.edgePoints.get(e.id)&&!app.busy;}
  function select(kind,id){app.selection={kind,id};routing?.selectionChanged();renderList();renderInspector();drawOverlay();}
  function startDrag(e,kind,payload){
    if(app.busy||e.button!==0||!app.basis)return;
    e.preventDefault();e.stopPropagation();
    app.drag={kind,...payload,start:pointerWorld(e),screen:{x:e.clientX,y:e.clientY},moved:false};
    $('viewport').setPointerCapture(e.pointerId);
  }
  function drawOverlay(){
    const overlay=$('overlay');overlay.replaceChildren();if(!app.snapshot||!app.basis)return;
    const d=diagram(),z=app.zoom;
    for(const e of d.edges){
      const points=app.edgePoints.get(e.id);if(!points)continue;
      const curve=e.route==='bezier'&&[3,4].includes(points.length);
      const hit=curve?el('path',{d:`M ${points[0].x},${points[0].y} ${points.length===3?'Q':'C'} ${points.slice(1).map(p=>`${p.x},${p.y}`).join(' ')}`,class:'edge-hit','stroke-width':10/z,'data-edge':e.id}):el('polyline',{points:points.map(p=>`${p.x},${p.y}`).join(' '),class:'edge-hit','stroke-width':10/z,'data-edge':e.id});
      hit.addEventListener('pointerdown',ev=>{ev.stopPropagation();select('edge',e.id);});overlay.append(hit);
    }
    for(const n of d.nodes){
      const b=app.nodeRects.get(n.id);if(!b)continue;
      const selected=app.selection?.kind==='node'&&app.selection.id===n.id;
      const r=el('rect',{x:b.x-2,y:b.y-2,width:b.w+4,height:b.h+4,rx:3,class:`node-hit${selected?' selected':''}${nodeEditable(n)?'':' locked'}`,'data-node':n.id});
      r.addEventListener('pointerdown',ev=>{ev.stopPropagation();select('node',n.id);if(nodeEditable(n))startDrag(ev,'node',{node:n,box:b});});overlay.append(r);
    }
    routing?.draw(overlay);
    if(app.selection?.kind==='edge'){
      const e=d.edges.find(e=>e.id===app.selection.id),points=e&&app.edgePoints.get(e.id);if(!points)return;
      overlay.append(el('polyline',{points:points.map(p=>`${p.x},${p.y}`).join(' '),class:'route-guide','stroke-width':1.1/z}));
      points.forEach((p,j)=>{
        if(!vertexEditable(e,j))return;
        const h=el('circle',{cx:p.x,cy:p.y,r:5/z,class:'handle','data-waypoint':`${e.id}:${j}`});
        h.addEventListener('pointerdown',ev=>startDrag(ev,'waypoint',{edge:e,vertex:j,point:e.vertices[j].point}));overlay.append(h);
      });
      points.slice(0,-1).forEach((p,j)=>{
        if(e.route==='bezier')return;
        if(!vertexEditable(e,j)||!vertexEditable(e,j+1))return;
        const a=e.vertices[j].point,b=e.vertices[j+1].point;
        const vertical=Math.abs(a.x-b.x)<1e-6,horizontal=Math.abs(a.y-b.y)<1e-6;
        if(vertical===horizontal)return;
        const q=points[j+1],mid={x:(p.x+q.x)/2,y:(p.y+q.y)/2};
        // Offset segment grips from the route: a centered edge label otherwise
        // captures the same hit target and makes the segment impossible to drag.
        const grip={x:mid.x-(vertical?12/z:0),y:mid.y-(horizontal?12/z:0)};
        overlay.append(el('line',{x1:mid.x,y1:mid.y,x2:grip.x,y2:grip.y,class:'route-guide'}));
        const h=el('rect',{x:grip.x-3.5/z,y:grip.y-3.5/z,width:7/z,height:7/z,class:'handle','data-segment':`${e.id}:${j}`});
        h.addEventListener('pointerdown',ev=>startDrag(ev,'segment',{edge:e,segment:j,vertical,points:[p,q]}));overlay.append(h);
      });
      const label=app.labelRects.get(e.id);
      if(label&&e.editable&&e.has_label&&e.route!=='bezier'){
        const p=center(label),h=el('circle',{cx:p.x,cy:p.y,r:5/z,class:'label-handle','data-label':e.id});
        h.addEventListener('pointerdown',ev=>startDrag(ev,'label',{edge:e,box:label}));overlay.append(h);
      }
    }
  }
  function nearestSegment(p,points){
    let best=null;
    points.slice(0,-1).forEach((a,i)=>{
      const b=points[i+1],dx=b.x-a.x,dy=b.y-a.y,l=dx*dx+dy*dy;if(l<1e-9)return;
      const t=Math.max(0,Math.min(1,((p.x-a.x)*dx+(p.y-a.y)*dy)/l)),q={x:a.x+t*dx,y:a.y+t*dy},dist=distance(p,q);
      if(!best||dist<best.dist)best={segment:i,fraction:Math.round(t*10000)/10000,p:q,dist};
    });return best;
  }
  function moveDrag(event){
    const drag=app.drag;if(!drag)return;
    if(drag.kind==='pan'){
      const dx=event.clientX-drag.screen.x,dy=event.clientY-drag.screen.y;
      drag.moved ||= Math.hypot(dx,dy)>3;app.pan={x:drag.pan.x+dx,y:drag.pan.y+dy};paintTransform();return;
    }
    if(!app.basis)return;
    const p=pointerWorld(event),delta={x:p.x-drag.start.x,y:p.y-drag.start.y};
    drag.moved ||= Math.hypot(event.clientX-drag.screen.x,event.clientY-drag.screen.y)>3;
    if(!drag.moved)return;
    $('overlay').querySelector('.drag-ghost')?.remove();
    if(drag.kind==='node'||drag.kind==='waypoint'){
      const original=drag.kind==='node'?drag.node.position:drag.point;
      let x=snap(original.x+delta.x,event),y=snap(original.y+delta.y,event);
      if(event.shiftKey){if(Math.abs(delta.x)>Math.abs(delta.y))y=original.y;else x=original.x;}
      drag.command=drag.kind==='node'?{kind:'move_node',id:drag.node.id,x,y}:{kind:'move_waypoint',edge:drag.edge.id,vertex:drag.vertex,x,y};
      const here=worldToSvg({x,y}),old=worldToSvg(original);
      const b=drag.kind==='node'?drag.box:{x:old.x-5/app.zoom,y:old.y-5/app.zoom,w:10/app.zoom,h:10/app.zoom};
      $('overlay').append(el('rect',{x:b.x+here.x-old.x,y:b.y+here.y-old.y,width:b.w,height:b.h,rx:3,class:'drag-ghost'}));
      notify(`${drag.kind==='node'?drag.node.id:'Waypoint'} → x ${x.toFixed(2)} mm, y ${y.toFixed(2)} mm · release to compile`);
    }else if(drag.kind==='segment'){
      const d=snap(drag.vertical?delta.x:delta.y,event);
      drag.command={kind:'move_segment',edge:drag.edge.id,segment:drag.segment,delta:d};
      const zero=worldToSvg({x:0,y:0}),q=worldToSvg({x:drag.vertical?d:0,y:drag.vertical?0:d});
      $('overlay').append(el('polyline',{points:drag.points.map(p=>`${p.x+q.x-zero.x},${p.y+q.y-zero.y}`).join(' '),class:'drag-ghost',fill:'none','stroke-width':3/app.zoom}));
      notify(`Orthogonal segment offset ${d.toFixed(2)} mm · release to compile`);
    }else if(drag.kind==='label'){
      const nearest=nearestSegment(screenToSvg(event.clientX,event.clientY),app.edgePoints.get(drag.edge.id));if(!nearest)return;
      drag.command={kind:'set_label',edge:drag.edge.id,segment:nearest.segment,fraction:nearest.fraction};
      $('overlay').append(el('circle',{cx:nearest.p.x,cy:nearest.p.y,r:7/app.zoom,class:'drag-ghost'}));
      notify(`Label → segment ${nearest.segment}, fraction ${nearest.fraction.toFixed(2)} · release to compile`);
    }
  }
  async function endDrag(){const drag=app.drag;if(!drag)return;app.drag=null;$('viewport').classList.remove('panning');drawOverlay();if(drag.moved&&drag.command)await post('/api/edit',{command:drag.command});else if(drag.kind==='pan'&&!drag.moved){app.selection=null;renderList();renderInspector();drawOverlay();}}
  function renderList(){
    if(!app.snapshot)return;const d=diagram(),root=$('elements'),query=$('search').value.toLowerCase();root.replaceChildren();
    const items=app.list==='parameters'?parameters():app.list==='nodes'?d.nodes:d.edges;
    $('node-count').textContent=d.nodes.length;$('edge-count').textContent=d.edges.length;$('parameter-count').textContent=parameters().length;$('element-count').textContent=d.nodes.length+d.edges.length+parameters().length;
    if(!items.length)root.append(html('p','inspector-hint',app.list==='parameters'?'No declared controls in this source. Add studio.param declarations to expose layout or style values.':'No recognized '+app.list+'. The Typst preview remains available; use Controls for declared parameters.'));
    for(const item of items){
      const name=app.list==='parameters'?(item.label||item.id):app.list==='nodes'?item.id:edgeName(item);if(!`${name} ${item.id} ${item.title||''}`.toLowerCase().includes(query))continue;
      const kind=app.list==='parameters'?'parameter':app.list==='nodes'?'node':'edge',button=html('button',`element${app.selection?.kind===kind&&app.selection.id===item.id?' active':''}`);
      button.dataset.element=item.id;button.title=item.title||name;button.append(html('span','kind-icon',kind==='parameter'?'⚙':kind==='node'?'▭':'↗'),html('span','element-name',name),html('span','line',String(item.line||'')));
      button.addEventListener('click',()=>select(kind,item.id));root.append(button);
    }
  }
  function edgeName(e){const name=v=>v?.kind==='anchor'?v.name:'point';return `${name(e.vertices[0])} → ${name(e.vertices.at(-1))}`;}
  function inputNumber(id,value,label){const wrapper=html('label',null,label),input=html('input');input.id=id;input.type='number';input.step='0.5';input.value=Number(value.toFixed(4));wrapper.append(input);return wrapper;}
  function renderParameter(root, item){
    root.append(html('div','selection-type','DECLARED CONTROL'),html('h2','selection-name',item.label||item.id),html('div','source-line',`Source line ${item.line} · ${item.id}`));
    const label=html('label','field-label',item.unit?`Value · ${item.unit}`:'Value'),input=html('input');
    input.id='parameter-value';label.htmlFor=input.id;
    if(item.kind==='bool'){input.type='checkbox';input.checked=item.value;}
    else if(item.kind==='color'){input.type='text';input.value=item.value;input.pattern='#[0-9a-fA-F]{6}';input.placeholder='#d8eadd';}
    else{input.type='number';input.value=item.value;input.step=item.step??'any';if(item.min!=null)input.min=item.min;if(item.max!=null)input.max=item.max;}
    input.disabled=app.busy||!app.snapshot.preview_current;
    const apply=html('button','fullwidth','Apply control');apply.id='apply-parameter';apply.disabled=input.disabled;
    apply.onclick=()=>{
      if(!input.reportValidity())return;
      const value=item.kind==='bool'?input.checked:item.kind==='color'?input.value:Number(input.value);
      if(input.type==='number'&&(!input.value.trim()||!Number.isFinite(value)))return;
      post('/api/edit',{command:{kind:'set_parameter',id:item.id,value}});
    };
    root.append(label,input,apply,html('p','field-note','Only this declaration’s literal changes. Typst recomputes every drawing that uses it; equations and generated source are preserved.'));
  }
  function renderTextFields(root,item,kind){
    const fields=item.text_fields||[];if(!fields.length)return;
    root.append(html('label','field-label',kind==='node'?'Text content':'Edge text'));
    for(const field of fields){
      const wrap=html('div','text-field'),label=html('label',null,field.id[0].toUpperCase()+field.id.slice(1)),input=html('textarea');
      const source=html('textarea','typst-editor'),applySource=html('button','fullwidth','Apply Typst');
      source.id=`${kind}-${field.id}-source`;source.value=field.source??'';source.spellcheck=false;
      source.setAttribute('aria-label',`${label.textContent} Typst content`);
      source.disabled=app.busy||!field.source_editable||!structuralEdits();
      applySource.id=`apply-${kind}-${field.id}-source`;applySource.disabled=source.disabled;
      applySource.onclick=async()=>{
        const draft=source.value;
        const command=kind==='node'?{kind:'set_node_source',id:item.id,field:field.id,source:draft}:{kind:'set_edge_source',edge:item.id,field:field.id,source:draft};
        const result=await post('/api/edit',{command});
        if(!result){const retained=$(source.id);if(retained){retained.value=draft;retained.focus();}}
      };
      label.htmlFor=source.id;wrap.append(label,source,applySource);
      wrap.append(html('p','field-note','Edit this content expression, including brackets, maths, or composed Typst. Apply compiles the draft; Save writes the file.'));
      const plain=html('details','plain-text-control');plain.id=`plain-${kind}-${field.id}`;
      plain.append(html('summary',null,'Plain-text shortcut'));
      input.id=`${kind}-${field.id}-text`;input.value=field.value??'';input.disabled=app.busy||!field.editable||!structuralEdits();
      input.setAttribute('aria-label',`${label.textContent} plain text`);
      const apply=html('button','fullwidth','Apply text');apply.id=`apply-${kind}-${field.id}`;apply.disabled=input.disabled;
      apply.onclick=()=>post('/api/edit',{command:kind==='node'?{kind:'set_node_text',id:item.id,field:field.id,text:input.value}:{kind:'set_edge_text',edge:item.id,field:field.id,text:input.value}});
      plain.append(input,apply);
      const reason=field.reason||(!structuralEdits()?'Editing needs a verified graph preview. This source remains viewable.':null);if(reason)plain.append(html('p','read-only-reason',reason));
      wrap.append(plain);
      root.append(wrap);
    }
  }
  function renderInspector(){
    const root=$('inspector');root.replaceChildren();const sel=app.selection,d=diagram();
    if(sel?.kind==='parameter'){
      const item=parameters().find(p=>p.id===sel.id);
      if(item){renderParameter(root,item);return;}
      app.selection=null;
    }
    if(!app.selection){root.append(html('div','inspector-hint',graphGestures()?'Select a node or connection to edit supported layout properties. Controls exposes declared layout and style parameters.':'This figure is rendered by Typst. Select a declared control to edit its value; undeclared or computed properties remain source-owned.'));return;}
    const item=(sel.kind==='node'?d.nodes:d.edges).find(n=>n.id===sel.id);if(!item){app.selection=null;renderInspector();return;}
    root.append(html('div','selection-type',sel.kind==='node'?'NODE':'CONNECTION'),html('h2','selection-name',sel.kind==='node'?item.id:item.id.toUpperCase()),html('div','source-line',`Source line ${item.line} · ${sel.kind==='node'?item.kind:'Fletcher edge'}`));
    if(sel.kind==='node'){
      renderTextFields(root,item,'node');
      if(item.position){
        root.append(html('label','field-label','Position · millimetres'));
        const row=html('div','coordinate-row');row.append(inputNumber('node-x',item.position.x,'X'),inputNumber('node-y',item.position.y,'Y'));root.append(row);
        for(const id of ['node-x','node-y'])$(id).disabled=!nodeEditable(item);
        const apply=html('button','fullwidth','Apply position');apply.disabled=!nodeEditable(item);
        apply.addEventListener('click',()=>{const x=Number($('node-x').value),y=Number($('node-y').value);if(Number.isFinite(x)&&Number.isFinite(y))post('/api/edit',{command:{kind:'move_node',id:item.id,x,y}});});root.append(apply);
      }
      root.append(html('p','field-note',nodeEditable(item)?'Y increases downwards. Shift-drag locks one axis. Alt temporarily disables snapping.':(app.locked.has(item.id)?'Measured position does not match the configured wrapper. Editing is locked; check --y-scale or the n() convention.':'This node uses unsupported or elastic coordinates and is read-only.')));
      if(!item.text_fields?.length&&item.title)root.append(html('label','field-label','Source content'),html('p','inspector-hint',item.title));
      const actions=html('div','inspector-actions'),copy=html('button',null,'Copy node'),paste=html('button',null,'Paste copy'),duplicate=html('button',null,'Duplicate');
      copy.id='copy-node';paste.id='paste-node';duplicate.id='duplicate-node';copy.disabled=!structuralEdits()||app.busy;duplicate.disabled=copy.disabled;paste.disabled=copy.disabled||!app.copiedNode||!d.nodes.some(n=>n.id===app.copiedNode);
      copy.onclick=()=>{app.copiedNode=item.id;notify(`${item.id} copied. Paste duplicates it in this diagram.`);renderInspector();};
      paste.onclick=()=>duplicateNode(app.copiedNode);duplicate.onclick=()=>duplicateNode(item.id);actions.append(copy,paste,duplicate);root.append(actions);
      if(app.copiedNode===item.id)root.append(html('p','field-note','Copied internally · Ctrl/Cmd+V duplicates this node.'));
      const remove=html('button','fullwidth','Delete node');remove.id='delete-node';
      remove.disabled=app.busy||!structuralEdits()||!item.deletable;
      remove.onclick=()=>{
        const count=item.attached_edges||0;
        if(count&&!window.confirm(`Delete ${item.id} and its ${count} attached connection${count===1?'':'s'}? You can undo this change.`))return;
        post('/api/edit',{command:{kind:'delete_node',id:item.id,cascade:count>0}});
      };
      root.append(remove);if(item.delete_reason)root.append(html('p','read-only-reason',item.delete_reason));
    }else{
      renderTextFields(root,item,'edge');
      renderManualRoute(root,item);
      root.append(html('p','inspector-hint',edgeName(item)),html('label','field-label','Attachment ports'));
      for(const end of ['start','end']){
        const v=end==='start'?item.vertices[0]:item.vertices.at(-1),row=html('div','property-row'),selectEl=html('select');
        for(const p of ['auto','north','south','east','west','north-east','north-west','south-east','south-west']){const o=html('option',null,p);o.value=p;selectEl.append(o);}
        const resolved=v?.kind==='anchor'?d.nodes.filter(n=>v.name===n.id||v.name.startsWith(`${n.id}.`)).sort((a,b)=>b.id.length-a.id.length)[0]:null;
        const port=resolved?v.name.slice(resolved.id.length).replace(/^\./,''):'auto';selectEl.value=port||'auto';selectEl.disabled=!resolved||!item.editable||!structuralEdits()||app.busy;
        selectEl.addEventListener('change',()=>post('/api/edit',{command:{kind:'set_port',edge:item.id,end,port:selectEl.value}}));row.append(html('span',null,end==='start'?'From':'To'),selectEl);root.append(row);
      }
      root.append(html('p','field-note','Green circles move literal waypoints. Small squares move orthogonal segments whose ends are both literal points. Amber moves the existing label.'));
      if(item.has_label){
        root.append(html('label','field-label','Label placement'));
        const pos=item.label_position||[0,.5],row=html('div','coordinate-row');row.append(inputNumber('label-segment',pos[0],'Segment (0-based)'),inputNumber('label-fraction',pos[1],'Fraction'));root.append(row);$('label-segment').step='1';$('label-fraction').step='.05';if(item.route==='bezier'){$('label-segment').value=0;$('label-segment').disabled=true;}
        const button=html('button','fullwidth','Apply label position');button.disabled=!item.editable||!structuralEdits()||app.busy;button.addEventListener('click',()=>post('/api/edit',{command:{kind:'set_label',edge:item.id,segment:Number($('label-segment').value),fraction:Number($('label-fraction').value)}}));root.append(button);
      }
      const remove=html('button','fullwidth','Delete connection');remove.id='delete-edge';remove.disabled=app.busy||!structuralEdits();
      remove.onclick=()=>post('/api/edit',{command:{kind:'delete_edge',edge:item.id}});root.append(remove);
    }
  }
  function renderManualRoute(root,edge){
    if(edge.route===undefined)return;
    root.append(html('label','field-label','Path mode'));
    const mode=html('select');mode.id='edge-route';
    for(const [value,label] of [['polyline','Corners / polyline'],['bezier','Bézier controls']]){const option=html('option',null,label);option.value=value;mode.append(option);}
    mode.value=edge.route;mode.disabled=app.busy||!structuralEdits()||!edge.route_editable;
    mode.onchange=()=>post('/api/edit',{command:{kind:'set_edge_route',edge:edge.id,route:mode.value}});root.append(mode);
    if(edge.route_reason)root.append(html('p','read-only-reason',edge.route_reason));
    const bezier=edge.route==='bezier';root.append(html('label','field-label',bezier?'Control points':'Corners'));
    edge.vertices.forEach((vertex,index)=>{
      if(index===0||index===edge.vertices.length-1||vertex.kind!=='point')return;
      const row=html('div','property-row'),remove=html('button',null,'Remove');remove.dataset.removeWaypoint=String(index);
      remove.disabled=app.busy||!structuralEdits()||!edge.waypoint_editable||!vertex.point.editable||(bezier&&edge.vertices.length<=3);
      remove.onclick=()=>post('/api/edit',{command:{kind:'remove_waypoint',edge:edge.id,vertex:index}});
      row.append(html('span',null,`${bezier?'Control':'Corner'} ${index} · ${vertex.point.x.toFixed(1)}, ${vertex.point.y.toFixed(1)} mm`),remove);root.append(row);
    });
    const segment=html('select');segment.id='insert-segment';
    for(let i=0;i<edge.vertices.length-1;i++){const option=html('option',null,`Insert on segment ${i}`);option.value=String(i);segment.append(option);}
    const add=html('button','fullwidth',bezier?'Add control point':'Add corner');add.id='insert-waypoint';
    add.disabled=app.busy||!structuralEdits()||!edge.waypoint_editable||(bezier&&edge.vertices.length>=4);segment.disabled=add.disabled;
    add.onclick=()=>{const i=Number(segment.value),points=app.edgePoints.get(edge.id);if(!points?.[i+1])return;const a=svgToWorld(points[i]),b=svgToWorld(points[i+1]);post('/api/edit',{command:{kind:'insert_waypoint',edge:edge.id,segment:i,x:(a.x+b.x)/2,y:(a.y+b.y)/2}});};
    root.append(segment,add,html('p','field-note',bezier?'Drag the control handles to shape the curve. Use label fraction below for placement.':'Add a corner at a segment midpoint, then drag its handle.'));
    if(edge.waypoint_reason)root.append(html('p','read-only-reason',edge.waypoint_reason));
  }
  function showModal(id){
    if($('modal-backdrop').hidden)app.modalTrigger=document.activeElement;
    $('modal-backdrop').hidden=false;
    for(const modal of $('modal-backdrop').querySelectorAll('.modal'))modal.hidden=modal.id!==id;
    $('modal-backdrop').querySelector(`#${id} button:not(:disabled),#${id} input:not(:disabled),#${id} select:not(:disabled)`)?.focus();
  }
  function closeModal(){
    if(!$('dirty-modal').hidden)app.pendingProjectAction=null;
    $('modal-backdrop').hidden=true;for(const modal of $('modal-backdrop').querySelectorAll('.modal'))modal.hidden=true;
    const trigger=app.modalTrigger;app.modalTrigger=null;if(trigger?.isConnected)trigger.focus();
  }
  async function duplicateNode(id){
    const before=new Set(diagram().nodes.map(n=>n.id)),data=await post('/api/edit',{command:{kind:'duplicate_node',id}});
    const created=data?.snapshot?.diagram?.nodes?.find(n=>!before.has(n.id));if(created)select('node',created.id);
  }
  function viewCenterWorld(){
    if(!app.basis||!app.svg)return {x:50,y:50};
    const box=$('viewport').getBoundingClientRect();
    try{return pointerWorld({clientX:box.left+box.width/2,clientY:box.top+box.height/2});}catch{return {x:50,y:50};}
  }
  function openGallery(){
    const supported=new Set(diagram().insert_primitives||(app.fixture?primitives.map(([id])=>id):[]));
    for(const button of $('primitive-gallery').querySelectorAll('[data-primitive]')){button.disabled=!supported.has(button.dataset.primitive);button.title=button.disabled?(diagram().insertion_reason||'This primitive is unavailable for the current source.'):'';}
    if(!structuralEdits()||!supported.size){notify(diagram().insertion_reason||'Node insertion needs a recognized, current Fletcher diagram.',true);return;}
    showModal('gallery-modal');
  }
  function openEdgeCreator(){
    const nodes=diagram().nodes;if(!structuralEdits()||nodes.length<2){notify('Adding an edge needs at least two recognized named nodes.',true);return;}
    for(const id of ['edge-from','edge-to']){$(id).replaceChildren();nodes.forEach(n=>{const o=html('option',null,n.id);o.value=n.id;$(id).append(o);});}
    if(app.selection?.kind==='node')$('edge-from').value=app.selection.id;
    $('edge-to').selectedIndex=Math.min(1,nodes.length-1);$('new-edge-label').value='';showModal('edge-modal');
  }
  async function requestProjectAction(action){
    if(app.snapshot?.dirty&&!action.body.discard){app.pendingProjectAction=action;showModal('dirty-modal');return;}
    closeModal();await post(action.path,action.body,{message:action.label,projectAction:true});
  }
  async function checkVisibleFiles(){
    if(!app.project)return;
    const query=$('project-filter').value.trim().toLowerCase(),files=app.project.files.filter(f=>f.status==='unchecked'&&(!query||f.path.toLowerCase().includes(query)));
    let completed=0;for(const file of files){const data=await post('/api/project/check',{path:file.path},{message:`Checked ${file.name}`,quiet:true});if(!data)break;completed++;}
    if(!files.length)notify('No unchecked files in this view.');else if(completed===files.length)notify(`Compatibility checked for ${completed} file${completed===1?'':'s'}.`);
  }
  function refresh(snapshot){
    app.snapshot=snapshot;$('filename').textContent=snapshot.filename;$('dirty').textContent=snapshot.dirty?'Unsaved layout':'Source unchanged';$('dirty').className=`pill${snapshot.dirty?' modified':''}`;
    $('revision').textContent=`Revision ${snapshot.revision}`;
    const pages=snapshot.pages?.length?snapshot.pages:snapshot.svg?[snapshot.svg]:[];
    app.page=Math.min(app.page,Math.max(0,pages.length-1));
    const picker=$('page-select');picker.hidden=pages.length<2;picker.replaceChildren();
    pages.forEach((_,i)=>{const option=html('option',null,`Page ${i+1} / ${pages.length}`);option.value=i;picker.append(option);});picker.value=app.page;
    const svg=pages[app.page],gestures=graphGestures();
    if(svg){
      if(svg!==app.mountedSvg||gestures!==app.mountedGestures){
        app.mappingError='';
        try{mountSvg(svg);}catch(e){app.mappingError=e.message;notify(e.message,true);app.basis=null;}
        app.mountedSvg=svg;app.mountedGestures=gestures;
      }
    }else{app.mountedSvg=null;app.basis=null;$('paper').style.display='none';$('empty').hidden=false;}
    $('preview-status').textContent=app.fixture?'UI fixture':(svg?(app.basis?'Editable Typst preview':parameters().length?'Typst preview · controls':'Typst preview · view only'):'Preview unavailable');
    $('canvas-hint').textContent=app.basis?'Drag a node · Select an edge for routes · Space + drag to pan':'Use Controls for declared parameters · Scroll to zoom · Drag to pan';
    $('undo').disabled=app.busy||!snapshot.undo;$('redo').disabled=app.busy||!snapshot.redo;
    $('save').disabled=app.fixture||app.busy||!snapshot.dirty||!snapshot.preview_current||!!app.mappingError;
    $('render').disabled=app.busy;$('download').disabled=!snapshot.source;
    const warnings=[...(snapshot.warnings||diagram().warnings)];if(app.mappingError)warnings.push(app.mappingError);if(app.locked.size)warnings.push(`${app.locked.size} handle(s) locked after source/preview coordinate checks.`);
    $('warning-count').textContent=warnings.length+(snapshot.diagnostics?1:0);$('diagnostics').textContent=[...warnings,snapshot.diagnostics].filter(Boolean).join('\n\n')||'No compiler diagnostics.';
    $('diff').replaceChildren();const lines=snapshot.diff?snapshot.diff.split('\n'):['No changes. The original source is untouched.'];
    for(const line of lines){const cls=line.startsWith('@@')?'hunk':line.startsWith('+')&&!line.startsWith('+++')?'add':line.startsWith('-')&&!line.startsWith('---')?'remove':null;$('diff').append(html('span',cls,`${line}\n`));}
    $('diff-count').textContent=lines.filter(l=>/^[+-](?![+-])/.test(l)).length;renderProject();renderList();renderInspector();routing?.refresh();drawOverlay();
    $('add-node').disabled=app.busy||!insertionAvailable();$('add-node').title=diagram().insertion_reason||'Insert a supported primitive';$('add-edge').disabled=app.busy||!structuralEdits()||diagram().nodes.length<2;
  }
  async function post(path,body={},options={}){
    if(app.busy)return;
    app.busy=true;$('busy-overlay').hidden=false;refresh(app.snapshot);
    try{
      const response=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json','X-Cetz-Studio-Token':app.token},body:JSON.stringify({session_id:app.sessionId,revision:app.snapshot.revision,...body})});
      const data=await response.json();if(!response.ok)throw Object.assign(new Error(data.error||'Request failed'),{envelope:data});
      app.busy=false;applyEnvelope(data);
      if(!options.quiet)notify(options.message||(path==='/api/save'?(data.backup?`Saved source. Backup: ${data.backup}`:'No source changes to save.'):(app.fixture?'UI fixture updated. This did not execute Rust or Typst.':'Source edit compiled. Original source stays unchanged until Save.')));
      return data;
    }catch(e){app.busy=false;if(e.envelope)applyEnvelope(e.envelope);else refresh(app.snapshot);notify(e.message,true);return null;}
    finally{$('busy-overlay').hidden=true;}
  }
  $('viewport').addEventListener('pointerdown',e=>{if(!app.busy&&((app.space&&e.button===0)||e.button===1)){e.preventDefault();e.stopPropagation();app.drag={kind:'pan',screen:{x:e.clientX,y:e.clientY},pan:{...app.pan},moved:false};$('viewport').setPointerCapture(e.pointerId);$('viewport').classList.add('panning');}},{capture:true});
  $('viewport').addEventListener('pointerdown',e=>{if(app.busy||e.target.closest?.('.handle,.label-handle,.node-hit,.edge-hit'))return;if(e.button===0||e.button===1){e.preventDefault();app.drag={kind:'pan',screen:{x:e.clientX,y:e.clientY},pan:{...app.pan},moved:false};$('viewport').setPointerCapture(e.pointerId);$('viewport').classList.add('panning');}});
  $('viewport').addEventListener('pointermove',moveDrag);$('viewport').addEventListener('pointerup',endDrag);$('viewport').addEventListener('pointercancel',()=>{app.drag=null;drawOverlay();});
  $('viewport').addEventListener('wheel',e=>{e.preventDefault();const b=$('viewport').getBoundingClientRect();zoomBy(Math.exp(-e.deltaY*.001),e.clientX-b.left,e.clientY-b.top);},{passive:false});
  $('fit').onclick=fit;$('zoom-in').onclick=()=>zoomBy(1.2);$('zoom-out').onclick=()=>zoomBy(1/1.2);
  $('undo').onclick=()=>post('/api/undo');$('redo').onclick=()=>post('/api/redo');$('render').onclick=()=>post('/api/render');$('save').onclick=()=>post('/api/save');
  $('add-node').onclick=openGallery;$('add-edge').onclick=openEdgeCreator;
  const primitives=[
    ['fletcher-rect','Fletcher rectangle','A standard rectangular Fletcher node.','rect'],
    ['fletcher-ellipse','Fletcher ellipse','A rounded elliptical Fletcher node.','ellipse'],
    ['fletcher-diamond','Fletcher diamond','A decision-shaped Fletcher node.','diamond'],
    ['studio-node','Studio node','A themed general-purpose Studio node.','studio'],
    ['studio-card','Studio card','A themed title and body card.','card']
  ];
  for(const [id,name,description,shape] of primitives){
    const button=html('button','primitive-card');button.dataset.primitive=id;
    const preview=html('span',`primitive-preview ${shape}`);preview.setAttribute('aria-hidden','true');button.append(preview,html('strong',null,name),html('span',null,description));
    button.onclick=async()=>{const before=new Set(diagram().nodes.map(n=>n.id)),p=viewCenterWorld();closeModal();const data=await post('/api/edit',{command:{kind:'insert_node',primitive:id,x:snap(p.x),y:snap(p.y)}});const created=data?.snapshot?.diagram?.nodes?.find(n=>!before.has(n.id));if(created)select('node',created.id);};
    $('primitive-gallery').append(button);
  }
  for(const button of document.querySelectorAll('.modal-close'))button.onclick=closeModal;
  $('modal-backdrop').addEventListener('pointerdown',e=>{if(e.target===$('modal-backdrop'))closeModal();});
  $('create-edge').onclick=async()=>{const from=$('edge-from').value,to=$('edge-to').value;if(from===to){notify('Choose two different nodes.',true);return;}closeModal();await post('/api/edit',{command:{kind:'add_edge',from,to,label:$('new-edge-label').value.trim()||null,arrow:$('edge-arrow').value}});};
  $('dirty-cancel').onclick=()=>{app.pendingProjectAction=null;closeModal();};
  $('dirty-discard').onclick=()=>{const action=app.pendingProjectAction;app.pendingProjectAction=null;if(action)requestProjectAction({...action,body:{...action.body,discard:true}});};
  $('dirty-save').onclick=async()=>{
    const action=app.pendingProjectAction,buttons=['dirty-save','dirty-discard','dirty-cancel'].map($);buttons.forEach(button=>button.disabled=true);
    const saved=await post('/api/save');buttons.forEach(button=>button.disabled=false);
    if(saved&&action&&app.pendingProjectAction===action){app.pendingProjectAction=null;await requestProjectAction(action);}
    else if(!saved&&app.pendingProjectAction===action)showModal('dirty-modal');
  };
  $('project-filter').addEventListener('input',renderProject);$('check-project').onclick=checkVisibleFiles;
  $('refresh-project').onclick=()=>{if(app.fixture)renderProject();else post('/api/project/refresh',{}, {message:'Project files refreshed.'});};
  $('open-project').onclick=()=>{const path=$('project-root').value.trim();if(path)requestProjectAction({path:'/api/project/root',body:{path},label:'Project opened'});};
  $('project-root').addEventListener('keydown',e=>{if(e.key==='Enter')$('open-project').click();});
  $('search').addEventListener('input',renderList);
  function showList(kind){app.list=kind;for(const tab of ['nodes','edges','parameters'])$(`${tab}-tab`).classList.toggle('active',kind===tab);renderList();}
  for(const kind of ['nodes','edges','parameters'])$(`${kind}-tab`).onclick=()=>showList(kind);
  $('page-select').onchange=()=>{app.page=Number($('page-select').value);app.first=true;refresh(app.snapshot);};
  for(const tab of ['diff','diagnostics'])$(`${tab}-tab`).onclick=()=>{$('diff').hidden=tab!=='diff';$('diagnostics').hidden=tab!=='diagnostics';$('diff-tab').classList.toggle('active',tab==='diff');$('diagnostics-tab').classList.toggle('active',tab==='diagnostics');};
  $('download').onclick=()=>{if(!app.snapshot)return;const u=URL.createObjectURL(new Blob([app.snapshot.source],{type:'text/plain;charset=utf-8'})),a=document.createElement('a');a.href=u;a.download=app.snapshot.filename.replace(/\.typ$/,'.draft.typ');a.click();setTimeout(()=>URL.revokeObjectURL(u),1000);};
  document.addEventListener('keydown',e=>{
    if(e.key==='Tab'&&!$('modal-backdrop').hidden){
      const modal=[...$('modal-backdrop').querySelectorAll('.modal')].find(item=>!item.hidden),focusable=[...modal.querySelectorAll('button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled)')];
      if(focusable.length){const first=focusable[0],last=focusable.at(-1);if(!modal.contains(document.activeElement)){e.preventDefault();(e.shiftKey?last:first).focus();}else if(e.shiftKey&&document.activeElement===first){e.preventDefault();last.focus();}else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first.focus();}}return;
    }
    if(e.key==='Escape'){if(!$('modal-backdrop').hidden){app.pendingProjectAction=null;closeModal();return;}$('toast').hidden=true;app.drag=null;app.selection=null;renderInspector();drawOverlay();return;}
    if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='s'){e.preventDefault();if(!$('save').disabled)post('/api/save');return;}
    if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='z'){e.preventDefault();const redo=e.shiftKey;if(!$(redo?'redo':'undo').disabled)post(redo?'/api/redo':'/api/undo');return;}
    if(['INPUT','TEXTAREA','SELECT'].includes(document.activeElement.tagName))return;
    if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='c'&&app.selection?.kind==='node'){e.preventDefault();app.copiedNode=app.selection.id;notify(`${app.copiedNode} copied. Paste duplicates it in this diagram.`);renderInspector();return;}
    if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='v'&&app.copiedNode){e.preventDefault();if(diagram().nodes.some(n=>n.id===app.copiedNode))duplicateNode(app.copiedNode);else{app.copiedNode=null;notify('The copied node is not in this diagram.',true);}return;}
    if(e.code==='Space'){e.preventDefault();app.space=true;}
    const step=e.shiftKey?5:.5,delta={ArrowLeft:[-step,0],ArrowRight:[step,0],ArrowUp:[0,-step],ArrowDown:[0,step]}[e.key];
    if(delta&&app.selection?.kind==='node'&&!app.busy){const n=diagram().nodes.find(n=>n.id===app.selection.id);if(n&&nodeEditable(n)){e.preventDefault();post('/api/edit',{command:{kind:'move_node',id:n.id,x:n.position.x+delta[0],y:n.position.y+delta[1]}});}}
  });
  document.addEventListener('keyup',e=>{if(e.code==='Space')app.space=false;});
  window.addEventListener('beforeunload',e=>{if(app.snapshot?.dirty&&!app.fixture){e.preventDefault();e.returnValue='';}});
  new ResizeObserver(()=>{if(app.svg&&!app.drag)fit();}).observe($('viewport'));
  if(app.fixture){$('fixture-banner').hidden=false;$('session-mode').textContent='BROWSER-TESTED UI FIXTURE';}
  fetch('/api/state').then(r=>{if(!r.ok)throw new Error('Cannot open session');return r.json();}).then(data=>{app.token=data.token;applyEnvelope(data);if(!graphGestures()&&parameters().length)showList('parameters');notify(app.fixture?'Interactive fixture loaded. Native rendering is not exercised in this fixture.':'Source opened. Only explicit Save writes to disk.');}).catch(e=>notify(e.message,true));
  // Read-only observation seam for the browser smoke test; never accepts edits.
  window.cetzStudioDebug=()=>({revision:app.snapshot?.revision,basis:app.basis,locked:[...app.locked],selection:app.selection,busy:app.busy});
})();
