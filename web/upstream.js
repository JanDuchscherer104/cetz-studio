// DOMPurify owns SVG sanitization. Studio only constrains resource access.
import createDOMPurify from 'dompurify';

const purifier = createDOMPurify(window);
const fragment = /^#[^\s"'<>]+$/u;
const raster = /^data:image\/(?:png|jpeg|gif|webp);base64,[a-z\d+/=\s]+$/iu;
const localPaint = /^url\(\s*(['"]?)(#[^\s"'()<>]+)\1\s*\)$/iu;
const paintAttributes = new Set(['fill', 'stroke', 'filter', 'clip-path', 'mask',
  'marker', 'marker-start', 'marker-mid', 'marker-end', 'cursor']);

purifier.addHook('uponSanitizeAttribute', (node, attribute) => {
  const name = attribute.attrName.toLowerCase();
  const value = attribute.attrValue;
  if (name === 'href' || name === 'xlink:href' || name === 'src') {
    attribute.keepAttr = fragment.test(value) ||
      (node.localName === 'image' && raster.test(value));
  }
  // Presentation attributes can fetch URLs too. CSS escapes are unnecessary
  // in compiler output and are rejected rather than parsed by a home-grown CSS parser.
  if (paintAttributes.has(name) &&
      (/[\\\u0000-\u001f]/u.test(value) ||
       (/url\s*\(/iu.test(value) && !localPaint.test(value)))) {
    attribute.keepAttr = false;
  }
});

export function sanitizeSvg(root) {
  if (root?.namespaceURI !== 'http://www.w3.org/2000/svg' || root.localName !== 'svg') {
    throw new Error('Preview is not an SVG document');
  }
  return purifier.sanitize(root, {
    IN_PLACE: true,
    USE_PROFILES: {svg: true, svgFilters: true},
    ADD_TAGS: ['use'],
    FORBID_TAGS: ['style', 'foreignObject', 'a', 'feImage', 'animate', 'set',
      'animateTransform', 'animateMotion', 'discard'],
    FORBID_ATTR: ['style', 'xml:base'],
    ALLOW_DATA_ATTR: false,
  });
}
