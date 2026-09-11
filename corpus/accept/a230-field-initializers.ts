// corpus: accept/a230-field-initializers
// purpose: Accepts a class whose every field carries an initializer.
// observable: a fresh instance prints each initializer before any assignment.
// exercises: class-field, field-initializer, value-class, reference-field
// questions: compiler section 108
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
@CStruct
class Extent {
  width: i32 = 4;
  height: i32 = 3;
}

class Inner {
  value: i32 = 7;
}

class Holder {
  count: i32 = 1;
  label: string = "ready";
  inner: Inner = new Inner();
  extent: Extent = new Extent();
}

export function main(): void {
  const holder: Holder = new Holder();
  print(`${holder.count} ${holder.label} ${holder.inner.value}`);
  print(`${holder.extent.width}x${holder.extent.height}`);
}
