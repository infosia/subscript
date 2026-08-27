// corpus: accept/a66-q27-map-set
// purpose: Exercises Q27 stage 4 Map.groupBy and ES2024 Set algebra,
//          pinning first-seen group order, group membership, fresh
//          results, both operand directions, and predicate outcomes.
// exercises: map-group-by, set-algebra, insertion-order, owned-results
// questions: Q27, Q24, C2, C5
// tsc: accepts; js-comparable: yes
let setOrder: string = "";

function showSet(label: string, values: Set<i32>): void {
  setOrder = "";
  values.forEach((value: i32): void => {
    if (setOrder.length > 0) {
      setOrder += ",";
    }
    setOrder += `${value}`;
  });
  print(`${label} ${setOrder}`);
}

function addAll(values: Set<i32>, a: i32, b: i32): void {
  values.add(a);
  values.add(b);
}

export function main(): void {
  const items: i32[] = [1, 2, 3, 4, 5];
  const groups: Map<string, i32[]> = Map.groupBy(
    items,
    (value: i32): string => value % 2 === 0 ? "even" : "odd",
  );
  groups.forEach((values: i32[], key: string): void => {
    print(`group ${key} ${values.join(",")}`);
  });
  print(`groups ${groups.size} ${groups.has("even")} ${groups.get("even") === null}`);

  const s1: Set<i32> = new Set<i32>();
  s1.add(1);
  s1.add(2);
  s1.add(3);
  const s2: Set<i32> = new Set<i32>();
  addAll(s2, 3, 4);

  const union12: Set<i32> = s1.union(s2);
  showSet("union12", union12);
  showSet("union21", s2.union(s1));
  showSet("intersection12", s1.intersection(s2));
  showSet("intersection21", s2.intersection(s1));
  showSet("difference12", s1.difference(s2));
  showSet("difference21", s2.difference(s1));
  showSet("symmetric12", s1.symmetricDifference(s2));
  showSet("symmetric21", s2.symmetricDifference(s1));

  const wide: Set<i32> = new Set<i32>();
  wide.add(1);
  wide.add(2);
  wide.add(3);
  wide.add(4);
  const narrow: Set<i32> = new Set<i32>();
  addAll(narrow, 4, 2);
  showSet("intersectionSmall12", wide.intersection(narrow));
  showSet("intersectionSmall21", narrow.intersection(wide));

  print(`pred12 ${s1.isSubsetOf(s2)} ${s1.isSupersetOf(s2)} ${s1.isDisjointFrom(s2)}`);
  print(`pred21 ${s2.isSubsetOf(s1)} ${s2.isSupersetOf(s1)} ${s2.isDisjointFrom(s1)}`);
  const only3: Set<i32> = new Set<i32>();
  only3.add(3);
  const outside: Set<i32> = new Set<i32>();
  outside.add(9);
  print(`predTrue ${only3.isSubsetOf(s1)} ${s1.isSupersetOf(only3)} ${s1.isDisjointFrom(outside)}`);

  union12.add(9);
  print(`fresh ${union12 === s1} ${s1.has(9)} ${items.join(",")}`);
}
