"use strict";

const ambient = {
  print(message) {
    process.stdout.write(`${message}\n`);
  },
};

Object.assign(globalThis, ambient);
module.exports = Object.freeze(Object.keys(ambient));
