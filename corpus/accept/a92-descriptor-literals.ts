// corpus: accept/a92-descriptor-literals
// purpose: Exercises literal construction of data-only @Descriptor reference classes.
// exercises: required/defaulted members, Q32 aliases, nesting, descriptor arrays, all-defaulted {}, arguments
// questions: Q33, C1, C7
// tsc: accepts; js-comparable: no Q33: The Descriptor decorator has no JavaScript shim.
type Mode = "fast" | "safe";

@Descriptor
class LeafDescriptor {
  value?: i32 = 7;
  mode?: Mode = "safe";
}

function nestedDefault(): LeafDescriptor {
  const leaf: LeafDescriptor = {};
  print(`nested-default=${leaf.value},${leaf.mode}`);
  return leaf;
}

@Descriptor
class AllDefaultedDescriptor {
  enabled?: boolean = true;
  label?: string = "ready";
}

@Descriptor
class RequestDescriptor {
  id!: i32;
  size?: i32 = 64;
  mode?: Mode = "safe";
  nested!: LeafDescriptor;
  fallback?: LeafDescriptor = nestedDefault();
  items!: LeafDescriptor[];
  flags?: AllDefaultedDescriptor = {};
}

function consume(request: RequestDescriptor): void {
  print(
    `arg=${request.id},${request.size},${request.mode},${request.nested.value},${request.items[0].value}`,
  );
}

export function main(): void {
  const full: RequestDescriptor = {
    id: 1,
    size: 128,
    mode: "fast",
    nested: { value: 9, mode: "fast" },
    fallback: { value: 10, mode: "fast" },
    items: [{ value: 2 }, { value: 3, mode: "fast" }],
    flags: { enabled: false, label: "custom" },
  };
  const defaulted: RequestDescriptor = {
    id: 2,
    nested: { value: 5 },
    items: [{}, { value: 6 }],
  };
  const allDefaulted: AllDefaultedDescriptor = {};

  print(`full=${full.id},${full.size},${full.mode},${full.nested.value}`);
  print(`defaulted=${defaulted.id},${defaulted.size},${defaulted.mode}`);
  print(`array=${full.items[0].value},${defaulted.items[0].value}`);
  print(`all-defaulted=${allDefaulted.enabled},${allDefaulted.label}`);

  consume({
    id: 3,
    nested: { value: 11 },
    items: [{ value: 12 }],
  });
}
