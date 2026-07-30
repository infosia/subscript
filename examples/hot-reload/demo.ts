// Run ./run.sh, then edit editableResult's return value. The next line of
// output uses the new body while `run` keeps increasing, proving that the
// module state survived the swap.
//
// To see a refused reload, add or rename a field on DemoMarker (and update its
// constructor so the edit still checks). Restore the class declaration, then
// make another function-body edit; the watch session will accept it.

class DemoMarker {
  label: string;

  constructor(label: string) {
    this.label = label;
  }
}

let run: i32 = 0;
let marker: DemoMarker = new DemoMarker("hot reload");

function editableResult(): i32 {
  return 10;
}

export function main(): void {
  run += 1;
  print(`${marker.label}: run ${run}, editable result ${editableResult()}`);
}
