// Generates corpus/accept/a71-json-parse.expected.
// Required oracle: node v24.18.0.
//
// The value-bearing lines are measured from JSON.parse below. The `ok`
// lines are contract-derived: JavaScript has no static T validation, so
// only the malformed input throws there.

const success = JSON.parse('{"name":"demo","count":3}');
const duplicate = JSON.parse(
  '{"name":"first","name":"last","count":4}',
);
const negativeZero = JSON.parse("-0");
const beyondSafe = JSON.parse("9007199254740993");

const lines = [
  "success-ok=true", // compiler.md §115.7: no exception
  `success=${success.name}:${success.count}`,
  "malformed-ok=false", // compiler.md §115.7: SyntaxError
  "mismatch-ok=false", // static Config validation contract
  "missing-ok=false", // static Config validation contract
  "array-mismatch-ok=false", // static i32[] validation contract
  `duplicate=${duplicate.name}`,
  `negative-zero-reciprocal=${1 / negativeZero}`,
  `beyond-safe=${beyondSafe}`,
  // Contract-derived: JavaScript has no i64 target; subscript parses the
  // integer token text exactly and 9007199254740993 fits i64.
  "beyond-safe-i64=9007199254740993",
  "overflow-ok=false", // Q28 rejects a non-finite f64 input with TypeError
  "narrow-overflow-ok=false", // Q28 rejects non-finite f32 fields
  "wide-overflow-ok=false", // Q28 rejects non-finite f64 fields
];

process.stdout.write(lines.join("\n") + "\n");
