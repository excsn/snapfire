export type RefValue = {
	readonly kind: "action" | "module";
	readonly id: string;
};
export type VariantValue = {
	readonly tag: string;
	readonly payload?: SfValue;
};
export declare function ref(kind: "action" | "module", id: string): RefValue;
export declare const actionRef: (id: string) => RefValue;
export declare const moduleRef: (id: string) => RefValue;
export declare function variant(tag: string, payload?: SfValue): VariantValue;
export declare function isRef(v: unknown): v is RefValue;
export declare function isVariant(v: unknown): v is VariantValue;
export type SfValue = null | boolean | number | bigint | string | Uint8Array | Int8Array | Int16Array | Uint16Array | Int32Array | Uint32Array | BigInt64Array | BigUint64Array | Float32Array | Float64Array | SfValue[] | RefValue | VariantValue | {
	[key: string]: SfValue;
};
/** Decodes the JSON pair's output back into JS values. Untagged JSON passes through untouched. */
export declare function decodeValue(json: unknown): SfValue;
/** Encodes JS values into the JSON pair's tagged form for the server. JS has one number type, so an integral number arrives as an Int; Uint8Array arrives as bytes. */
export declare function encodeValue(v: SfValue): unknown;
