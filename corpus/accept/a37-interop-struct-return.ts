// corpus: accept/a37-interop-struct-return
// purpose: Reads fields of boundary value classes returned BY VALUE from foreign calls (register and sret ABI).
// exercises: interop-struct-return, by-value-aggregate-abi, boundary-value-class, foreign-call
// questions: Q13
// tsc: accepts
// §14.2 by-value boundary-struct return. A foreign function returns a
// boundary value class by value; both tiers marshal the C-ABI struct return
// (small structs in registers, larger via sret), arch-gated by §12.3a. The
// returned value class is an ordinary in-language value afterward — its
// fields are read directly. SubFuture is 8 bytes (returned in a register,
// the async-future shape); SubStats is 24 bytes (returned via sret), so this
// one entry proves both ABI return paths. subFutureMake(5).id = 5*3+1 = 16;
// subStatsMake(10) = (10, 20, 30).

export function main(): void {
  const f: SubFuture = subFutureMake(5);
  print(`${f.id}`);

  const s: SubStats = subStatsMake(10);
  print(`${s.submitted}`);
  print(`${s.completed}`);
  print(`${s.pending}`);
}
