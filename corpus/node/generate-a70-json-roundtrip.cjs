// Generates corpus/accept/a70-json-roundtrip.expected.
// Required oracle: node v24.18.0.

const values = [
  -8,
  250,
  -1600,
  65000,
  -2000000000,
  4000000000,
  -9007199254740991,
  9007199254740991,
  1.5,
  0.000001,
  -0,
  true,
  false,
  "\u0000\u0001\u0002\u0003\u0004\u0005\u0006\u0007\u0008\u0009\u000a\u000b\u000c\u000d\u000e\u000f" +
    "\u0010\u0011\u0012\u0013\u0014\u0015\u0016\u0017\u0018\u0019\u001a\u001b\u001c\u001d\u001e\u001f" +
    " \"/\\\u007f\u0080\u2028\u2029",
  [[1, 2], [], [-3, 4]],
  [-2, 0, 7],
  { x: 9, ready: true },
  { name: "Ada", age: 37, active: false },
  { name: "Ada", age: 37, active: false },
  null,
];

const output = values
  .map((value) => JSON.stringify(JSON.parse(JSON.stringify(value))))
  .join("\n") + "\n";
process.stdout.write(output);
