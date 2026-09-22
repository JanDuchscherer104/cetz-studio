import createDOMPurify from 'dompurify';

const SVG = 'http://www.w3.org/2000/svg';
const XLINK = 'http://www.w3.org/1999/xlink';
const XMLNS = 'http://www.w3.org/2000/xmlns/';
const purifier = createDOMPurify(window);
const localReference = /^#[A-Za-z_][A-Za-z0-9_.:-]*$/;
const rasterData = /^data:image\/(?:png|jpeg|gif|webp);base64,[A-Za-z0-9+/=\s]+$/i;
const svgData = /^data:image\/svg\+xml;base64,([A-Za-z0-9+/=\s]+)$/i;
const maxEmbeddedSvgBytes = 1024 * 1024;
const maxEmbeddedSvgDepth = 1;
const paintReferences = new Set([
  'fill', 'stroke', 'filter', 'clip-path', 'mask', 'cursor',
  'marker', 'marker-start', 'marker-mid', 'marker-end',
]);
let activeContext = null;

function recordLoss(kind, reason) {
  const losses = activeContext?.losses;
  if (!losses) return;
  const key = `${kind}:${reason}`;
  const existing = losses.get(key);
  if (existing) existing.count += 1;
  else losses.set(key, {kind, reason, count: 1});
}

function decodeBase64Utf8(encoded) {
  const compact = encoded.replace(/\s/g, '');
  if (!compact || compact.length > Math.ceil(maxEmbeddedSvgBytes / 3) * 4) return null;
  try {
    const binary = window.atob(compact);
    if (binary.length > maxEmbeddedSvgBytes) return null;
    const bytes = Uint8Array.from(binary, byte => byte.charCodeAt(0));
    return new TextDecoder('utf-8', {fatal: true}).decode(bytes);
  } catch {
    return null;
  }
}

function encodeBase64Utf8(text) {
  const bytes = new TextEncoder().encode(text);
  let binary = '';
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return window.btoa(binary);
}

function sanitizeEmbeddedSvg(value) {
  if (activeContext.depth >= maxEmbeddedSvgDepth) {
    recordLoss('embedded SVG image', 'nested SVG depth exceeds the supported limit');
    return null;
  }
  const match = svgData.exec(value);
  const text = match && decodeBase64Utf8(match[1]);
  if (!text) {
    recordLoss('embedded SVG image', 'invalid or oversized data');
    return null;
  }
  const nested = new DOMParser().parseFromString(text, 'image/svg+xml');
  if (nested.querySelector('parsererror') || nested.documentElement.namespaceURI !== SVG ||
      nested.documentElement.localName !== 'svg') {
    recordLoss('embedded SVG image', 'malformed SVG data');
    return null;
  }
  const previousDepth = activeContext.depth;
  activeContext.depth += 1;
  try {
    const sanitized = sanitizeRoot(nested.documentElement);
    const serialized = new XMLSerializer().serializeToString(sanitized);
    if (new TextEncoder().encode(serialized).length > maxEmbeddedSvgBytes) {
      recordLoss('embedded SVG image', 'sanitized SVG exceeds the size limit');
      return null;
    }
    return `data:image/svg+xml;base64,${encodeBase64Utf8(serialized)}`;
  } finally {
    activeContext.depth = previousDepth;
  }
}

// DOMPurify owns markup security. This hook only narrows Studio's resource
// policy: local definitions, embedded raster images and recursively sanitized,
// bounded compiler-emitted SVG images; never remote URLs.
purifier.addHook('uponSanitizeAttribute', (node, attribute) => {
  const name = attribute.attrName.toLowerCase();
  const value = attribute.attrValue.trim();
  if (name === 'href' || name === 'xlink:href' || name === 'src') {
    if (localReference.test(value) || (node.localName === 'image' && rasterData.test(value))) return;
    if (node.localName === 'image' && svgData.test(value)) {
      const sanitized = sanitizeEmbeddedSvg(value);
      if (sanitized) {
        attribute.attrValue = sanitized;
        return;
      }
      attribute.keepAttr = false;
      return;
    }
    attribute.keepAttr = false;
    if (node.localName === 'image') recordLoss('image resource', 'blocked by the resource policy');
  }
  if (paintReferences.has(name)) {
    if (/\\|[\u0000-\u001f\u007f]/.test(value)) attribute.keepAttr = false;
    if (/url\s*\(/i.test(value) &&
        !/^url\(\s*(['"]?)#[A-Za-z_][A-Za-z0-9_.:-]*\1\s*\)$/i.test(value)) {
      attribute.keepAttr = false;
      recordLoss('paint resource', 'blocked by the resource policy');
    }
  }
});

function assertSvgRoot(root) {
  if (!purifier.isSupported || root?.namespaceURI !== SVG || root.localName !== 'svg' || root.querySelector('parsererror')) {
    throw new Error('A supported SVG sanitizer and a valid SVG root are required');
  }
}

// XML serializers may spell SVG tags as ns0:svg and XLink as ns1:href.
// DOMPurify's allowlists use qualified node/attribute names, not namespace URIs.
// Canonicalize only the known namespaces, then sanitize every copied node and
// attribute normally. Foreign namespaces must never acquire SVG capabilities.
function canonicalSvgNamespaces(root) {
  const copyElement = source => {
    const target = source.ownerDocument.createElementNS(source.namespaceURI,
      source.namespaceURI === SVG ? source.localName : source.nodeName);
    for (const attribute of source.attributes) {
      // XMLSerializer regenerates declarations from the actual namespace URIs.
      if (attribute.namespaceURI === XMLNS) continue;
      const name = attribute.namespaceURI === XLINK ? `xlink:${attribute.localName}` : attribute.name;
      target.setAttributeNS(attribute.namespaceURI, name, attribute.value);
    }
    return target;
  };
  const result = copyElement(root), pending = [[root, result]];
  while (pending.length) {
    const [source, target] = pending.pop();
    for (const child of source.childNodes) {
      const copy = child.nodeType === 1 ? copyElement(child) : child.cloneNode(true);
      target.appendChild(copy);
      if (child.nodeType === 1) pending.push([child, copy]);
    }
  }
  return result;
}

function sanitizeRoot(root) {
  return purifier.sanitize(canonicalSvgNamespaces(root), {
    IN_PLACE: true,
    USE_PROFILES: {svg: true, svgFilters: true},
    ADD_TAGS: ['use'],
    FORBID_TAGS: ['script', 'foreignObject', 'iframe', 'style', 'animate',
      'set', 'animateTransform', 'animateMotion', 'feImage'],
    FORBID_ATTR: ['style', 'xml:base'],
    ALLOW_DATA_ATTR: false,
  });
}

/**
 * Sanitize a detached SVG root and report only material preview resources that
 * were removed. No source or encoded payload is returned in the report.
 */
export function sanitizeSvg(root) {
  assertSvgRoot(root);
  const previousContext = activeContext;
  activeContext = {depth: 0, losses: new Map()};
  try {
    const svg = sanitizeRoot(root);
    return {svg, losses: [...activeContext.losses.values()]};
  } finally {
    activeContext = previousContext;
  }
}
