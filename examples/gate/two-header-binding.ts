// gate: two-header-binding
// proves: compiler.md §23.7 binds engine.h and interop.h together with per-mirror provenance.
// see: compiler.md §23.3, compiler.md §23.7

class EventLog {
  hits: i32;
  bytes: i32;

  constructor() {
    this.hits = 0;
    this.bytes = 0;
  }
}

class EventCounter {
  hits: i32;

  constructor() {
    this.hits = 0;
  }
}

class FixtureLog {
  hits: i32;
  bytes: i32;

  constructor() {
    this.hits = 0;
    this.bytes = 0;
  }
}

function placeholderTransform(): EngTransform {
  return new EngTransform(false, 0.0, 0.0, 0.0, 0);
}

export function main(): void {
  const limit: EngEntityLimitOption = new EngEntityLimitOption(
    new EngWorldOption(EngWorldOptionKind.ENG_WORLD_OPTION_ENTITY_LIMIT, null),
    3,
  );
  const tick: EngTickOption = new EngTickOption(
    new EngWorldOption(
      EngWorldOptionKind.ENG_WORLD_OPTION_TICK,
      limit.engHeader,
    ),
    2,
  );

  // Q13 and invariant 1: the Struct | null chain slot receives the live
  // embedded header. corpus/accept/a89-interop-chain-payload.ts and
  // compiler.md §23.7a pin reading the enclosing option payload from it.
  const world: EngWorld = engWorldCreate(tick.engHeader);
  engWorldRetain(world);

  // Q13: a string crosses as an explicit byte-length view; no NUL is added.
  engWorldSetName(world, "world");

  const log: EventLog = new EventLog();
  const counter: EventCounter = new EventCounter();
  const sink: EngEventSink = new EngEventSink(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        // C7/Q13: !== null removes null from object | null; C1 identifies
        // EventLog as the distinct nominal target of the checked narrowing.
        const eventLog = userdata1 as EventLog;
        eventLog.hits = eventLog.hits + 1;
        eventLog.bytes = eventLog.bytes + message.length;
      }
      if (userdata2 !== null) {
        // C7/Q13: the second object | null slot narrows independently; C1
        // identifies EventCounter as its distinct nominal target.
        const eventCounter = userdata2 as EventCounter;
        eventCounter.hits = eventCounter.hits + 1;
      }
    },
    log,
    counter,
  );
  engWorldSetEventSink(world, sink);
  print(`deferred=${log.hits},${counter.hits}`);
  engWorldPump(world);
  print(
    `ready=${log.hits},${log.bytes},${counter.hits},${engWorldLastEvent(world)}`,
  );

  const input: EngEntityState[] = [
    new EngEntityState(
      1,
      new EngTransform(true, 1.25, -2.5, 0.5, 3),
      ENG_ENTITY_FLAG_NONE,
    ),
    new EngEntityState(
      2,
      new EngTransform(false, 10.0, 20.0, 0.75, 5),
      ENG_ENTITY_FLAG_ACTIVE,
    ),
  ];
  // Invariant 1: this array is borrowed as the const EngEntityStateView.
  engWorldReplaceEntities(world, input);

  // Invariant 1: EngTransform crosses by value with its C padding layout.
  engWorldSetTransform(
    world,
    2,
    new EngTransform(true, 11.5, 22.25, 1.5, 9),
  );

  const output: EngEntityState[] = [
    new EngEntityState(0, placeholderTransform(), ENG_ENTITY_FLAG_NONE),
    new EngEntityState(0, placeholderTransform(), ENG_ENTITY_FLAG_NONE),
    new EngEntityState(0, placeholderTransform(), ENG_ENTITY_FLAG_NONE),
  ];
  // §14.3: the mutable EngEntityStateOut writes this array's own storage.
  const written: u64 = engWorldReadEntities(world, output);
  print(`read=${written}`);
  print(
    `entity=${output[0].engId},${output[0].engTransform.engX},${output[0].engTransform.engY},${output[0].engTransform.engRotation},${output[0].engTransform.engLayer},${output[0].engFlags}`,
  );
  print(
    `entity=${output[1].engId},${output[1].engTransform.engX},${output[1].engTransform.engY},${output[1].engTransform.engRotation},${output[1].engTransform.engLayer},${output[1].engFlags}`,
  );

  // Q18: folded u64 flag members combine without an implicit narrowing.
  const combined: EngEntityFlags =
    ENG_ENTITY_FLAG_ACTIVE | ENG_ENTITY_FLAG_VISIBLE;
  const matched: u64 = engWorldApplyFlags(
    world,
    new EngEntityBatch(combined, [1, 2, 99]),
  );
  const flagged: EngEntityState[] = [
    new EngEntityState(0, placeholderTransform(), ENG_ENTITY_FLAG_NONE),
    new EngEntityState(0, placeholderTransform(), ENG_ENTITY_FLAG_NONE),
  ];
  engWorldReadEntities(world, flagged);
  print(`flags=${matched},${flagged[0].engFlags},${flagged[1].engFlags}`);

  engWorldPump(world);
  print(
    `changed=${log.hits},${log.bytes},${counter.hits},${engWorldLastEvent(world)}`,
  );

  // The host-owned loop records frame state before a zero-argument script
  // entry; these accessors are the boundary, not a synthetic update argument.
  engFrameBegin(world, 0.125);
  const current: EngWorld = engFrameWorld();
  const fixedStep: f32 = engFrameFixedStep();
  engWorldStep(current, fixedStep);
  print(`frame=${engFrameIndex()},${fixedStep}`);
  engWorldPump(current);
  print(
    `stepped=${log.hits},${log.bytes},${counter.hits},${engWorldLastEvent(current)}`,
  );

  engWorldRelease(world);
  engWorldRelease(world);

  const device: SubDevice = subDeviceCreate(null);
  const fixtureLog: FixtureLog = new FixtureLog();
  const completion: SubCompletionInfo = new SubCompletionInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        // C7/Q13: !== null removes null from the fixture's object | null
        // slot; C1 identifies FixtureLog as its distinct nominal target.
        const observed = userdata1 as FixtureLog;
        observed.hits = observed.hits + 1;
        observed.bytes = observed.bytes + message.length;
      }
    },
    fixtureLog,
  );
  subDeviceOnComplete(device, completion);
  // The second header reconstructs SubBufferView, independently of the
  // engine header's EngEntityStateView and mutable EngEntityStateOut.
  subDeviceSubmit(device, [2, 3, 4]);
  print(`fixture-deferred=${fixtureLog.hits}`);
  subDevicePump(device);
  print(`fixture=${fixtureLog.hits},${fixtureLog.bytes}`);
  subDeviceRelease(device);
}
