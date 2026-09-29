// corpus: accept/a297-async-export-site-order/main
// purpose: Pins entry export-site order across modules and aliases.
// exercises: async-roots, re-exports, input-order
// questions: compiler.md §129, Q34, C8
// tsc: accepts; js-comparable: no C8: The corpus runner starts async exports differently.
export { root as main, root as start, zeta } from "./zlib";
export { beta } from "./alib";
export { alpha, zeta as again } from "./zlib";
