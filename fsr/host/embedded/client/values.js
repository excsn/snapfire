const REF_MARK = Symbol.for("sf.ref");
const VARIANT_MARK = Symbol.for("sf.variant");
const DOUBLE_MARK = Symbol.for("sf.double");
export function ref(kind, id) {
    const v = {
        kind,
        id
    };
    v[REF_MARK] = true;
    return Object.freeze(v);
}
export const actionRef = (id)=>ref("action", id);
export const moduleRef = (id)=>ref("module", id);
export function variant(tag, payload) {
    const v = payload === undefined ? {
        tag
    } : {
        tag,
        payload
    };
    v[VARIANT_MARK] = true;
    return Object.freeze(v);
}
export function f64(value) {
    const v = {
        value
    };
    v[DOUBLE_MARK] = true;
    return Object.freeze(v);
}
export function isRef(v) {
    return typeof v === "object" && v !== null && v[REF_MARK] === true;
}
export function isVariant(v) {
    return typeof v === "object" && v !== null && v[VARIANT_MARK] === true;
}
export function isDouble(v) {
    return typeof v === "object" && v !== null && v[DOUBLE_MARK] === true;
}
const TYPED_ARRAYS = {
    i8: Int8Array,
    u8: Uint8Array,
    i16: Int16Array,
    u16: Uint16Array,
    i32: Int32Array,
    u32: Uint32Array,
    i64: BigInt64Array,
    u64: BigUint64Array,
    f32: Float32Array,
    f64: Float64Array
};
function bytesFromBase64(b64) {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for(let i = 0; i < bin.length; i++){
        bytes[i] = bin.charCodeAt(i);
    }
    return bytes;
}
function decodeFloat(v) {
    if (v === "nan") return NaN;
    if (v === "inf") return Infinity;
    if (v === "-inf") return -Infinity;
    return v;
}
function decodeTagged(obj) {
    const tag = obj["$"];
    switch(tag){
        case "i":
        case "u":
            {
                const big = BigInt(obj["v"]);
                return big >= BigInt(Number.MIN_SAFE_INTEGER) && big <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(big) : big;
            }
        case "f":
        case "f32":
            return decodeFloat(obj["v"]);
        case "b":
            return bytesFromBase64(obj["v"]);
        case "ta":
            {
                const ctor = TYPED_ARRAYS[obj["k"]];
                if (!ctor) throw new Error(`unknown typed array kind: ${obj["k"]}`);
                const bytes = bytesFromBase64(obj["v"]);
                const array = new ctor(bytes.buffer);
                if (obj["k"] === "u8") Object.defineProperty(array, U8_TYPED, {
                    value: true
                });
                return array;
            }
        case "m":
            {
                const out = {};
                for (const [k, v] of obj["v"]){
                    out[k] = decodeValue(v);
                }
                return out;
            }
        case "var":
            {
                const payload = obj["p"];
                return payload === undefined ? variant(obj["t"]) : variant(obj["t"], decodeValue(payload));
            }
        case "ref":
            return ref(obj["k"], obj["id"]);
        default:
            throw new Error(`unknown value tag: ${tag}`);
    }
}
export function decodeValue(json) {
    if (json === null || typeof json === "boolean" || typeof json === "number" || typeof json === "string") {
        return json;
    }
    if (Array.isArray(json)) {
        return json.map(decodeValue);
    }
    const obj = json;
    if (typeof obj["$"] === "string") {
        return decodeTagged(obj);
    }
    const out = {};
    for (const key of Object.keys(obj)){
        out[key] = decodeValue(obj[key]);
    }
    return out;
}
const U8_TYPED = Symbol("sf.u8");
const TYPED_ARRAY_KINDS = [
    [
        Int8Array,
        "i8"
    ],
    [
        Int16Array,
        "i16"
    ],
    [
        Uint16Array,
        "u16"
    ],
    [
        Int32Array,
        "i32"
    ],
    [
        Uint32Array,
        "u32"
    ],
    [
        BigInt64Array,
        "i64"
    ],
    [
        BigUint64Array,
        "u64"
    ],
    [
        Float32Array,
        "f32"
    ],
    [
        Float64Array,
        "f64"
    ]
];
function base64FromBytes(bytes) {
    let bin = "";
    for(let i = 0; i < bytes.length; i++)bin += String.fromCharCode(bytes[i]);
    return btoa(bin);
}
const I128_MAX = (1n << 127n) - 1n;
const I128_MIN = -(1n << 127n);
const U128_MAX = (1n << 128n) - 1n;
export function encodeValue(v) {
    if (v === null || typeof v === "boolean" || typeof v === "string") return v;
    if (typeof v === "number") {
        if (Number.isNaN(v)) return {
            $: "f",
            v: "nan"
        };
        if (v === Infinity) return {
            $: "f",
            v: "inf"
        };
        if (v === -Infinity) return {
            $: "f",
            v: "-inf"
        };
        return v;
    }
    if (typeof v === "bigint") {
        if (v >= I128_MIN && v <= I128_MAX) return {
            $: "i",
            v: v.toString()
        };
        if (v > I128_MAX && v <= U128_MAX) return {
            $: "u",
            v: v.toString()
        };
        throw new Error("bigint outside the value model's integer range");
    }
    if (v instanceof Uint8Array) {
        const base64 = base64FromBytes(v);
        return U8_TYPED in v ? {
            $: "ta",
            k: "u8",
            v: base64
        } : {
            $: "b",
            v: base64
        };
    }
    for (const [ctor, kind] of TYPED_ARRAY_KINDS){
        if (v instanceof ctor) {
            const ta = v;
            return {
                $: "ta",
                k: kind,
                v: base64FromBytes(new Uint8Array(ta.buffer, ta.byteOffset, ta.byteLength))
            };
        }
    }
    if (Array.isArray(v)) return v.map(encodeValue);
    if (isDouble(v)) {
        const n = v.value;
        if (Number.isNaN(n)) return {
            $: "f",
            v: "nan"
        };
        if (n === Infinity) return {
            $: "f",
            v: "inf"
        };
        if (n === -Infinity) return {
            $: "f",
            v: "-inf"
        };
        return {
            $: "f",
            v: n
        };
    }
    if (isRef(v)) return {
        $: "ref",
        k: v.kind,
        id: v.id
    };
    if (isVariant(v)) {
        return v.payload === undefined ? {
            $: "var",
            t: v.tag
        } : {
            $: "var",
            t: v.tag,
            p: encodeValue(v.payload)
        };
    }
    const obj = v;
    if (Object.prototype.hasOwnProperty.call(obj, "$")) {
        return {
            $: "m",
            v: Object.keys(obj).map((k)=>[
                    k,
                    encodeValue(obj[k])
                ])
        };
    }
    const out = {};
    for (const key of Object.keys(obj)){
        out[key] = encodeValue(obj[key]);
    }
    return out;
}
//# sourceMappingURL=values.js.map
