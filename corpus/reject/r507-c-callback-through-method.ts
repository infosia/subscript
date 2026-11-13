// corpus: reject/r507-c-callback-through-method
// purpose: C callback storage cannot hold a capture.
// exercises: C-callback, method
// questions: compiler.md §274 rules 5 and 7, collisions.md C5
// tsc: accepts
// expected-error: S100 at line 8
class Holder { constructor(public cb:SubLogCallback) {} install():void { new SubCallbackInfo(this.cb,null,null); } }
export function main():void { const k:i32=7; new Holder((m,u1,u2):void=>{print(`${k}`);}).install(); }
