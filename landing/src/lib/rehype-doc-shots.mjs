import { readFileSync } from 'node:fs';

// Frame complete device captures; preserve the dimensions of cropped details.
export default function docShots() {
  return (tree) => {
    const visit = (node) => {
      if (!node.children) return;
      node.children = node.children.map((child) => {
        if (child.type !== 'element' || child.tagName !== 'img' ||
            !(String(child.properties?.src).startsWith('/docs/shots/') || String(child.properties?.src).startsWith('/screenshots/ipad-2026-11-07/'))) {
          visit(child);
          return child;
        }
        const src = child.properties.src;
        const png = readFileSync(new URL(`../../public${src}`, import.meta.url));
        const width = png.readUInt32BE(16);
        const height = png.readUInt32BE(20);
        const ratio = width / height;
        const framed = src.endsWith('-framed.png');
        const kind = framed ? 'framed' : ratio > 1.28 && ratio < 1.4 ? 'tablet' : ratio > 0.44 && ratio < 0.49 ? 'phone' : 'detail';
        Object.assign(child.properties, { width, height, loading: 'lazy', decoding: 'async' });
        return {
          type: 'element', tagName: 'a',
          properties: {
            href: framed ? src.replace(/\/([^/]+)-framed\.png$/, '/raw/$1-raw.png') : src, target: '_blank', rel: ['noopener'],
            className: ['shot-link', `shot-${kind}`],
            style: `--shot-width: ${width}px`,
            ariaLabel: `${child.properties.alt || 'Screenshot'} — open full-size image in a new tab`,
          },
          children: [{ type: 'element', tagName: 'span', properties: { className: ['shot-screen'] }, children: [child] }],
        };
      });
    };
    visit(tree);
  };
}
