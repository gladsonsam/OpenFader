// L0 layout CSS helpers: size values accept px, fr, %, or plain numbers (=px).
// Pure and node-testable; shared by all ui/modules/layout/*.svelte containers.

export type SizeValue = number | string;
export type BoxTag = 'div' | 'section' | 'span' | 'header' | 'footer' | 'nav' | 'main' | 'aside';

/** Visual box props (L1): exact card styles from the design, data-driven. */
export interface BoxProps {
  /** Element tag (default 'div'); sections use tag 'section' + label. */
  tag?: BoxTag | string;
  /** aria-label for landmark sections. */
  label?: string;
  /** Full CSS border, e.g. '1px solid var(--u4-line-panel)'. */
  border?: string;
  /** CSS background, e.g. 'var(--u4-panel-bg)'. */
  background?: string;
  /** Border radius (px number or CSS string). */
  radius?: SizeValue;
  minWidth?: SizeValue;
  maxWidth?: SizeValue;
  minHeight?: SizeValue;
  margin?: SizeValue;
  /** Full CSS overflow, e.g. 'hidden' (proc wells clip their SVG thumbs). */
  overflow?: string;
  /** CSS position, e.g. 'relative' (anchor for an absolutely-positioned popup). */
  position?: string;
  /** Full CSS box-shadow, e.g. the strip selection ring. */
  shadow?: string;
  /** box-sizing override; cards use 'content-box' like the design. */
  box?: 'content-box' | 'border-box';
}

/** Normalise a size prop: numbers become px, strings pass through verbatim. */
export function sizeCss(v: SizeValue | undefined): string | undefined {
  if (v === undefined || v === null) return undefined;
  if (typeof v === 'number') return `${v}px`;
  const s = v.trim();
  return s === '' ? undefined : s;
}

/** Join style fragments, dropping empties. */
export function styleJoin(parts: Array<string | undefined | false>): string {
  return parts.filter(Boolean).join(';');
}

export function gapCss(gap: SizeValue | undefined): string | undefined {
  const s = sizeCss(gap);
  return s === undefined ? undefined : `gap:${s}`;
}

/** Style fragments for the L1 visual box props (tag/label excluded). */
export function boxStyle(p: BoxProps): Array<string | undefined | false> {
  return [
    p.border ? `border:${p.border}` : undefined,
    p.background ? `background:${p.background}` : undefined,
    sizeCss(p.radius) ? `border-radius:${sizeCss(p.radius)}` : undefined,
    sizeCss(p.minWidth) ? `min-width:${sizeCss(p.minWidth)}` : undefined,
    sizeCss(p.maxWidth) ? `max-width:${sizeCss(p.maxWidth)}` : undefined,
    sizeCss(p.minHeight) ? `min-height:${sizeCss(p.minHeight)}` : undefined,
    sizeCss(p.margin) ? `margin:${sizeCss(p.margin)}` : undefined,
    p.overflow ? `overflow:${p.overflow}` : undefined,
    p.position ? `position:${p.position}` : undefined,
    p.shadow ? `box-shadow:${p.shadow}` : undefined,
    p.box ? `box-sizing:${p.box}` : undefined,
  ];
}
