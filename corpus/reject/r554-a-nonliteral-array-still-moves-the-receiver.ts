// corpus: reject/r554-a-nonliteral-array-still-moves-the-receiver
// purpose: Parameters, global assignments, and slice results stay moves.
// exercises: value-types, array-parameters, assignment, slice
// questions: compiler.md §333 rule 3
// tsc: accepts
// expected-error: S100: `W.parameter` can move a dynamic array
@ValueType
class W {
  a: i32 = 1;
  parameter(xs: W[]): void { xs.push(this); }
  assigned(): void { let xs: W[] = []; xs = keep; xs.push(this); }
  sliced(): void { const xs: W[] = keep.slice(0); xs.push(this); }
}
const keep: W[] = [];
const all: W[] = [new W()];
all[0].parameter(keep);
all[0].assigned();
all[0].sliced();
