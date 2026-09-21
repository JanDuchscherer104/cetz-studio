import createDOMPurify from 'dompurify';

const SVG = 'http://www.w3.org/2000/svg';
const purifier = createDOMPurify(window);
const localReference = /^#[A-Za-z_][A-Za-z0-9_.:-]*$/;
const rasterData = /^data:image\/(?:png|jpeg|gif|webp);base64,[A-Za-z0-9+/=\s]+$/i;
const paintReferences = new Set([
  'fill', 'stroke', 'filter', 'clip-path', 'mask', 'cursor',
  'marker', 'marker-start', 'marker-mid', 'marker-end',
]);

// DOMPurify owns markup security. This hook only narrows Studio's resource
// policy: local definitions and embedded raster images, never remote URLs.
purifier.addHook('uponSanitizeAttribute', (node, attribute) => {
  const name = attribute.attrName.toLowerCase();
  const value = attribute.attrValue.trim();
  if (name === 'href' || name === 'xlink:href' || name === 'src') {
    attribute.keepAttr = localReference.test(value) ||
      (node.localName === 'image' && rasterData.test(value));
  }
  if (paintReferences.has(name)) {
    if (/\\|[\u0000-\u001f\u007f]/.test(value)) attribute.keepAttr = false;
    if (/url\s*\(/i.test(value) &&
        !/^url\(\s*(['"]?)#[A-Za-z_][A-Za-z0-9_.:-]*\1\s*\)$/i.test(value)) {
      attribute.keepAttr = false;
    }
  }
});

/** Sanitize a detached SVG root in place, including attributes on the root. */
export function sanitizeSvg(root) {
  if (!purifier.isSupported || root?.namespaceURI !== SVG || root.localName !== 'svg' ||
      root.querySelector('parsererror')) {
    throw new Error('A supported SVG sanitizer and a valid SVG root are required');
  }
  return purifier.sanitize(root, {
    IN_PLACE: true,
    USE_PROFILES: {svg: true, svgFilters: true},
    ADD_TAGS: ['use'],
    FORBID_TAGS: ['script', 'foreignObject', 'iframe', 'style', 'animate',
      'set', 'animateTransform', 'animateMotion', 'feImage'],
    FORBID_ATTR: ['style', 'xml:base'],
    ALLOW_DATA_ATTR: false,
  });
}
