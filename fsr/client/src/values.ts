export type RefValue = { readonly kind: "action" | "module"; readonly id: string };
export type VariantValue = { readonly tag: string; readonly payload?: SfValue };

const REF_MARK = Symbol.for("sf.ref");
const VARIANT_MARK = Symbol.for("sf.variant");

export function ref(kind: "action" | "module", id: string): RefValue {
  const v = { kind, id } as RefValue;
  (v as never as { [k: symbol]: boolean })[REF_MARK] = true;
  return Object.freeze(v);
}

export const actionRef = (id: string): RefValue => ref("action", id);
export const moduleRef = (id: string): RefValue => ref("module", id);

export function variant(tag: string, payload?: SfValue): VariantValue {
  const v = (payload === undefined ? { tag } : { tag, payload }) as VariantValue;
  (v as never as { [k: symbol]: boolean })[VARIANT_MARK] = true;
  return Object.freeze(v);
}

export function isRef(v: unknown): v is RefValue {
  return typeof v === "object" && v !== null && (v as { [k: symbol]: unknown })[REF_MARK] === true;
}

export function isVariant(v: unknown): v is VariantValue {
  return typeof v === "object" && v !== null && (v as { [k: symbol]: unknown })[VARIANT_MARK] === true;
}

export type SfValue =
  | null
  | boolean
  | number
  | bigint
  | string
  | Uint8Array
  | Int8Array
  | Int16Array
  | Uint16Array
  | Int32Array
  | Uint32Array
  | BigInt64Array
  | BigUint64Array
  | Float32Array
  | Float64Array
  | SfValue[]
  | RefValue
  | VariantValue
  | { [key: string]: SfValue };

const TYPED_ARRAYS: Record<string, new (buf: ArrayBuffer) => SfValue> = {
  i8: Int8Array,
  u8: Uint8Array,
  i16: Int16Array,
  u16: Uint16Array,
  i32: Int32Array,
  u32: Uint32Array,
  i64: BigInt64Array,
  u64: BigUint64Array,
  f32: Float32Array,
  f64: Float64Array,
};

function bytesFromBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) {
    bytes[i] = bin.charCodeAt(i);
  }
  return bytes;
}

function decodeFloat(v: unknown): number {
  if (v === "nan") return NaN;
  if (v === "inf") return Infinity;
  if (v === "-inf") return -Infinity;
  return v as number;
}

function decodeTagged(obj: { [key: string]: unknown }): SfValue {
  const tag = obj["$"] as string;
  switch (tag) {
    case "i":
    case "u": {
      const big = BigInt(obj["v"] as string);
      return big >= BigInt(Number.MIN_SAFE_INTEGER) && big <= BigInt(Number.MAX_SAFE_INTEGER)
        ? Number(big)
        : big;
    }
    case "f":
    case "f32":
      return decodeFloat(obj["v"]);
    case "b":
      return bytesFromBase64(obj["v"] as string);
    case "ta": {
      const ctor = TYPED_ARRAYS[obj["k"] as string];
      if (!ctor) throw new Error(`unknown typed array kind: ${obj["k"]}`);
      const bytes = bytesFromBase64(obj["v"] as string);
      const array = new ctor(bytes.buffer);
      if (obj["k"] === "u8") Object.defineProperty(array, U8_TYPED, { value: true });
      return array;
    }
    case "m": {
      const out: { [key: string]: SfValue } = {};
      for (const [k, v] of obj["v"] as [string, unknown][]) {
        out[k] = decodeValue(v);
      }
      return out;
    }
    case "var": {
      const payload = obj["p"];
      return payload === undefined
        ? variant(obj["t"] as string)
        : variant(obj["t"] as string, decodeValue(payload));
    }
    case "ref":
      return ref(obj["k"] as "action" | "module", obj["id"] as string);
    default:
      throw new Error(`unknown value tag: ${tag}`);
  }
}

/** Decodes the JSON pair's output back into JS values. Untagged JSON passes through untouched. */
export function decodeValue(json: unknown): SfValue {
  if (json === null || typeof json === "boolean" || typeof json === "number" || typeof json === "string") {
    return json;
  }
  if (Array.isArray(json)) {
    return json.map(decodeValue);
  }
  const obj = json as { [key: string]: unknown };
  if (typeof obj["$"] === "string") {
    return decodeTagged(obj);
  }
  const out: { [key: string]: SfValue } = {};
  for (const key of Object.keys(obj)) {
    out[key] = decodeValue(obj[key]);
  }
  return out;
}

/** Marks a `Uint8Array` that arrived as a `u8` typed array rather than as bytes, so it goes back the way it came. A `Uint8Array` the page made is bytes. */
const U8_TYPED = Symbol("sf.u8");

const TYPED_ARRAY_KINDS: [new (...a: never[]) => object, string][] = [
  [Int8Array, "i8"],
  [Int16Array, "i16"],
  [Uint16Array, "u16"],
  [Int32Array, "i32"],
  [Uint32Array, "u32"],
  [BigInt64Array, "i64"],
  [BigUint64Array, "u64"],
  [Float32Array, "f32"],
  [Float64Array, "f64"],
];

function base64FromBytes(bytes: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
  return btoa(bin);
}

const I128_MAX = (1n << 127n) - 1n;
const I128_MIN = -(1n << 127n);
const U128_MAX = (1n << 128n) - 1n;

/** Encodes JS values into the JSON pair's tagged form for the server. JS has one number type, so an integral number arrives as an Int; Uint8Array arrives as bytes. */
export function encodeValue(v: SfValue): unknown {
  if (v === null || typeof v === "boolean" || typeof v === "string") return v;
  if (typeof v === "number") {
    if (Number.isNaN(v)) return { $: "f", v: "nan" };
    if (v === Infinity) return { $: "f", v: "inf" };
    if (v === -Infinity) return { $: "f", v: "-inf" };
    return v;
  }
  if (typeof v === "bigint") {
    if (v >= I128_MIN && v <= I128_MAX) return { $: "i", v: v.toString() };
    if (v > I128_MAX && v <= U128_MAX) return { $: "u", v: v.toString() };
    throw new Error("bigint outside the value model's integer range");
  }
  if (v instanceof Uint8Array) {
    const base64 = base64FromBytes(v);
    return U8_TYPED in v ? { $: "ta", k: "u8", v: base64 } : { $: "b", v: base64 };
  }
  for (const [ctor, kind] of TYPED_ARRAY_KINDS) {
    if (v instanceof ctor) {
      const ta = v as unknown as { buffer: ArrayBuffer; byteOffset: number; byteLength: number };
      return { $: "ta", k: kind, v: base64FromBytes(new Uint8Array(ta.buffer, ta.byteOffset, ta.byteLength)) };
    }
  }
  if (Array.isArray(v)) return v.map(encodeValue);
  if (isRef(v)) return { $: "ref", k: v.kind, id: v.id };
  if (isVariant(v)) {
    return v.payload === undefined ? { $: "var", t: v.tag } : { $: "var", t: v.tag, p: encodeValue(v.payload) };
  }
  const obj = v as { [key: string]: SfValue };
  if (Object.prototype.hasOwnProperty.call(obj, "$")) {
    return { $: "m", v: Object.keys(obj).map((k) => [k, encodeValue(obj[k])]) };
  }
  const out: { [key: string]: unknown } = {};
  for (const key of Object.keys(obj)) {
    out[key] = encodeValue(obj[key]);
  }
  return out;
}
