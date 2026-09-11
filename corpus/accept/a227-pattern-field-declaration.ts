// corpus: accept/a227-pattern-field-declaration
// purpose: Binds class fields by name in a declaration, with and without renaming.
// observable: a getter runs, and a value-class field copies.
// exercises: binding-pattern, class-field, renamed-field, getter, value-class
// questions: Q30, compiler section 107
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
@CStruct class Extent {
  width: i32;
  height: i32;
  constructor(width: i32, height: i32) {
    this.width = width;
    this.height = height;
  }
}

class Frame {
  extent: Extent;
  private ticks: i32 = 0;
  constructor(extent: Extent) {
    this.extent = extent;
  }
  get counted(): i32 {
    print("counted");
    return this.ticks + 7;
  }
}

export function main(): void {
  const frame: Frame = new Frame(new Extent(3, 4));
  const { extent } = frame;
  const { width, height: tall } = extent;
  print(`${width} ${tall}`);
  const { counted } = frame;
  print(`${counted}`);
  const { extent: copied } = frame;
  print(`${copied.width}`);
  const {} = frame;
  const extents: Extent[] = [new Extent(5, 6)];
  const [value] = extents;
  print(`${value.height}`);
  const frames: Frame[] = [frame];
  const [reference] = frames;
  print(`${reference.extent.width}`);
}
