// The existing control UI behind a small lifecycle seam. The following stack
// replaces these widgets with Tweakpane without changing command admission.
export function createParameterEditor(root, item, {disabled, apply}) {
  const label=document.createElement('label'),input=document.createElement('input');
  label.className='field-label';label.textContent=item.unit?`Value · ${item.unit}`:'Value';
  input.id='parameter-value';label.htmlFor=input.id;
  if(item.kind==='bool'){input.type='checkbox';input.checked=item.value;}
  else if(item.kind==='color'){input.type='text';input.value=item.value;input.pattern='#[0-9a-fA-F]{6}';input.placeholder='#d8eadd';}
  else{input.type='number';input.value=item.value;input.step=item.step??'any';if(item.min!=null)input.min=item.min;if(item.max!=null)input.max=item.max;}
  input.disabled=disabled;
  const button=document.createElement('button');button.className='fullwidth';button.textContent='Apply control';button.id='apply-parameter';button.disabled=disabled;
  button.onclick=()=>{
    if(!input.reportValidity())return;
    const value=item.kind==='bool'?input.checked:item.kind==='color'?input.value:Number(input.value);
    if(input.type==='number'&&(!input.value.trim()||!Number.isFinite(value)))return;
    apply(value);
  };
  root.append(label,input,button);
  return {dispose(){button.onclick=null;}};
}
