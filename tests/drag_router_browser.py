"""Test the real upstream router and the fail-closed preview adapter in Chromium."""
from pathlib import Path
import json
import os
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
with sync_playwright() as runtime:
    options = {"headless": True}
    if os.environ.get("CHROMIUM"):
        options["executable_path"] = os.environ["CHROMIUM"]
    browser = runtime.chromium.launch(**options)
    try:
        page = browser.new_page()
        page.add_script_tag(path=str(ROOT / 'web/dist/ui.js'))
        checks = page.evaluate('''() => {
          const results = [];
          const check = (ok,name) => {if(!ok)throw new Error(name);results.push(name);};
          const a={x:0,y:0}, b={x:200,y:0}, obstacle={minX:80,minY:-30,maxX:120,maxY:30};
          const before=document.body.innerHTML;
          const route=CetzUi.routePreview(a,b,[obstacle]);
          check(route && route.length>2, 'Upstream route detours around an intervening obstacle');
          check(JSON.stringify(route[0])===JSON.stringify(a) && JSON.stringify(route.at(-1))===JSON.stringify(b), 'Measured endpoints retained');
          check(route.slice(1).every((p,i)=>p.x===route[i].x || p.y===route[i].y), 'Only orthogonal segments are exposed');
          check(JSON.stringify(route)===JSON.stringify(CetzUi.routePreview(a,b,[obstacle])), 'Repeated fixture routing is deterministic');
          check(CetzUi.routePreview(a,b,[{minX:190,minY:-20,maxX:210,maxY:20}])===null, 'Unsafe fallback through enclosed target is suppressed');
          check(CetzUi.routePreview({x:NaN,y:0},b,[])===null, 'Nonfinite input rejected');
          check(CetzUi.routePreview(a,b,Array(129).fill(obstacle))===null, 'Obstacle-count limit enforced');
          check(CetzUi.routePreview(a,b,[{minX:9,minY:0,maxX:2,maxY:1}])===null, 'Inverted bounds rejected');
          const preciseA={x:12.345,y:23.789}, preciseB={x:212.345,y:23.789};
          const precise=CetzUi.routePreview(preciseA,preciseB,[obstacle]);
          check(precise && JSON.stringify(precise[0])===JSON.stringify(preciseA) && JSON.stringify(precise.at(-1))===JSON.stringify(preciseB), 'Fractional measured endpoints survive upstream rounding');
          check(precise.slice(1).every((p,i)=>p.x===precise[i].x || p.y===precise[i].y), 'Fractional attachment correction remains orthogonal');
          const vertical=CetzUi.routePreview({x:1.234,y:0},{x:1.234,y:200},[]);
          check(vertical && vertical[1].y>0, 'Vertical ports follow the dominant displacement');
          check(CetzUi.routePreview(a,b,[],{start:'diagonal'})===null, 'Unsupported port directions stay unavailable');
          check(CetzUi.routePreview(a,{x:5000,y:0},[])===null, 'Oversized projection is bounded before routing');
          check(CetzUi.routePreview(a,b,[null])===null, 'Malformed obstacle is rejected before upstream code');
          for(let i=0;i<10;i++)CetzUi.routePreview(a,b,[obstacle]);
          check(document.body.innerHTML===before, 'Disposable graph leaves no canvas or DOM behind');
          return results;
        }''')
        print(json.dumps({'evidence':'real-maxgraph-browser','passed':len(checks),'checks':checks},indent=2))
    finally:
        browser.close()
