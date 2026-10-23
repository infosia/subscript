// A fixed set of names. In TypeScript you can write a string enum or a
// union of string literals. In subscript you write the union, and its
// set of names is closed. This script runs a traffic light.

// `Light` has exactly three values. `enum Light { Red = "red" }` does not
// compile, because an enum member must have an integer value.
type Light = "red" | "yellow" | "green";

// A `switch` without `default` must have one `case` for each value of
// `Light`. If you add a value to `Light` later, this function does not
// compile until you add its `case`. Because each value has a `case`,
// the function needs no `return` after the `switch`.
function next(light: Light): Light {
  switch (light) {
    case "red":
      return "green";
    case "green":
      return "yellow";
    case "yellow":
      return "red";
  }
}

function seconds(light: Light): i32 {
  switch (light) {
    case "red":
      return 30;
    case "green":
      return 25;
    case "yellow":
      return 5;
  }
}

export function main(): void {
  let light: Light = "red";
  for (let step: i32 = 0; step < 4; step += 1) {
    // A template string prints the name of the value.
    print(`${light} for ${seconds(light)} s`);
    light = next(light);
  }

  // `==` compares a value with a name in the union.
  print(`go: ${light == "green"}`);

  // A `Light` is not a `string`. `const text: string = light;` does not
  // compile, and `const other: Light = "blue";` does not compile. tsc
  // accepts the first line and rejects the second.
}
