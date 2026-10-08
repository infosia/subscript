"use strict";

const ambient = {
  // Node supplies a completed wait; collection has no observable output.
  Context: {
    suspend() { return Promise.resolve(); },
    collect() {},
  },
  print(message) {
    process.stdout.write(`${message}\n`);
  },
};

Object.assign(globalThis, ambient);
module.exports = Object.freeze(Object.keys(ambient));
