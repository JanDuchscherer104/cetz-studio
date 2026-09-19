"""Real browser/Rust/Typst routing proof. Uses disposable copies, never edits examples.

Run with --binary /path/to/cetz-studio [--chromium /path/to/chromium].
Screenshots and latency observations are emitted to --output-dir.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import tempfile
import threading
import time
from typing import Any

from playwright.sync_api import sync_playwright
from native_browser import running_app, open_page, browser_snapshot, wait_revision, wait_condition

ROOT = Path(__file__).resolve().parents[1]


def api(page, path: str, body: dict[str, Any] | None = None) -> dict[str, Any]:
    return page.evaluate("""async ({path,body})=>{
        const state=await (await fetch('/api/state')).json();
        const opts=body===null?{}:{method:'POST',headers:{'Content-Type':'application/json','X-Cetz-Studio-Token':state.token},
          body:JSON.stringify({session_id:state.session_id,revision:state.snapshot.revision,...body})};
        const response=await fetch(path,opts);return {status:response.status,data:await response.json()};
    }""", {"path": path, "body": body})


def routing_ready(page):
    deadline=time.monotonic()+30
    while time.monotonic()<deadline:
        data=api(page,"/api/routing/status")["data"]
        if not data["routing"]["running"]:
            return data
        threading.Event().wait(.05)
    raise AssertionError("routing did not finish within the native fixture budget")


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary",required=True,type=Path)
    parser.add_argument("--chromium",type=Path)
    parser.add_argument("--output-dir",type=Path,default=ROOT/".ci"/"routing")
    args=parser.parse_args()
    out=args.output_dir.resolve();out.mkdir(parents=True,exist_ok=True)
    binary=args.binary.resolve();real_typst=shutil.which("typst")
    assert real_typst, "Typst is required"
    checks=[];timings={};errors=[]
    def check(ok,name):
        assert ok,name
        checks.append(name)
    with tempfile.TemporaryDirectory(prefix="cetz-routing-browser-") as temp, sync_playwright() as pw:
        root=Path(temp);source=root/"routing.typ";original=(ROOT/"examples/routing.typ").read_text();source.write_text(original)
        # Delays only selected queries, not actual compilation. This exposes the
        # worker lifecycle deterministically without replacing Typst's behavior.
        tools=root/"bin";tools.mkdir();wrapper=tools/"typst"
        wrapper.write_text('#!/bin/sh\nif [ "$1" = "query" ] && [ -f "$CETZ_ROUTE_DELAY" ]; then sleep 1; fi\nexec "$CETZ_REAL_TYPST" "$@"\n')
        wrapper.chmod(0o755);delay=root/"query-delay"
        env={**os.environ,"PATH":str(tools)+os.pathsep+os.environ["PATH"],"CETZ_REAL_TYPST":real_typst,"CETZ_ROUTE_DELAY":str(delay)}
        browser=pw.chromium.launch(headless=True,executable_path=str(args.chromium) if args.chromium else None,args=["--no-sandbox"])
        try:
            with running_app(binary,root,source,out/"server.log",env) as native:
                page=open_page(browser,native.origin,"routing",errors)
                check(browser_snapshot(page)["capabilities"]["graph_gestures"],"native graph gestures available")
                page.locator('#edges-tab').click();page.locator('[data-element="e0"]').click()
                wait_condition(page,"!document.getElementById('routing-preview').disabled")
                page.screenshot(path=str(out/"before.png"),full_page=True)
                started=time.monotonic();page.locator('#routing-preview').click()
                wait_condition(page,"!document.getElementById('routing-apply').disabled")
                timings["cold_preview_wall_ms"]=(time.monotonic()-started)*1000
                proposed=routing_ready(page)["routing"]["proposal"]
                timings["geometry_plus_search_ms"]=proposed["total_ms"]
                timings["search_ms"]=proposed["plan"]["search_ms"]
                check(source.read_text()==original and browser_snapshot(page)["source"]==original,"preview changes neither source nor draft")
                points=proposed["plan"]["routes"][0]["points"]
                check(len(points)>2 and all(abs(a['x']-b['x'])<1e-7 or abs(a['y']-b['y'])<1e-7 for a,b in zip(points,points[1:])),"proposed path is orthogonal")
                # Obstacle's source center is 46,0, size 24x18 mm. Clearance2.
                for a,b in zip(points,points[1:]):
                    for k in range(201):
                        t=k/200;x=a['x']*(1-t)+b['x']*t;y=a['y']*(1-t)+b['y']*t
                        assert not (32<x<60 and -11<y<11), "route entered obstacle clearance"
                checks.append("route avoids intervening node and requested clearance")
                page.screenshot(path=str(out/"proposal.png"),full_page=True)
                page.locator('#routing-apply').click();accepted=wait_revision(page,0)
                check(source.read_text()==original and accepted["dirty"],"apply adopts one draft revision without writing")
                check(accepted["revision"]==1,"route is one undo transaction")
                saved_draft=accepted["source"]
                for text in ['n(0, 0, <source>','n(46, 0, <obstacle>','n(92, 0, <target>','[$bold(x) in RR^d$]','<source.east>','<target.west>','"-|>"','edge-stroke: .8pt']:
                    check(text in saved_draft,"source retained: "+text)
                page.screenshot(path=str(out/"after.png"),full_page=True)
                page.locator('#undo').click();check(wait_revision(page,1)["source"]==original,"undo restores exact bytes")
                page.locator('#redo').click();check(wait_revision(page,2)["source"]==saved_draft,"redo restores compiled route")
                page.locator('#save').click();wait_revision(page,3);check(source.read_text()==saved_draft,"explicit save persists routed source")
                # A consumed proposal is not replayable.
                check(api(page,'/api/routing/apply',{'proposal_id':proposed['id']})['status']==409,"consumed proposal cannot be replayed")
                page.locator('#routing-scope').select_option('all')
                wait_condition(page,"!document.getElementById('routing-preview').disabled")
                page.locator('#routing-preview').click();wait_condition(page,"!document.getElementById('routing-apply').disabled")
                batch=routing_ready(page)
                check(len(batch['routing']['proposal']['plan']['routes'])==2,"all eligible routes excludes unsupported curved edge")
                check(any(e['edge']=='e2' and not e['eligible'] and e['reason'] for e in batch['eligibility']),"unsupported edge has explicit reason")
                page.locator('#routing-discard').click();wait_condition(page,"document.getElementById('routing-apply').disabled")
                check(browser_snapshot(page)['source']==saved_draft,"discard leaves draft intact")
                # Failed routing is transactional.
                failure=api(page,'/api/routing/propose',{'edges':['e0'],'clearance_mm':25})
                check(failure['status']==200,"impossible proposal accepted only as background request")
                failure=routing_ready(page)
                check(bool(failure['routing']['error']) and failure['routing']['proposal'] is None,"blocked port returns no partial proposal")
                check(browser_snapshot(page)['source']==saved_draft,"failed routing retains draft")
                # Query delay proves responsiveness, one-slot bound and stale-result handling.
                delay.touch()
                result=api(page,'/api/routing/propose',{'edges':['e0'],'clearance_mm':2})
                check(result['status']==200,"routing worker starts asynchronously")
                started=time.monotonic();api(page,'/api/state');timings['state_during_query_ms']=(time.monotonic()-started)*1000
                check(timings['state_during_query_ms']<800,"state stays responsive during delayed native query")
                zoom=page.locator('#zoom').inner_text();page.locator('#zoom-in').click()
                check(page.locator('#zoom').inner_text()!=zoom,"viewport stays interactive while routing")
                check(api(page,'/api/routing/propose',{'edges':['e0'],'clearance_mm':2})['status']==409,"one worker slot prevents unbounded queue")
                changed=api(page,'/api/edit',{'command':{'kind':'move_node','id':'obstacle','x':46,'y':5}})
                check(changed['status']==200,"ordinary edit can advance source during routing")
                stale=routing_ready(page)
                check(stale['routing']['proposal'] is None,"obsolete worker result cannot overwrite changed source")
                check(api(page,'/api/routing/apply',{'proposal_id':batch['routing']['proposal']['id']})['status']==409,"stale proposal adoption refused")
                delay.unlink()
                # Disk replacement after preview also prevents adoption.
                check(api(page,'/api/routing/propose',{'edges':['e0'],'clearance_mm':2})['status']==200,"fresh revision can request a new route")
                fresh=routing_ready(page)['routing']['proposal'];check(fresh is not None,"fresh proposal completes")
                before_external=browser_snapshot(page)
                source.write_text(saved_draft+'\n// external writer\n')
                check(api(page,'/api/routing/apply',{'proposal_id':fresh['id']})['status']==409,"external source edit blocks route adoption")
                check(browser_snapshot(page)['source']==before_external['source'],"external edit failure retains in-memory draft")
                source.write_text(saved_draft)
                page.close()
            with running_app(binary,root,source,out/"reopen.log",env) as native:
                page=open_page(browser,native.origin,"routing-reopen",errors)
                state=browser_snapshot(page)
                check(state['source']==saved_draft and state['preview_current'],"saved routes reopen and compile")
                page.close()
            # Deliberate rejected API commands produce browser HTTP console
            # errors; only unexpected JavaScript errors are failure evidence.
            unexpected=[e for e in errors if '409' not in e]
            check(not unexpected,"no unexpected browser errors: "+str(unexpected))
        finally:
            browser.close()
    result={'checks':checks,'count':len(checks),'timings_ms':timings,'platform':os.uname().sysname,'evidence':'native browser + Rust + Typst; disposable fixture'}
    (out/'results.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))


if __name__=='__main__':main()
