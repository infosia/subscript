// corpus: accept/a266-date-es2022-additions
// purpose: Date copies and UTC formatting, default toFixed, and Infinity (stdlib.md §3.1, §11.4a).
// exercises: date, number
// questions: Q20, Q25
// tsc: accepts; js-comparable: yes
export function main(): void {
  print(`Infinity ${Infinity} ${-Infinity} ${Infinity === Number.POSITIVE_INFINITY} ${1.0 / Infinity}`);
  print(`toFixed() ${(1.5).toFixed()} ${(2.5).toFixed()} ${(-1.5).toFixed()} ${(1234.5).toFixed()} ${(0.0).toFixed()} ${(-0.0).toFixed()} ${(1e21).toFixed()} ${NaN.toFixed()} ${(0.5).toFixed()} ${(1.4).toFixed(1)}`);
  const narrow: f32 = 1.5;
  print(`f32 ${narrow.toFixed()} ${narrow.toFixed(0)}`);
  print(`nonfinite ${Infinity.toFixed()} ${(-Infinity).toFixed()}`);
  const d: Date = new Date(1700000000123);
  print(`toJSON ${d.toJSON()} ${d.toISOString()}`);
  print(`toUTCString ${d.toUTCString()} ${new Date(0).toUTCString()} ${new Date(-1).toUTCString()} ${new Date(Date.UTC(99, 0)).toUTCString()}`);
  print(`far ${new Date(Date.UTC(10000, 0)).toUTCString()} ${new Date(Date.UTC(-1, 0)).toUTCString()} ${new Date(-62198755200000).toUTCString()}`);
  print(`year99 ${new Date(-59042995200000).toUTCString()}`);
  print(`leap ${new Date(Date.UTC(2000, 1, 29)).toUTCString()}`);
  print(`ends ${new Date(-8640000000000000).toUTCString()} ${new Date(8640000000000000).toUTCString()}`);
  const value: i64 = d.valueOf();
  print(`valueOf ${value}`);
  const copy: Date = new Date(d);
  print(`copy ${copy.getTime()}`);
  print(`UTC(year) ${Date.UTC(2020)} ${Date.UTC(2020, 0)} ${Date.UTC(99)} ${Date.UTC(1970)}`);
  print(`UTC(0) ${Date.UTC(0)} ${Date.UTC(0, 0)}`);
}
