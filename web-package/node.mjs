// wasm-pack's Node target initializes synchronously on require. Present the
// same asynchronous default initializer as the browser package.
import bindings from '../pkg-node/ries_rs.js';

export default async function init() { return bindings.__wasm; }
export function initSync() { return bindings.__wasm; }
export const { SearchOptions, WasmMatch, search, listPresets, list_presets, version } = bindings;
export const initRuntime = bindings.init;
export { initRuntime as init };
