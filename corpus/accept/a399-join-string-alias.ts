// corpus: accept/a399-join-string-alias
// purpose: An array of a string-literal alias takes `join`, an array template, and the array methods of an enum array.
// exercises: array-join, array-map, array-filter, array-search, template-interpolation, string-alias, CEnum, generic-function
// questions: compiler.md §216
// tsc: accepts; js-comparable: yes
type Mode = "on" | "off";
type Same = Mode;
type Wire = CEnum<{ low: 3; high: 7; }>;

let receiverCalls: i32 = 0;

function modes(): Mode[] {
  receiverCalls += 1;
  print(`receiver ${receiverCalls}`);
  return ["off", "on"];
}

function joined<T>(values: T[]): string {
  return values.join("+");
}

class Panel {
  states: Mode[] = ["on"];
  describe(): string {
    return this.states.join(" ");
  }
}

export function main(): void {
  const ms: Mode[] = ["on", "off", "on"];
  print(ms.join("/"));
  print(ms.join());
  print(ms.join(""));

  const none: Mode[] = [];
  print(`[${none.join(",")}]`);
  print(`[${none}]`);

  print(`${ms}`);
  print(`modes: ${ms}.`);

  print(modes().join("-"));
  print(`${modes()}`);
  print(`${receiverCalls}`);

  const same: Same[] = ["off"];
  print(same.join(","));

  const wires: Wire[] = ["high", "low"];
  print(wires.join(":"));
  print(`${wires}`);

  print(joined<Mode>(ms));
  print(new Panel().describe());

  print(ms.map((x: Mode): string => `${x}`).join("/"));
  print(ms.filter((x: Mode): boolean => x === "on").join(","));
  print(`${ms.includes("off")} ${none.includes("off")}`);
  print(`${ms.indexOf("off")} ${none.indexOf("off")} ${ms.lastIndexOf("on")}`);
  print(`${wires.includes("low")} ${wires.indexOf("low")}`);
  print(wires.map((x: Wire): string => `<${x}>`).join(""));
  print(wires.filter((x: Wire): boolean => x !== "low").join(","));
}
