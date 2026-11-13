// corpus: reject/r504-c-callback-through-return
// purpose: C callback storage cannot hold a capture.
// exercises: C-callback, return
// questions: compiler.md §274 rules 5 and 7, collisions.md C5
// tsc: accepts
// expected-error: S100 at line 7
function make():SubLogCallback { const k:i32=7; return (m,u1,u2):void=>{print(`${k}`);}; }
export function main():void { new SubCallbackInfo(make(),null,null); }
