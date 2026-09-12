/**
 * The template dialect: what a page, layout or boundary under `routes/` is
 * written in. TSX that the build lowers and the server renders, typed here
 * without React, so an application that ships no React reads no React
 * declarations either. `Island`, `island`, `Link` and `Slot` are the same
 * placements `@snapfire/fsr-client/react` offers a React application; the
 * lowerer reads either import.
 */

/** What a template element is once lowered. Opaque: nothing reads it. */
export interface TemplateNode {
  readonly __sfTemplate: true;
}

/** What may sit between an element's tags. */
export type Children = TemplateNode | string | number | bigint | boolean | null | undefined | readonly Children[];

export type MountTiming = "load" | "visible" | "idle";
export type PrefetchTiming = "hover" | "viewport" | "none";

type Booleanish = boolean | "true" | "false";
type Handler<E = Event> = (event: E) => void;

/** CSS as a template writes it: React's property names, so `style={{ marginTop: 4 }}`. */
export type StyleValue = { [property: string]: string | number | null | undefined };

/** The attributes every element takes. */
export interface Attributes {
  children?: Children;
  key?: string | number | bigint;
  id?: string;
  className?: string;
  style?: StyleValue;
  title?: string;
  lang?: string;
  dir?: "ltr" | "rtl" | "auto";
  hidden?: boolean;
  tabIndex?: number;
  role?: string;
  draggable?: Booleanish;
  contentEditable?: Booleanish | "inherit";
  spellCheck?: Booleanish;
  translate?: "yes" | "no";
  slot?: string;
  dangerouslySetInnerHTML?: { __html: string };
  onClick?: Handler<MouseEvent>;
  onDoubleClick?: Handler<MouseEvent>;
  onMouseDown?: Handler<MouseEvent>;
  onMouseUp?: Handler<MouseEvent>;
  onMouseEnter?: Handler<MouseEvent>;
  onMouseLeave?: Handler<MouseEvent>;
  onMouseMove?: Handler<MouseEvent>;
  onPointerDown?: Handler<PointerEvent>;
  onPointerUp?: Handler<PointerEvent>;
  onKeyDown?: Handler<KeyboardEvent>;
  onKeyUp?: Handler<KeyboardEvent>;
  onKeyPress?: Handler<KeyboardEvent>;
  onFocus?: Handler<FocusEvent>;
  onBlur?: Handler<FocusEvent>;
  onInput?: Handler<Event>;
  onChange?: Handler<Event>;
  onSubmit?: Handler<Event>;
  onScroll?: Handler<Event>;
  onWheel?: Handler<WheelEvent>;
  onTouchStart?: Handler<TouchEvent>;
  onTouchEnd?: Handler<TouchEvent>;
  onTouchMove?: Handler<TouchEvent>;
  onLoad?: Handler<Event>;
  onError?: Handler<Event>;
  [aria: `aria-${string}`]: string | number | boolean | undefined;
  [data: `data-${string}`]: string | number | boolean | undefined;
}

export interface AnchorAttributes extends Attributes {
  href?: string;
  target?: "_self" | "_blank" | "_parent" | "_top" | (string & {});
  rel?: string;
  download?: string | boolean;
  hrefLang?: string;
  type?: string;
  referrerPolicy?: string;
}

export interface ImageAttributes extends Attributes {
  src?: string;
  srcSet?: string;
  sizes?: string;
  alt?: string;
  width?: number | string;
  height?: number | string;
  loading?: "eager" | "lazy";
  decoding?: "async" | "auto" | "sync";
  crossOrigin?: "anonymous" | "use-credentials" | "";
  referrerPolicy?: string;
}

export interface InputAttributes extends Attributes {
  type?: string;
  name?: string;
  value?: string | number | readonly string[];
  defaultValue?: string | number | readonly string[];
  checked?: boolean;
  defaultChecked?: boolean;
  placeholder?: string;
  disabled?: boolean;
  readOnly?: boolean;
  required?: boolean;
  autoComplete?: string;
  autoFocus?: boolean;
  min?: number | string;
  max?: number | string;
  step?: number | string;
  minLength?: number;
  maxLength?: number;
  pattern?: string;
  multiple?: boolean;
  accept?: string;
  list?: string;
  form?: string;
  inputMode?: "none" | "text" | "tel" | "url" | "email" | "numeric" | "decimal" | "search";
  size?: number;
}

export interface ButtonAttributes extends Attributes {
  type?: "submit" | "reset" | "button";
  name?: string;
  value?: string | number;
  disabled?: boolean;
  form?: string;
  formAction?: string;
  formMethod?: string;
  autoFocus?: boolean;
}

export interface FormAttributes extends Attributes {
  action?: string;
  method?: "get" | "post" | "dialog" | (string & {});
  encType?: string;
  target?: string;
  autoComplete?: "on" | "off";
  noValidate?: boolean;
  acceptCharset?: string;
  name?: string;
}

export interface LabelAttributes extends Attributes {
  htmlFor?: string;
  form?: string;
}

export interface SelectAttributes extends Attributes {
  name?: string;
  value?: string | number | readonly string[];
  defaultValue?: string | number | readonly string[];
  disabled?: boolean;
  required?: boolean;
  multiple?: boolean;
  size?: number;
  autoComplete?: string;
  autoFocus?: boolean;
  form?: string;
}

export interface OptionAttributes extends Attributes {
  value?: string | number | readonly string[];
  label?: string;
  selected?: boolean;
  disabled?: boolean;
}

export interface OptionGroupAttributes extends Attributes {
  label?: string;
  disabled?: boolean;
}

export interface TextAreaAttributes extends Attributes {
  name?: string;
  value?: string | number | readonly string[];
  defaultValue?: string | number | readonly string[];
  placeholder?: string;
  rows?: number;
  cols?: number;
  disabled?: boolean;
  readOnly?: boolean;
  required?: boolean;
  minLength?: number;
  maxLength?: number;
  wrap?: string;
  autoComplete?: string;
  autoFocus?: boolean;
  form?: string;
}

export interface TableCellAttributes extends Attributes {
  colSpan?: number;
  rowSpan?: number;
  headers?: string;
  scope?: string;
  abbr?: string;
}

export interface TableColAttributes extends Attributes {
  span?: number;
}

export interface ListAttributes extends Attributes {
  start?: number;
  reversed?: boolean;
  type?: "1" | "a" | "A" | "i" | "I";
}

export interface ListItemAttributes extends Attributes {
  value?: number;
}

export interface TimeAttributes extends Attributes {
  dateTime?: string;
}

export interface QuoteAttributes extends Attributes {
  cite?: string;
}

export interface DetailsAttributes extends Attributes {
  open?: boolean;
  name?: string;
}

export interface DialogAttributes extends Attributes {
  open?: boolean;
}

export interface ProgressAttributes extends Attributes {
  value?: number | string;
  max?: number | string;
}

export interface MeterAttributes extends Attributes {
  value?: number | string;
  min?: number | string;
  max?: number | string;
  low?: number | string;
  high?: number | string;
  optimum?: number | string;
}

export interface IframeAttributes extends Attributes {
  src?: string;
  srcDoc?: string;
  name?: string;
  width?: number | string;
  height?: number | string;
  allow?: string;
  allowFullScreen?: boolean;
  sandbox?: string;
  loading?: "eager" | "lazy";
  referrerPolicy?: string;
}

export interface MediaAttributes extends Attributes {
  src?: string;
  autoPlay?: boolean;
  controls?: boolean;
  loop?: boolean;
  muted?: boolean;
  playsInline?: boolean;
  poster?: string;
  preload?: "none" | "metadata" | "auto" | "";
  width?: number | string;
  height?: number | string;
  crossOrigin?: "anonymous" | "use-credentials" | "";
}

export interface SourceAttributes extends Attributes {
  src?: string;
  srcSet?: string;
  sizes?: string;
  type?: string;
  media?: string;
}

export interface CanvasAttributes extends Attributes {
  width?: number | string;
  height?: number | string;
}

export interface ScriptAttributes extends Attributes {
  src?: string;
  type?: string;
  async?: boolean;
  defer?: boolean;
  noModule?: boolean;
  crossOrigin?: string;
  integrity?: string;
  nonce?: string;
}

export interface LinkElementAttributes extends Attributes {
  href?: string;
  rel?: string;
  type?: string;
  as?: string;
  media?: string;
  sizes?: string;
  hrefLang?: string;
  crossOrigin?: string;
  integrity?: string;
}

export interface MetaAttributes extends Attributes {
  name?: string;
  content?: string;
  charSet?: string;
  httpEquiv?: string;
  property?: string;
}

/** SVG as a template writes it: React's camel-cased spellings, which the renderer prints in SVG's own. Any attribute goes, since SVG has hundreds. */
export interface SvgAttributes extends Attributes {
  [attribute: string]: unknown;
}

/** A custom element's attributes are its own; the dialect checks only the ones every element has. */
export interface CustomElementAttributes extends Attributes {
  [attribute: string]: unknown;
}

export interface TemplateAttributes extends Attributes {
  shadowrootmode?: "open" | "closed";
  shadowrootdelegatesfocus?: boolean;
  shadowrootclonable?: boolean;
  shadowrootserializable?: boolean;
}

/** The elements a template may write. A tag not listed here is a typo the checker catches, unless it carries a hyphen, which makes it a custom element the browser defines. */
export interface Intrinsic {
  [custom: `${string}-${string}`]: CustomElementAttributes;
  a: AnchorAttributes;
  abbr: Attributes;
  address: Attributes;
  article: Attributes;
  aside: Attributes;
  audio: MediaAttributes;
  b: Attributes;
  bdi: Attributes;
  bdo: Attributes;
  blockquote: QuoteAttributes;
  br: Attributes;
  button: ButtonAttributes;
  canvas: CanvasAttributes;
  caption: Attributes;
  cite: Attributes;
  code: Attributes;
  col: TableColAttributes;
  colgroup: TableColAttributes;
  data: Attributes & { value?: string | number };
  datalist: Attributes;
  dd: Attributes;
  del: QuoteAttributes & { dateTime?: string };
  details: DetailsAttributes;
  dfn: Attributes;
  dialog: DialogAttributes;
  div: Attributes;
  dl: Attributes;
  dt: Attributes;
  em: Attributes;
  fieldset: Attributes & { disabled?: boolean; form?: string; name?: string };
  figcaption: Attributes;
  figure: Attributes;
  footer: Attributes;
  form: FormAttributes;
  h1: Attributes;
  h2: Attributes;
  h3: Attributes;
  h4: Attributes;
  h5: Attributes;
  h6: Attributes;
  header: Attributes;
  hgroup: Attributes;
  hr: Attributes;
  i: Attributes;
  iframe: IframeAttributes;
  img: ImageAttributes;
  input: InputAttributes;
  ins: QuoteAttributes & { dateTime?: string };
  kbd: Attributes;
  label: LabelAttributes;
  legend: Attributes;
  li: ListItemAttributes;
  link: LinkElementAttributes;
  main: Attributes;
  mark: Attributes;
  menu: Attributes;
  meta: MetaAttributes;
  meter: MeterAttributes;
  nav: Attributes;
  noscript: Attributes;
  object: Attributes & { data?: string; type?: string; name?: string; width?: number | string; height?: number | string };
  ol: ListAttributes;
  optgroup: OptionGroupAttributes;
  option: OptionAttributes;
  output: Attributes & { htmlFor?: string; form?: string; name?: string };
  p: Attributes;
  picture: Attributes;
  pre: Attributes;
  progress: ProgressAttributes;
  q: QuoteAttributes;
  s: Attributes;
  samp: Attributes;
  script: ScriptAttributes;
  search: Attributes;
  section: Attributes;
  select: SelectAttributes;
  slot: Attributes & { name?: string };
  small: Attributes;
  source: SourceAttributes;
  span: Attributes;
  strong: Attributes;
  style: Attributes & { media?: string; nonce?: string };
  sub: Attributes;
  summary: Attributes;
  sup: Attributes;
  table: Attributes;
  tbody: Attributes;
  td: TableCellAttributes;
  template: TemplateAttributes;
  textarea: TextAreaAttributes;
  tfoot: Attributes;
  th: TableCellAttributes;
  thead: Attributes;
  time: TimeAttributes;
  tr: Attributes;
  track: Attributes & { src?: string; kind?: string; srcLang?: string; label?: string; default?: boolean };
  u: Attributes;
  ul: Attributes;
  var: Attributes;
  video: MediaAttributes;
  wbr: Attributes;
  svg: SvgAttributes;
  path: SvgAttributes;
  circle: SvgAttributes;
  ellipse: SvgAttributes;
  line: SvgAttributes;
  polyline: SvgAttributes;
  polygon: SvgAttributes;
  rect: SvgAttributes;
  g: SvgAttributes;
  defs: SvgAttributes;
  use: SvgAttributes;
  symbol: SvgAttributes;
  text: SvgAttributes;
  tspan: SvgAttributes;
  title: SvgAttributes;
  desc: SvgAttributes;
  clipPath: SvgAttributes;
  mask: SvgAttributes;
  pattern: SvgAttributes;
  marker: SvgAttributes;
  linearGradient: SvgAttributes;
  radialGradient: SvgAttributes;
  stop: SvgAttributes;
  filter: SvgAttributes;
  image: SvgAttributes;
  foreignObject: SvgAttributes;
}

/** A component a template renders: a function of its props. */
export type Component<P = {}> = (props: P) => TemplateNode | null;

export interface IslandProps {
  /** When the island hydrates: immediately, when scrolled into view or when the main thread is idle. Defaults to the registry's timing, else "load". */
  when?: MountTiming;
  /** `server`: the island's events round-trip to the server, which re-renders it; no root is mounted. */
  mode?: "server";
  children?: Children;
}

/** Places its one child component as an island of its own, mounted in the browser at the timing asked for. Lowered by the build. */
export function Island(props: IslandProps): TemplateNode;

/** `component` as an island with a fixed timing, for a page to place like any component: `const Seen = island(Feedback, { when: "visible" })`. */
export function island<P extends object>(component: Component<P>, options?: { when?: MountTiming; mode?: "server" }): Component<P>;

export interface SlotProps {
  /** The slot's name: a `slots/<name>` directory beside the layout or the slot a `page.<name>.tsx` under it renders into. */
  name: string;
  /** What the slot shows while nothing fills it. Rendered by the server, lowered by the build. */
  children?: Children;
}

/** A layout's named slot, where the parallel segment of that name renders. */
export function Slot(props: SlotProps): TemplateNode;

export interface LinkProps extends AnchorAttributes {
  /** Always the document's rendering of the target, never an intercept into a slot. */
  full?: boolean;
  /** Renders the target into this slot of the nearest live layout that declares it, whether or not the server would intercept from here. */
  into?: string;
  /** Whether the navigator fetches the target ahead of a click. */
  prefetch?: PrefetchTiming;
  /** Leaves the click to the browser: a full document load. */
  native?: boolean;
}

/** An `<a>` the navigator reads: `full`, `into`, `prefetch` and `native` ride as `data-sf-*` attributes. */
export function Link(props: LinkProps): TemplateNode;
