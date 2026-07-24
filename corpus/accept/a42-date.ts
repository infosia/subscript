// corpus: accept/a42-date
// purpose: Exercises the accepted Date subset of stdlib.md §3:
//          construction via new Date(ms) and Date.UTC (trailing-argument
//          defaults, ECMA month/day carry arithmetic, the MakeFullYear
//          two-digit-year mapping), every UTC accessor, getTime,
//          toISOString zero-padding and year bounds 0000/9999, pre-1970
//          times, the TimeClip boundary values, and a Date stored in a
//          class field and a Date[] element.
// exercises: date-intrinsics, date-accessors, date-iso, q14-formatting
// questions: Q14, Q20

class Stamp {
  at: Date;
  constructor(at: Date) {
    this.at = at;
  }
}

export function main(): void {
  // Construction via Date.UTC; every accessor.
  const t0: i64 = Date.UTC(2020, 5, 15, 12, 34, 56, 789);
  const d0: Date = new Date(t0);
  print(`ms ${d0.getTime()}`);
  print(`year ${d0.getUTCFullYear()}`);
  print(`month ${d0.getUTCMonth()}`);
  print(`date ${d0.getUTCDate()}`);
  print(`day ${d0.getUTCDay()}`);
  print(`hours ${d0.getUTCHours()}`);
  print(`minutes ${d0.getUTCMinutes()}`);
  print(`seconds ${d0.getUTCSeconds()}`);
  print(`millis ${d0.getUTCMilliseconds()}`);
  print(`iso ${d0.toISOString()}`);
  // The epoch: day 0, a Thursday (getUTCDay 4).
  const epoch: Date = new Date(0);
  print(`epoch ${epoch.toISOString()} ${epoch.getUTCDay()}`);
  // Date.UTC trailing-argument defaults: day 1, time components 0.
  print(`defaults ${new Date(Date.UTC(2020, 5)).toISOString()}`);
  // 400-rule leap days exist.
  print(`leap2000 ${new Date(Date.UTC(2000, 1, 29)).toISOString()}`);
  print(`leap2400 ${new Date(Date.UTC(2400, 1, 29)).toISOString()}`);
  // Century non-leap years: day 29 carries to March 1 (ECMA MakeDay).
  print(`carry1900 ${new Date(Date.UTC(1900, 1, 29)).toISOString()}`);
  print(`carry2100 ${new Date(Date.UTC(2100, 1, 29)).toISOString()}`);
  // Month 13 carries into the next year; day 0 into the previous month.
  print(`carrym ${new Date(Date.UTC(2020, 13, 1)).toISOString()}`);
  print(`carryd ${new Date(Date.UTC(2020, 2, 0)).toISOString()}`);
  // ECMA MakeFullYear: years 0–99 map to 1900+year.
  print(`y2digit ${new Date(Date.UTC(7, 0, 1)).getUTCFullYear()}`);
  // Pre-1970: -1 ms is the last millisecond of 1969 (a Wednesday).
  const before: Date = new Date(-1);
  print(`before ${before.toISOString()} ${before.getUTCDay()}`);
  // toISOString zero-pads years to four digits.
  print(`pad ${new Date(-61940937600000).toISOString()}`);
  // The toISOString year bounds, 0000 and 9999.
  print(`y0 ${new Date(-62167219200000).toISOString()}`);
  print(`y9999 ${new Date(253402300799999).toISOString()}`);
  // The TimeClip boundary: exactly ±8640000000000000 ms is valid.
  const max: Date = new Date(8640000000000000);
  const min: Date = new Date(-8640000000000000);
  print(`max ${max.getTime()} ${max.getUTCFullYear()} ${max.getUTCMonth()} ${max.getUTCDate()} ${max.getUTCDay()}`);
  print(`min ${min.getTime()} ${min.getUTCFullYear()} ${min.getUTCMonth()} ${min.getUTCDate()} ${min.getUTCDay()}`);
  // A Date in a class field and a Date[] element, end to end.
  const stamp: Stamp = new Stamp(d0);
  print(`field ${stamp.at.toISOString()}`);
  const log: Date[] = [epoch, stamp.at];
  print(`elem ${log[1].getTime()} ${log.length}`);
}
