// corpus: accept/a84-for-of-bmp
// purpose: P24 keeps BMP string for-of on the allocation-free static table.
// observable: BMP code points print in Unicode scalar order.
// exercises: for-of-string, bmp-code-points, p24-static-table

export function main(): void {
  const text: string = "Aé漢かな";
  for (const value of text) {
    print(`bmp:${value}`);
  }
}
