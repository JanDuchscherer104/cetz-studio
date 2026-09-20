import {Pane} from 'tweakpane';

/** Widgets only. Rust admits values, patches source and owns history/saving. */
export function createParameterEditor(root, item, {disabled, apply}) {
  if (!['number', 'length', 'bool', 'color'].includes(item.kind)) {
    throw new Error(`Unsupported declared control: ${item.kind}`);
  }
  const values = {value: item.value};
  const pane = new Pane({container: root});
  pane.element.id = 'parameter-pane';
  const options = {label: item.unit ? `Value · ${item.unit}` : 'Value'};
  if (item.kind === 'number' || item.kind === 'length') {
    options.format = value => String(value);
    for (const key of ['min', 'max']) if (item[key] != null) options[key] = item[key];
    // Tweakpane rounds steps from the initial value; Rust validates from min/0.
    // Reuse keyboard/pointer scaling, not a conflicting second step validator.
    if (item.step != null) { options.keyScale = item.step; options.pointerScale = item.step; }
  }
  const binding = pane.addBinding(values, 'value', options);
  let changed = false;
  binding.on('change', () => { changed = true; });
  const input = binding.element.querySelector('input');
  if (input) { input.id = 'parameter-value'; input.setAttribute('aria-label', options.label); }
  const button = pane.addButton({title: 'Apply control'});
  button.element.querySelector('button').id = 'apply-parameter';
  button.on('click', () => {
    // Merely opening/applying a picker must not normalize the saved literal.
    const same = values.value === item.value ||
      (item.kind === 'color' && values.value.toLowerCase() === item.value.toLowerCase());
    apply(!changed || same ? item.value : values.value);
  });
  pane.disabled = disabled;
  const note = document.createElement('p');
  note.className = 'field-note';
  note.textContent = 'Widgets stage values until Apply. Ranges constrain input; Rust validates declared steps. Save writes the source.';
  root.append(note);
  return {dispose() { pane.dispose(); note.remove(); }};
}
