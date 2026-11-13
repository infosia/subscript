// corpus: reject/r506-c-callback-through-array
// purpose: C callback storage cannot hold a capture.
// exercises: C-callback, array
// questions: compiler.md §274 rules 5 and 7, collisions.md C5
// tsc: accepts
// expected-error: S100 at line 8

export function main():void { const k:i32=7; const arr:SubLogCallback[]=[]; arr.push((m,u1,u2):void=>{print(`${k}`);}); new SubCallbackInfo(arr[0],null,null); }
