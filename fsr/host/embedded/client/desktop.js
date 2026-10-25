function call(method, args) {
    const held = globalThis.__sfDesktop;
    if (!held) return Promise.reject(new Error(`${method} runs only in an fsr --desktop window`));
    return held.call(method, args);
}
export async function openFile(options = {}) {
    return await call("openFile", options);
}
export async function openExternal(url) {
    await call("openExternal", {
        url
    });
}
//# sourceMappingURL=desktop.js.map
