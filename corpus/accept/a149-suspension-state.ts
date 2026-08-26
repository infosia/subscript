// corpus: accept/a149-suspension-state
// purpose: Keeps each live value in its coroutine frame across a suspension.
// exercises: await-expression-order, nested-await-labels, evaluation-order-liveness, canonical-spill-kind, async-method-receiver-state, composite-expression-state, assignment-place-state, planner-order, complete-spill-cursor, skipped-statement-spill-cursor, aggregate-operand-copy-order, Context.bytes, foreign-call-state, descriptor-literal-state, aggregate-local-state, nested-list-lambda-environment, intrinsic-operand-state, switch-discriminant-state, array-spread-state, unconditional-lambda-environment, assigned-lambda-environment, distinct-lambda-environments, lambda-environment, managed-capture, for-of-suspension, generator-resume-address
// questions: §67

let leftCalls: i32 = 0;
let rightCalls: i32 = 0;

async function left(): Promise<i32> {
  leftCalls += 1;
  print(`await:left:call=${leftCalls}`);
  await Context.suspend();
  print(`await:left:return=${leftCalls}`);
  return 11;
}

async function right(): Promise<i32> {
  rightCalls += 1;
  print(`await:right:call=${rightCalls}`);
  await Context.suspend();
  print(`await:right:return=${rightCalls}`);
  return 22;
}

function* doubled(values: i32[]): Generator<i32> {
  for (const value of values) {
    yield value * 2;
  }
}

function* odd(): Generator<i32> {
  yield 1;
  yield 3;
}

function* even(): Generator<i32> {
  yield 2;
  yield 4;
}

function resume(generator: Generator<i32>): i32 {
  return generator.next().value;
}

class SuspensionBox {
  value: i32;

  constructor(value: i32) {
    this.value = value;
  }

  add(value: i32): i32 {
    return this.value + value;
  }
}

class SuspensionPair {
  left: i32;
  right: i32;

  constructor(left: i32, right: i32) {
    this.left = left;
    this.right = right;
  }
}

class SuspensionCell {
  value: i32 = 1;
}

@CStruct
class MachineryValue {
  first: i32;
  second: i32;

  constructor(first: i32, second: i32) {
    this.first = first;
    this.second = second;
  }
}

@CStruct
class OperandValue {
  a: i32;
  b: i32;

  constructor(a: i32, b: i32) {
    this.a = a;
    this.b = b;
  }
}

class OperandHolder {
  value: OperandValue;
  fixed: FixedArray<i32, 2>;

  constructor(value: OperandValue) {
    this.value = value;
    this.fixed = [1, 2];
  }
}

class OperandPair {
  value: OperandValue;
  key: i32;

  constructor(value: OperandValue, key: i32) {
    this.value = value;
    this.key = key;
  }
}

function operandSink(value: OperandValue, key: i32): i32 {
  return value.a + value.b + key;
}

function operandFixedSink(fixed: FixedArray<i32, 2>, key: i32): i32 {
  return fixed[0] * 10 + fixed[1] + key;
}

function operandBump(holder: OperandHolder): i32 {
  holder.value = new OperandValue(99, 99);
  holder.fixed = [7, 8];
  return 1;
}

const operandIndirect: (value: OperandValue, key: i32) => i32 = operandSink;

@Descriptor
class MachineryDescriptor {
  first!: i32;
  second!: i32;
  fallback?: i32 = 7;
}

function makeBox(): SuspensionBox {
  print("method:receiver");
  return new SuspensionBox(8);
}

function makeArray(): i32[] {
  print("index:base");
  return [10, 20];
}

async function compositeA(): Promise<i32> {
  print("composite:a");
  await Context.suspend();
  return 4;
}

async function compositeB(): Promise<i32> {
  print("composite:b");
  await Context.suspend();
  return 7;
}

async function assignedValue(): Promise<i32> {
  print("assign:value");
  await Context.suspend();
  return 5;
}

async function indexValue(): Promise<i32> {
  print("index:key");
  await Context.suspend();
  return 1;
}

async function compoundValue(): Promise<i32> {
  print("compound:value");
  await Context.suspend();
  return 3;
}

async function machineryValue(label: string, value: i32): Promise<i32> {
  print(`machinery:${label}`);
  await Context.suspend();
  return value;
}

async function machineryText(label: string, value: string): Promise<string> {
  print(`machinery:${label}`);
  await Context.suspend();
  return value;
}

async function machineryArray(label: string): Promise<i32[]> {
  print(`machinery:${label}`);
  await Context.suspend();
  return [4, 5];
}

function machinerySide(label: string, value: i32): i32 {
  print(`machinery:${label}`);
  return value;
}

function machinerySideValue(label: string): MachineryValue {
  print(`machinery:${label}`);
  return new MachineryValue(1, 2);
}

function machinerySideBytes(label: string, value: u8[]): u8[] {
  print(`machinery:${label}`);
  return value;
}

function makeMachineryValue(first: i32, second: i32): MachineryValue {
  return new MachineryValue(first, second);
}

function makeMachineryFixed(first: i32, second: i32): FixedArray<i32, 2> {
  return [first, second];
}

class DeclaredBox {
  value: i32 = 0;
}

class AsyncMachine {
  base: i32;

  constructor(base: i32) {
    this.base = base;
  }

  async step(delta: i32): Promise<i32> {
    await Context.suspend();
    return this.base + delta;
  }
}

async function roundFiveValue(value: i32): Promise<i32> {
  await Context.suspend();
  return value;
}

async function roundFiveIncrement(value: i32): Promise<i32> {
  await Context.suspend();
  return value + 1;
}

async function roundFiveNullable(): Promise<DeclaredBox | null> {
  await Context.suspend();
  return null;
}

function takeDeclared(_box: DeclaredBox | null, value: i32): i32 {
  return value;
}

function applyCaptured(callback: () => i32, value: i32): i32 {
  return callback() + value;
}

async function applyAfterSuspension(
  callback: () => i32,
  tag: string,
): Promise<i32> {
  print(`rule1k:apply:before:${tag}`);
  await Context.suspend();
  const value: i32 = callback();
  print(`rule1k:apply:after:${tag}=${value}`);
  return value;
}

function* callCaptureAfterYield(callback: () => i32): Generator<i32> {
  yield 0;
  yield callback();
}

async function unreachableAfterReturnCall(): Promise<void> {
  print("rule1l:return-call:start");
  const values: i32[] = [1];
  return;
  values.push(await roundFiveValue(2));
}

function unreachableSide(value: i32): i32 {
  return value;
}

async function unreachableAfterReturnTemplate(): Promise<void> {
  print("rule1l:return-template:start");
  return;
  print(`y=${unreachableSide(1) + (await roundFiveValue(2))}`);
}

async function unreachableAfterBreak(): Promise<void> {
  print("rule1l:break:start");
  while (true) {
    break;
    const factor: i32 = 4;
    const callback = (): i32 => factor * 5;
    await Context.suspend();
    print(`${callback()}`);
  }
  print("rule1l:break:end");
}

export async function main(): Promise<void> {
  print(`await:result=${await left()},${await right()}`);
  print(`await:calls=${leftCalls},${rightCalls}`);

  const literal: i32[] = [await compositeA(), 9, await compositeB()];
  print(`array=${literal.join(",")}`);
  print(`method=${makeBox().add(await compositeA())}`);
  const pair = new SuspensionPair(await compositeA(), await compositeB());
  print(`new=${pair.left},${pair.right}`);
  print(`index=${makeArray()[await indexValue()]}`);
  const cell = new SuspensionCell();
  cell.value = await assignedValue();
  print(`field=${cell.value}`);
  const pushed: i32[] = [1];
  pushed.push(await assignedValue());
  print(`push=${pushed.join(",")}`);
  const compound: i32[] = [1, 2];
  compound[1] += await compoundValue();
  print(`compound=${compound.join(",")}`);

  let machineryIndex: i32 = 0;
  const machineryValues: i32[] = [];
  const machineryCell = new SuspensionCell();
  for (
    machineryIndex = 0;
    machineryIndex < 2;
    machineryCell.value = await machineryValue("step", 5)
  ) {
    machineryValues.push(await machineryValue("body", 7));
    machineryIndex = machineryIndex + 1;
  }
  print(`machinery:for=${machineryValues.join(",")}:${machineryCell.value}`);

  let localAssigned: i32 = 0;
  localAssigned = await machineryValue("local", 5);
  print(`machinery:local=${localAssigned}`);

  const loopFactor: i32 = 3;
  const loopLambda = (): i32 => loopFactor * 5;
  let loopIndex: i32 = 0;
  while (loopIndex < 3) {
    print(`machinery:loop=${loopLambda()}`);
    await Context.suspend();
    loopIndex = loopIndex + 1;
  }

  const assignedFactor: i32 = 3;
  let assignedLambda = (): i32 => 0;
  assignedLambda = (): i32 => assignedFactor * 2;
  await Context.suspend();
  print(`machinery:assigned=${assignedLambda()}`);

  const sharedFactor: i32 = 3;
  const outerLambda = (): i32 => sharedFactor * 2;
  {
    const sharedFactor: i32 = 50;
    const innerLambda = (): i32 => sharedFactor * 2;
    await Context.suspend();
    print(`machinery:inner=${innerLambda()}`);
  }
  await Context.suspend();
  print(`machinery:outer=${outerLambda()}`);

  const intrinsicMap = new Map<i32, string>();
  intrinsicMap.set(await machineryValue("map-set", 1), "one");
  print(`machinery:map-set=${intrinsicMap.getOr(1, "missing")}`);

  const intrinsicAddSet = new Set<i32>();
  intrinsicAddSet.add(await machineryValue("set-add", 5));
  print(`machinery:set-add=${intrinsicAddSet.has(5)}`);

  const intrinsicHasSet = new Set<i32>();
  intrinsicHasSet.add(1);
  print(`machinery:set-has=${intrinsicHasSet.has(await machineryValue("set-has", 1))}`);

  const intrinsicArray: i32[] = [1, 2, 3];
  print(`machinery:index-of=${intrinsicArray.indexOf(await machineryValue("index-of", 2))}`);
  print(`machinery:str-slice=${"hello".slice(await machineryValue("str-slice", 1), 3)}`);
  print(
    `machinery:math-max=${Math.max(
      (await machineryValue("math-left", 3)) as f64,
      (await machineryValue("math-right", 9)) as f64,
    )}`,
  );

  const intrinsicGetOr = new Map<i32, string>();
  intrinsicGetOr.set(1, "one");
  print(
    `machinery:map-get-or=${intrinsicGetOr.getOr(
      await machineryValue("map-get-key", 1),
      await machineryText("map-get-default", "missing"),
    )}`,
  );

  const intrinsicFixed: FixedArray<i32, 3> = [
    await machineryValue("fixed-first", 1),
    2,
    await machineryValue("fixed-last", 3),
  ];
  print(
    `machinery:fixed=${intrinsicFixed[0]},${intrinsicFixed[1]},${intrinsicFixed[2]}`,
  );

  switch (machinerySide("switch-disc", 2)) {
    case await machineryValue("switch-test-1", 1):
      print("machinery:switch=one");
      break;
    case await machineryValue("switch-test-2", 2):
      print("machinery:switch=two");
      break;
    default:
      print("machinery:switch=default");
  }

  const spreadSource: i32[] = [1, 2];
  const spreadTail: i32[] = [
    0,
    ...spreadSource,
    await machineryValue("spread-tail", 9),
  ];
  print(`machinery:spread-tail=${spreadTail.join(",")}`);
  const spreadHead: i32[] = [
    await machineryValue("spread-head", 9),
    ...spreadSource,
  ];
  print(`machinery:spread-head=${spreadHead.join(",")}`);
  const spreadAwaitOnly: i32[] = [
    ...(await machineryArray("spread-await-only")),
    1,
  ];
  print(`machinery:spread-await-only=${spreadAwaitOnly.join(",")}`);
  const spreadAwaitAfter: i32[] = [
    ...spreadSource,
    ...(await machineryArray("spread-await-after")),
  ];
  print(`machinery:spread-await-after=${spreadAwaitAfter.join(",")}`);

  const bytesOfValue: MachineryValue = new MachineryValue(
    1,
    await machineryValue("bytes-of-second", 2),
  );
  const bytesOfResult: u8[] = Context.bytesOf<MachineryValue>(bytesOfValue);
  print(`machinery:bytes-of=${bytesOfResult.length}`);

  const bytesTarget: u8[] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
  ];
  Context.bytesInto<MachineryValue>(
    machinerySideValue("bytes-into-value"),
    bytesTarget,
    (await machineryValue("bytes-into-offset", 8)) as u32,
  );
  const bytesIntoValue: MachineryValue = Context.fromBytes<MachineryValue>(
    bytesTarget,
    8,
  );
  print(
    `machinery:bytes-into=${bytesIntoValue.first},${bytesIntoValue.second}`,
  );

  const fromBytesValue: MachineryValue = Context.fromBytes<MachineryValue>(
    machinerySideBytes("from-bytes-source", bytesTarget),
    (await machineryValue("from-bytes-offset", 8)) as u32,
  );
  print(`machinery:from-bytes=${fromBytesValue.first},${fromBytesValue.second}`);

  const foreignQueue: SubDevice = subDeviceCreate(null);
  const foreignFirst: SubDevice = subDeviceCreate(null);
  const foreignSecond: SubDevice = subDeviceCreate(null);
  const foreignCommands: SubDevice[] = [foreignFirst, foreignSecond];
  const foreignProbe: u64 = subProbeQueueSubmitCheck(
    foreignQueue,
    foreignCommands,
    (await machineryValue("foreign-selector", 2)) as u32,
  );
  print(`machinery:foreign=${foreignProbe}`);
  subDeviceRelease(foreignQueue);
  subDeviceRelease(foreignFirst);
  subDeviceRelease(foreignSecond);

  const descriptorValue: MachineryDescriptor = {
    first: machinerySide("descriptor-first", 1),
    second: await machineryValue("descriptor-second", 2),
  };
  print(
    `machinery:descriptor=${descriptorValue.first},${descriptorValue.second},${descriptorValue.fallback}`,
  );

  const aggregateNew: MachineryValue = new MachineryValue(
    1,
    await machineryValue("aggregate-new", 2),
  );
  print(`machinery:aggregate-new=${aggregateNew.first},${aggregateNew.second}`);
  const aggregateCall: MachineryValue = makeMachineryValue(
    3,
    await machineryValue("aggregate-call", 4),
  );
  print(
    `machinery:aggregate-call=${aggregateCall.first},${aggregateCall.second}`,
  );
  const aggregateFixed: FixedArray<i32, 2> = makeMachineryFixed(
    5,
    await machineryValue("aggregate-fixed", 6),
  );
  print(`machinery:aggregate-fixed=${aggregateFixed[0]},${aggregateFixed[1]}`);

  let lambdaFromIf = (): i32 => 0;
  if (true) {
    const nestedFactor: i32 = 7;
    lambdaFromIf = (): i32 => nestedFactor * 3;
  }
  await Context.suspend();
  print(`machinery:lambda-if=${lambdaFromIf()}`);

  let lambdaFromBlock = (): i32 => 0;
  {
    const nestedFactor: i32 = 9;
    lambdaFromBlock = (): i32 => nestedFactor * 5;
  }
  await Context.suspend();
  print(`machinery:lambda-block=${lambdaFromBlock()}`);

  let lambdaFromSwitch = (): i32 => 0;
  switch (1) {
    case 1: {
      const nestedFactor: i32 = 6;
      lambdaFromSwitch = (): i32 => nestedFactor * 4;
      break;
    }
  }
  await Context.suspend();
  print(`machinery:lambda-switch=${lambdaFromSwitch()}`);

  let lambdaKeep = (): i32 => 0;
  {
    const nestedFactor: i32 = 2;
    lambdaKeep = (): i32 => nestedFactor * 10;
  }
  await Context.suspend();
  {
    const nestedFactor: i32 = 30;
    const lambdaSibling = (): i32 => nestedFactor * 10;
    await Context.suspend();
    print(`machinery:lambda-reuse=${lambdaKeep()},${lambdaSibling()}`);
  }

  const calleeFactor: i32 = 7;
  const suspendingCallee = (value: i32): i32 => calleeFactor + value;
  print(`s02control=${suspendingCallee(3)}`);
  print(`s02=${suspendingCallee(await roundFiveValue(3))}`);

  const argumentFactor: i32 = 7;
  const suspendingArgument = (): i32 => argumentFactor * 2;
  print(`x01control=${applyCaptured(suspendingArgument, 1)}`);
  print(
    `x01=${applyCaptured(suspendingArgument, await roundFiveValue(1))}`,
  );

  const defaultFactor: i32 = 3;
  const defaultLambda = (): i32 => defaultFactor * 5;
  switch (9) {
    default:
      print(`P2=${defaultLambda()}`);
      break;
    case await roundFiveValue(2):
      print("P2=two");
      break;
  }

  print(`P1=${takeDeclared(null, await roundFiveValue(3))}`);
  const nullableFixed: FixedArray<DeclaredBox | null, 2> = [
    null,
    await roundFiveNullable(),
  ];
  print(
    `P1fixed=${nullableFixed[0] === null},${nullableFixed[1] === null}`,
  );

  const asyncMachine = new AsyncMachine(10);
  print(`P8=${await asyncMachine.step(await roundFiveValue(5))}`);
  print(`s01=${await roundFiveIncrement(await roundFiveValue(2))}`);

  const factor: i32 = 3;
  const text: string = "managed";
  const scalar = (): i32 => factor * 5;
  const managed = (): string => `${text}:${factor}`;
  print(`lambda:before=${scalar()}:${managed()}`);
  await Context.suspend();
  print(`lambda:after=${scalar()}:${managed()}`);

  for (const value of [2, 4, 6]) {
    print(`await-for:before=${value}`);
    await Context.suspend();
    print(`await-for:after=${value}`);
  }

  for (const value of doubled([5, 6, 7])) {
    print(`yield-for:value=${value}`);
  }

  const odds = odd();
  const evens = even();
  print(`generators:turn=${odds.next().value},${evens.next().value}`);
  print(`generators:function=${resume(odds)},${resume(evens)}`);

  const remoteFactor: i32 = 7;
  const remoteCapture = (): i32 => remoteFactor * 3;
  print(`rule1k:control=${remoteCapture()}`);
  print(
    `rule1k:p6=${await applyAfterSuspension(remoteCapture, "named")}`,
  );
  print(
    `rule1k:p6b=${await applyAfterSuspension(
      (): i32 => remoteFactor * 5,
      "inline",
    )}`,
  );

  const generatorFactor: i32 = 7;
  const generatorCapture = (): i32 => generatorFactor * 3;
  const capturedGenerator = callCaptureAfterYield(generatorCapture);
  print(`rule1k:g1=${capturedGenerator.next().value}`);
  await Context.suspend();
  print(`rule1k:g2=${capturedGenerator.next().value}`);

  const transitiveFactor: i32 = 4;
  const transitiveInner = (): i32 => transitiveFactor * 5;
  const transitiveOuter = (): i32 => transitiveInner() + 1;
  print(`rule1k:transitive:before=${transitiveOuter()}`);
  await Context.suspend();
  print(`rule1k:transitive:after=${transitiveOuter()}`);

  const initializerFactor: i32 = 7;
  for (let initializerCapture = (): i32 => initializerFactor * 3; true; ) {
    print(`rule1k:initializer:before=${initializerCapture()}`);
    await Context.suspend();
    print(`rule1k:initializer:after=${initializerCapture()}`);
    break;
  }

  let chainFirst = (): i32 => 0;
  let chainSecond = (): i32 => 0;
  const chainFactor: i32 = 9;
  chainFirst = chainSecond = (): i32 => chainFactor * 2;
  print(`rule1k:chain:control=${chainSecond()}`);
  await Context.suspend();
  print(`rule1k:chain:after=${chainSecond()}`);

  const operandCallHolder = new OperandHolder(new OperandValue(10, 20));
  print(
    `call=${operandSink(operandCallHolder.value, operandBump(operandCallHolder))}`,
  );
  const operandFixedHolder = new OperandHolder(new OperandValue(10, 20));
  print(
    `fixed=${operandFixedSink(operandFixedHolder.fixed, operandBump(operandFixedHolder))}`,
  );
  const operandIndirectHolder = new OperandHolder(new OperandValue(10, 20));
  print(
    `indirect=${operandIndirect(operandIndirectHolder.value, operandBump(operandIndirectHolder))}`,
  );
  const operandCtorHolder = new OperandHolder(new OperandValue(10, 20));
  const operandPair = new OperandPair(
    operandCtorHolder.value,
    operandBump(operandCtorHolder),
  );
  print(
    `ctor=${operandPair.value.a},${operandPair.value.b},${operandPair.key}`,
  );
  const operandLiteralHolder = new OperandHolder(new OperandValue(10, 20));
  const operandLiteral: FixedArray<OperandValue, 2> = [
    operandLiteralHolder.value,
    new OperandValue(operandBump(operandLiteralHolder), 0),
  ];
  print(`lit=${operandLiteral[0].a},${operandLiteral[0].b}`);

  await unreachableAfterReturnCall();
  await unreachableAfterReturnTemplate();
  await unreachableAfterBreak();
}
