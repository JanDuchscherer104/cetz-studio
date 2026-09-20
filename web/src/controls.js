import {Pane} from 'tweakpane';

/** Presentation only: Rust still admits values, patches source and owns history. */
export function mountParameter(container, item, disabled) {
  if (!['number', 'length', 'bool', 'color'].includes(item.kind)) {
    throw new Error(`Unsupported declared control: ${item.kind}`);
  }
  const values = {value: item.value};
  const pane = new Pane({container});
  pane.element.id = 'parameter-pane';
  const options = {label: item.unit ? `Value · ${item.unit}` : 'Value'};
  if (item.kind === 'number' || item.kind === 'length') {
    options.format = value => String(value);
    for (const key of ['min', 'max', 'step']) {
      if (item[key] != null) options[key] = item[key];
    }
  }
  const binding = pane.addBinding(values, 'value', options);
  let changed = false;
  binding.on('change', () => { changed = true; });
  // Label the upstream input, not a parallel hidden compatibility widget.
  const input = binding.element.querySelector('input');
  if (input) {
    input.id = 'parameter-value';
    input.setAttribute('aria-label', options.label);
  }
  pane.disabled = disabled;
  return {
    value() {
      // A picker may normalize spelling while mounting. Opening/applying a
      // control must not change the source's original value or hex case.
      if (!changed || values.value === item.value ||
          (item.kind === 'color' && values.value.toLowerCase() === item.value.toLowerCase())) {
        return item.value;
      }
      return values.value;
    },
    dispose() { pane.dispose(); },
  };
}
