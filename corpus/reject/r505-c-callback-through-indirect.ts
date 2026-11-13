// corpus: reject/r505-c-callback-through-indirect
// purpose: C callback storage cannot hold a capture.
// exercises: C-callback, indirect
// questions: compiler.md §274 rules 5 and 7, collisions.md C5
// tsc: accepts
// expected-error: S100 at line 8
function install(cb:SubLogCallback):void { new SubCallbackInfo(cb,null,null); }
export function main():void { const k:i32=7; const g:(cb:SubLogCallback)=>void=install; g((m,u1,u2):void=>{print(`${k}`);}); }
