import {Pane} from 'tweakpane';

/** Widgets only. Rust admits values, patches source and owns history/saving. */
export function createParameterEditor(root, item, {disabled, preview, apply}) {
  if (!['number', 'length', 'bool', 'color'].includes(item.kind)) {
    throw new Error(`Unsupported declared control: ${item.kind}`);
  }
  const values = {value: item.value};
  let current = item;
  let changed = false;
  let transaction = null;
  let timer = null;
  let sequence = 0;
  const pane = new Pane({container: root});
  pane.element.id = 'parameter-pane';
  // Use upstream theme properties; do not let the host's dark button-hover
  // style combine with Tweakpane's default dark-on-light foreground.
  for (const [property, value] of Object.entries({
    'base-background-color':'var(--panel, #151c26)',
    'button-background-color':'var(--line, #293443)',
    'button-background-color-hover':'var(--line, #293443)',
    'button-background-color-focus':'var(--line, #293443)',
    'button-background-color-active':'var(--line, #293443)',
    'button-foreground-color':'var(--text, #e1e7ef)',
    'input-background-color':'var(--bg, #10151d)',
    'input-foreground-color':'var(--text, #e1e7ef)',
    'label-foreground-color':'var(--muted, #8e9caf)',
  })) pane.element.style.setProperty(`--tp-${property}`, value);
  const options = {label: item.unit ? `Value · ${item.unit}` : 'Value'};
  if (item.kind === 'number' || item.kind === 'length') {
    options.format = value => String(value);
    for (const key of ['min', 'max']) if (item[key] != null) options[key] = item[key];
    // Tweakpane rounds steps from the initial value; Rust validates from min/0.
    // Sensitivity is upstream; normalize staged values to the source grid below.
    if (item.step != null) { options.keyScale = item.step; options.pointerScale = item.step; }
  }
  const binding = pane.addBinding(values, 'value', options);
  const same = (left, right) => left === right ||
    (current.kind === 'color' && String(left).toLowerCase() === String(right).toLowerCase());
  const valid = value => {
    if (current.kind !== 'number' && current.kind !== 'length') return current.kind !== 'color' || /^#[0-9a-f]{6}$/i.test(value);
    if (!Number.isFinite(Number(value))) return false;
    if (current.min != null && Number(value) < current.min) return false;
    if (current.max != null && Number(value) > current.max) return false;
    return true;
  };
  const cancelTimer = () => { if (timer) { clearTimeout(timer); timer = null; } };
  const emitPreview = () => {
    timer = null;
    if (changed && valid(values.value) && preview) preview(values.value, transaction);
  };
  const schedulePreview = (flush = false) => {
    cancelTimer();
    if (!changed || !valid(values.value) || !preview) return;
    if (flush) emitPreview();
    else timer = setTimeout(emitPreview, 200);
  };
  binding.on('change', event => {
    changed = true;
    if (!transaction) transaction = `${current.id}:${++sequence}`;
    if ((current.kind === 'number' || current.kind === 'length') && current.step != null) {
      // Tweakpane has no public step-origin option. Keep this source-policy
      // conversion at the adapter; do not alter its widgets or Rust admission.
      const origin = current.min ?? 0;
      let tick = Math.round((values.value - origin) / current.step);
      if (current.min != null) tick = Math.max(0, tick);
      if (current.max != null) tick = Math.min(tick, Math.floor((current.max - origin) / current.step + 1e-7));
      const aligned = Math.min(current.max ?? Infinity, Math.max(current.min ?? -Infinity, origin + tick * current.step));
      if (values.value !== aligned) { values.value = aligned; binding.refresh(); }
    }
    schedulePreview(Boolean(event?.last));
  });
  const input = binding.element.querySelector('input');
  if (input) { input.id = 'parameter-value'; input.setAttribute('aria-label', options.label); }
  const button = pane.addButton({title: 'Apply control'});
  button.element.querySelector('button').id = 'apply-parameter';
  button.on('click', () => {
    schedulePreview(true);
    // Merely opening/applying a picker must not normalize the saved literal.
    apply(!changed || same(values.value, current.value) ? current.value : values.value, transaction);
    cancelTimer();
    changed = false;
    transaction = null;
  });
  pane.disabled = disabled;
  const note = document.createElement('p');
  note.className = 'field-note';
  note.textContent = 'Widgets stage step-aligned values until Apply. Rust revalidates every command. Save writes the source.';
  root.append(note);
  return {
    matches(id) { return current.id === id; },
    update(next, nextDisabled) {
      current = next;
      pane.disabled = nextDisabled;
      if (!changed && !same(values.value, next.value)) {
        values.value = next.value;
        binding.refresh();
      }
    },
    dispose() { cancelTimer(); pane.dispose(); note.remove(); },
  };
}
