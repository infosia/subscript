// corpus: accept/a267-error-classes-and-uri
// purpose: Error-family tags, formatting, and URI transformations (stdlib.md §19).
// exercises: exceptions, string
// questions: Q9, Q5
// tsc: accepts; js-comparable: yes
function encode(s: string): void {
  print(encodeURIComponent(s));
  print(encodeURI(s));
}
function decode(s: string): void {
  print(decodeURIComponent(s));
  print(decodeURI(s));
}
function malformed(s: string): void {
  try { decodeURIComponent(s); } catch (e) {
    if (e instanceof URIError) { print(`${e instanceof URIError} ${e.name}`); }
  }
  try { decodeURI(s); } catch (e) {
    if (e instanceof URIError) { print(`${e instanceof URIError} ${e.name}`); }
  }
}
export function main(): void {
  const e0: Error = new Error("m");
  print(`${e0.name} ${e0.message} ${e0.toString()} ${e0 instanceof Error}`);
  print(`${e0 instanceof Error} ${e0 instanceof SyntaxError} ${e0 instanceof TypeError} ${e0 instanceof RangeError} ${e0 instanceof ReferenceError} ${e0 instanceof EvalError} ${e0 instanceof URIError}`);
  const e1: SyntaxError = new SyntaxError("m");
  print(`${e1.name} ${e1.message} ${e1.toString()} ${e1 instanceof Error}`);
  print(`${e1 instanceof Error} ${e1 instanceof SyntaxError} ${e1 instanceof TypeError} ${e1 instanceof RangeError} ${e1 instanceof ReferenceError} ${e1 instanceof EvalError} ${e1 instanceof URIError}`);
  const e2: TypeError = new TypeError("m");
  print(`${e2.name} ${e2.message} ${e2.toString()} ${e2 instanceof Error}`);
  print(`${e2 instanceof Error} ${e2 instanceof SyntaxError} ${e2 instanceof TypeError} ${e2 instanceof RangeError} ${e2 instanceof ReferenceError} ${e2 instanceof EvalError} ${e2 instanceof URIError}`);
  const e3: RangeError = new RangeError("m");
  print(`${e3.name} ${e3.message} ${e3.toString()} ${e3 instanceof Error}`);
  print(`${e3 instanceof Error} ${e3 instanceof SyntaxError} ${e3 instanceof TypeError} ${e3 instanceof RangeError} ${e3 instanceof ReferenceError} ${e3 instanceof EvalError} ${e3 instanceof URIError}`);
  const e4: ReferenceError = new ReferenceError("m");
  print(`${e4.name} ${e4.message} ${e4.toString()} ${e4 instanceof Error}`);
  print(`${e4 instanceof Error} ${e4 instanceof SyntaxError} ${e4 instanceof TypeError} ${e4 instanceof RangeError} ${e4 instanceof ReferenceError} ${e4 instanceof EvalError} ${e4 instanceof URIError}`);
  const e5: EvalError = new EvalError("m");
  print(`${e5.name} ${e5.message} ${e5.toString()} ${e5 instanceof Error}`);
  print(`${e5 instanceof Error} ${e5 instanceof SyntaxError} ${e5 instanceof TypeError} ${e5 instanceof RangeError} ${e5 instanceof ReferenceError} ${e5 instanceof EvalError} ${e5 instanceof URIError}`);
  const e6: URIError = new URIError("m");
  print(`${e6.name} ${e6.message} ${e6.toString()} ${e6 instanceof Error}`);
  print(`${e6 instanceof Error} ${e6 instanceof SyntaxError} ${e6 instanceof TypeError} ${e6 instanceof RangeError} ${e6 instanceof ReferenceError} ${e6 instanceof EvalError} ${e6 instanceof URIError}`);
  e0.name = "";
  e0.message = "x";
  print(e0.toString());
  e0.name = "N";
  e0.message = "";
  print(e0.toString());
  encode("a b");
  encode("héllo");
  encode("😀");
  encode("a&b=c/d?e#f");
  encode("-_.!~*'()");
  encode(";,/?:@&=+$#");
  encode("");
  encode("%");
  encode("\u2028");
  decode("a%20b");
  decode("%E2%82%AC");
  decode("%F0%9F%98%80");
  decode("%3B%2F%3F");
  malformed("%");
  malformed("%zz");
  malformed("%C3");
  malformed("%ED%A0%80");
  malformed("%C0%80");
  malformed("%FF");
}
