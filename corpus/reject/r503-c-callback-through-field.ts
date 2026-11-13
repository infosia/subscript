// corpus: reject/r503-c-callback-through-field
// purpose: C callback storage cannot hold a capture.
// exercises: C-callback, field
// questions: compiler.md §274 rules 5 and 7, collisions.md C5
// tsc: accepts
// expected-error: S100 at line 8
class Holder { constructor(public cb: SubLogCallback) {} }
export function main():void { const k:i32=7; const h = new Holder((m,u1,u2):void=>{print(`${k}`);}); new SubCallbackInfo(h.cb,null,null); }
