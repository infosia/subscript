// corpus: accept/a510-module-paths/main
// purpose: Two modules with one base name in two directories stay two modules, and each import resolves against its importer.
// exercises: module-import, namespace-import, re-export, module-initializer, class, enum
// questions: compiler.md §354 rules 1, 2, and 9, §137
// tsc: accepts; js-comparable: yes
import { inner, OtherKind } from "./a/inner";
import { label as labelA, Shape as ShapeA, Kind as KindA, count as countA } from "./a/util";
import { label as labelB, Shape as ShapeB, Kind as KindB, count as countB } from "./b/util";
import * as b from "./b/util";
print("main init");
export function main(): void {
  print(`${labelA()} ${labelB()} ${b.label()}`);
  print(`${new ShapeA(4).area()} ${new ShapeB(4).area()} ${b.count}`);
  print(`${KindA.High} ${KindB.High} ${OtherKind.Low} ${b.Kind.Low}`);
  print(`${countA} ${countB}`);
  print(inner());
}
