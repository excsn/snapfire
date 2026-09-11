import { decodeValue } from "./values.js";
export function decodeNode(row) {
    const arr = row;
    switch(arr[0]){
        case "t":
            return {
                kind: "text",
                text: arr[1]
            };
        case "r":
            return {
                kind: "raw",
                html: arr[1]
            };
        case "q":
            return {
                kind: "seq",
                children: arr[1].map(decodeNode)
            };
        case "c":
            {
                const body = arr[1];
                return {
                    kind: "client",
                    module: body["m"],
                    props: decodeValue(body["p"]),
                    children: (body["ch"] ?? []).map(decodeNode),
                    ssr: body["s"] == null ? null : decodeNode(body["s"])
                };
            }
        case "p":
            return {
                kind: "pending",
                slot: arr[1],
                fallback: decodeNode(arr[2])
            };
        default:
            throw new Error(`unknown node row kind: ${arr[0]}`);
    }
}
export function parseRow(line) {
    const tag = line[0];
    switch(tag){
        case "V":
            {
                const v = JSON.parse(line.slice(2));
                return {
                    tag,
                    format: v.fmt,
                    encoding: v.enc
                };
            }
        case "N":
            return {
                tag,
                tree: decodeNode(JSON.parse(line.slice(2)))
            };
        case "G":
            return {
                tag,
                segments: JSON.parse(line.slice(2))
            };
        case "H":
            return {
                tag,
                head: JSON.parse(line.slice(2))
            };
        case "T":
            return {
                tag,
                seed: decodeValue(JSON.parse(line.slice(2)))
            };
        case "L":
            return {
                tag,
                locale: JSON.parse(line.slice(2))
            };
        case "E":
            return {
                tag,
                entry: JSON.parse(line.slice(2))
            };
        case "C":
            return {
                tag,
                styles: JSON.parse(line.slice(2))
            };
        case "D":
            return {
                tag,
                catalog: JSON.parse(line.slice(2))
            };
        case "S":
            {
                const gap = line.indexOf(" ", 2);
                return {
                    tag,
                    slot: Number(line.slice(2, gap)),
                    node: decodeNode(JSON.parse(line.slice(gap + 1)))
                };
            }
        default:
            throw new Error(`unknown payload row tag: ${tag}`);
    }
}
export async function* linesOf(res) {
    const body = res.body;
    if (!body) {
        for (const line of (await res.text()).split("\n")){
            if (line.length > 0) yield line;
        }
        return;
    }
    const reader = body.getReader();
    const decoder = new TextDecoder();
    let carry = "";
    for(;;){
        const { done, value } = await reader.read();
        carry += done ? decoder.decode() : decoder.decode(value, {
            stream: true
        });
        let cut = carry.indexOf("\n");
        while(cut !== -1){
            const line = carry.slice(0, cut);
            carry = carry.slice(cut + 1);
            if (line.length > 0) yield line;
            cut = carry.indexOf("\n");
        }
        if (done) break;
    }
    if (carry.length > 0) yield carry;
}
export function parsePayload(text) {
    let format = 0;
    let encoding = "";
    let tree = null;
    let segments = null;
    const resolutions = [];
    const heads = [];
    const seeds = [];
    let locale = null;
    let catalog = null;
    let entry = null;
    let styles = [];
    for (const line of text.split("\n")){
        if (line.length === 0) continue;
        const row = parseRow(line);
        switch(row.tag){
            case "V":
                format = row.format;
                encoding = row.encoding;
                break;
            case "N":
                tree = row.tree;
                break;
            case "G":
                segments = row.segments;
                break;
            case "H":
                heads.push(row.head);
                break;
            case "T":
                seeds.push(row.seed);
                break;
            case "L":
                locale = row.locale;
                break;
            case "E":
                entry = row.entry;
                break;
            case "C":
                styles = row.styles;
                break;
            case "D":
                catalog = row.catalog;
                break;
            case "S":
                resolutions.push({
                    slot: row.slot,
                    node: row.node
                });
                break;
        }
    }
    if (tree === null) throw new Error("payload has no N row");
    return {
        format,
        encoding,
        tree,
        segments,
        heads,
        seeds,
        locale,
        catalog,
        entry,
        styles,
        resolutions
    };
}
//# sourceMappingURL=reader.js.map
