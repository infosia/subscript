// Strings. A subscript string holds UTF-8 bytes, because C code reads and
// writes UTF-8. A JavaScript string holds UTF-16 units. For ASCII text the
// two give the same results. This script prints the names of two users.

export function main(): void {
  // ASCII text: one character is one byte, so the results are the same
  // as in node.
  const ada: string = "Ada";
  print(`${ada} ${ada.length} ${ada.toUpperCase()}`);

  // "é" is two bytes in UTF-8. So `length` is 5 here, and node prints 4.
  // `indexOf` also counts bytes: "é" starts at byte 3.
  const jose: string = "José";
  print(`${jose} ${jose.length} ${jose.indexOf("é")} ${jose.toUpperCase()}`);

  // `slice` takes byte positions. `slice(0, 3)` stops before "é", so it
  // gives the same text as in node. A position inside "é", such as
  // `slice(0, 4)`, stops the program.
  print(jose.slice(0, 3));

  // To count characters, step through the string with `for...of`. Each
  // step gives one character, as in node.
  let characters: i32 = 0;
  for (const character of jose) {
    print(`  ${character}`);
    characters += 1;
  }
  print(`${characters} characters, ${jose.length} bytes`);
}
