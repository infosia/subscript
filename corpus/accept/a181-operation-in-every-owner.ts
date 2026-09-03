// corpus: accept/a181-operation-in-every-owner
// purpose: Runs operation-table calls from every expression owner.
// observable: Each owner prints one hand-checked value.
// exercises: parameter-default, lambda, field-initializer, static-initializer, descriptor-default, module-initializer, generic-instance, async
// questions: §83
// tsc: accepts; js-comparable: no Q33: The Descriptor decorator has no JavaScript shim.

const moduleValue: f64 = Math.pow(2.0, 3.0);
print(`top-level:${Math.cos(0.0)}`);

function scale(value: f64, factor: f64 = Math.max(2.0, 3.0)): f64 {
  return value * factor;
}

class OwnerBox {
  fieldValue: f64 = Math.floor(4.9);
  constructorValue: f64;
  static staticValue: f64 = Math.ceil(5.1);

  constructor(value: f64 = Math.abs(-7.0)) {
    this.constructorValue = value;
  }

  methodDefault(value: f64 = Math.min(8.0, 6.0)): f64 {
    return value;
  }
}

@Descriptor
class OwnerDescriptor {
  value?: f64 = Math.trunc(9.8);
}

function genericValue<T>(value: T): f64 {
  return Math.sqrt(121.0);
}

async function asyncValue(): Promise<f64> {
  return Math.sign(-4.0);
}

export async function main(): Promise<void> {
  print(`free-default:${scale(7.0)}`);

  const box: OwnerBox = new OwnerBox();
  print(`method-default:${box.methodDefault()}`);
  print(`constructor-default:${box.constructorValue}`);

  const lambda: () => f64 = (): f64 => Math.round(10.6);
  print(`lambda-body:${lambda()}`);
  print(`field-initializer:${box.fieldValue}`);
  print(`static-initializer:${OwnerBox.staticValue}`);

  const descriptor: OwnerDescriptor = {};
  print(`descriptor-default:${descriptor.value}`);
  print(`module-initializer:${moduleValue}`);
  print(`generic-instance:${genericValue<i32>(1)}`);
  print(`async-body:${await asyncValue()}`);
}
