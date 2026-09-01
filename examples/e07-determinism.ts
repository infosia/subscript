// example: e07-determinism
// teaches: Replay seeded random values, construct UTC dates from explicit milliseconds, and format numbers deterministically.
// differs-from-typescript: Q20/Q26 reject clock- and locale-dependent APIs instead of approximating their results.
// see: corpus/accept/a40-math.ts, corpus/accept/a41-math-random.ts, corpus/accept/a42-date.ts, corpus/accept/a57-number.ts, corpus/accept/a58-number-parse.ts, corpus/accept/a59-number-to-fixed.ts, corpus/reject/r16-math-variadic-max.ts, corpus/reject/r18-math-value.ts, corpus/reject/r19-date-local-accessor.ts, corpus/reject/r20-date-setter.ts, corpus/reject/r21-date-multiarg-ctor.ts, corpus/reject/r22-date-template.ts, corpus/reject/r23-date-zero-arg-ctor.ts, corpus/reject/r24-date-compare.ts, collisions.md Q14, collisions.md Q19-Q20, collisions.md Q25-Q26, stdlib.md §0.3

// Every value this program prints is fixed by the language, not by the machine
// and not by the clock. The three sections show where that comes from.
// Q12: this zero-argument void export is a host-callable script entry.
export function main(): void {
  // Section one: the seeded stream. A fresh Context replays these same three
  // values on every run and on every platform.
  // Q19: every fresh Context seeds Math.random with the fixed contract seed
  // 0x5355_4253_5245_4144; a host can reseed it for another recorded stream.
  print(`random=${Math.random()}`);
  print(`random=${Math.random()}`);
  print(`random=${Math.random()}`);

  // Section two: an instant from explicit milliseconds. date= and utc= read the
  // same instant two ways, both in UTC.
  // Q20: explicit epoch milliseconds construct an immutable UTC value
  // without reading the Context clock.
  const instant: Date = new Date(1592224496789);
  print(`date=${instant.toISOString()}`);
  print(
    `utc=${instant.getUTCFullYear()},${instant.getUTCMonth()},${instant.getUTCDate()},${instant.getUTCHours()}`,
  );

  // Section three: number text. number=1234.5678|1234.57|-0 shows the shortest
  // round-trip form, a two-digit fixed form, and negative zero.
  // Q14/Q25: interpolation and toFixed use the shared runtime formatter,
  // so neither execution tier delegates these bytes to a host libc.
  const measurement: f64 = 1234.5678;
  print(`number=${measurement}|${measurement.toFixed(2)}|${-0.0}`);

  // The two rules below state why this program reads no clock and no locale.
  // Q20/Q26: local-time access is rejected because the runtime has no
  // timezone or locale data; substituting UTC would silently change meaning.
  // Rejected alternative: instant.getFullYear() is S014; diagnostic excerpt:
  // "`getFullYear` is rejected: Local-time accessors are unavailable.";
  // corpus/reject/r19-date-local-accessor.ts pins it.

  // Q20: an implicit clock read is rejected; Date.now makes the Context-owned
  // input explicit, while this example needs no clock at all.
  // Rejected alternative: new Date() is S014; diagnostic excerpt:
  // "`new Date()` is rejected: The zero-argument constructor reads
  // nondeterministic current time."; corpus/reject/r23-date-zero-arg-ctor.ts
  // pins it.
}
