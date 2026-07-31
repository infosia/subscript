// corpus: accept/a91-string-literal-union
// purpose: Exercises nominal closed string-literal union aliases.
// exercises: contextual members, assignment, parameters, fields, returns, arrays, equality, formatting
// questions: Q32

type IndexFormat = "uint16" | "uint32";
type TwinFormat = "uint16" | "uint32";

@CStruct
class FormatBox {
  format: IndexFormat;

  constructor(format: IndexFormat) {
    this.format = format;
  }
}

function echoFormat(format: IndexFormat): IndexFormat {
  return format;
}

function differentFormat(left: IndexFormat, right: IndexFormat): boolean {
  return left !== right;
}

export function main(): void {
  const initial: IndexFormat = "uint16";
  let assigned: IndexFormat = initial;
  assigned = "uint32";
  const same: IndexFormat = "uint32";
  const formats: IndexFormat[] = ["uint16", "uint32"];
  const box: FormatBox = new FormatBox("uint16");
  const twin: TwinFormat = "uint16";

  print(`echo=${echoFormat(initial)}`);
  print(`literal=${assigned === "uint32"}`);
  print(`same=${assigned === same}`);
  print(`different=${differentFormat(initial, assigned)}`);
  print(`array=${formats[1]}`);
  print(`field=${box.format}`);
  print(`twin=${twin}`);
}
