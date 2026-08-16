// corpus: reject/r134-plain-alias-entry-param
// purpose: Rejects a plain string-literal union alias in a host-callable entry parameter.
// exercises: string-literal-union-alias, exported-signature
// questions: R32, Q32
// expected-error: plain string-literal union aliases have no host ABI representation

type Level = "low" | "high";

export function configure(level: Level): void {}
