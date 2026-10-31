// corpus: accept/a399-join-string-alias
// purpose: An array of a string-literal alias takes `join`, an array template, and the array methods of an enum array.
// exercises: array-join, array-map, array-filter, array-reduce, array-sort, array-search, function-value-callback, template-interpolation, string-alias, CEnum, generic-function
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

function isOn(x: Mode): boolean {
  return x === "on";
}

function label(x: Mode): string {
  return `(${x})`;
}

function rank(x: Mode): i32 {
  return x === "off" ? 0 : 1;
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

  print(`${ms.some((x: Mode): boolean => x === "off")} ${ms.every((x: Mode): boolean => x === "off")}`);
  print(`${ms.findIndex((x: Mode): boolean => x === "off")} ${ms.findLastIndex((x: Mode): boolean => x === "on")}`);
  print(ms.flatMap((x: Mode): Mode[] => [x, x]).join(""));
  print(ms.reduce((text: string, x: Mode): string => `${text}${x};`, ""));
  print(ms.reduceRight((text: string, x: Mode): string => `${text}${x};`, ""));
  const first: Mode = "off";
  const last: Mode = ms.reduce((held: Mode, x: Mode): Mode => x === "off" ? held : x, first);
  print(`${last}`);
  const sorted: Mode[] = ms.slice();
  sorted.sort((p: Mode, q: Mode): i32 => rank(p) - rank(q));
  print(sorted.join(","));

  const test = (x: Mode): boolean => x === "off";
  print(ms.filter(test).join(","));
  print(`${ms.some(test)}`);
  print(ms.filter(isOn).map(label).join(""));
}
