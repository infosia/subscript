// corpus: reject/r135-cstruct-align-below-natural
// purpose: Rejects a value-class alignment below its natural alignment.
// exercises: CStruct-alignment, natural-alignment
// questions: R33
// tsc: accepts
// expected-error: the requested alignment is below the natural alignment
@CStruct({ align: 2 })
class InvalidAlignment {
  value: f32 = 0.0;
}

export function main(): void {}
