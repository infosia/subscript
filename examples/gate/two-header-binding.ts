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

function placeholderTransform(): EngineTransform {
  return new EngineTransform(false, 0.0, 0.0, 0.0, 0);
}

export function main(): void {
  const limit: EngineEntityLimitOption = new EngineEntityLimitOption(
    new EngineWorldOption(EngineWorldOptionKind.ENGINE_WORLD_OPTION_ENTITY_LIMIT, null),
    3,
  );
  const tick: EngineTickOption = new EngineTickOption(
    new EngineWorldOption(
      EngineWorldOptionKind.ENGINE_WORLD_OPTION_TICK,
      limit.engineHeader,
    ),
    2,
  );

  // Q13 and invariant 1: the Struct | null chain slot receives the live
  // embedded header. corpus/accept/a89-interop-chain-payload.ts and
  // compiler.md §23.7a pin reading the enclosing option payload from it.
  const world: EngineWorld = engineWorldCreate(tick.engineHeader);
  engineWorldRetain(world);

  // Q13: a string crosses as an explicit byte-length view; no NUL is added.
  engineWorldSetName(world, "world");

  const log: EventLog = new EventLog();
  const counter: EventCounter = new EventCounter();
  const sink: EngineEventSink = new EngineEventSink(
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
  engineWorldSetEventSink(world, sink);
  print(`deferred=${log.hits},${counter.hits}`);
  engineWorldPump(world);
  print(
    `ready=${log.hits},${log.bytes},${counter.hits},${engineWorldLastEvent(world)}`,
  );

  const input: EngineEntityState[] = [
    new EngineEntityState(
      1,
      new EngineTransform(true, 1.25, -2.5, 0.5, 3),
      ENGINE_ENTITY_FLAG_NONE,
    ),
    new EngineEntityState(
      2,
      new EngineTransform(false, 10.0, 20.0, 0.75, 5),
      ENGINE_ENTITY_FLAG_ACTIVE,
    ),
  ];
  // Invariant 1: this array is borrowed as the const EngineEntityStateView.
  engineWorldReplaceEntities(world, input);

  // Invariant 1: EngineTransform crosses by value with its C padding layout.
  engineWorldSetTransform(
    world,
    2,
    new EngineTransform(true, 11.5, 22.25, 1.5, 9),
  );

  const output: EngineEntityState[] = [
    new EngineEntityState(0, placeholderTransform(), ENGINE_ENTITY_FLAG_NONE),
    new EngineEntityState(0, placeholderTransform(), ENGINE_ENTITY_FLAG_NONE),
    new EngineEntityState(0, placeholderTransform(), ENGINE_ENTITY_FLAG_NONE),
  ];
  // §14.3: the mutable EngineEntityStateOut writes this array's own storage.
  const written: u64 = engineWorldReadEntities(world, output);
  print(`read=${written}`);
  print(
    `entity=${output[0].engineId},${output[0].engineTransform.engineX},${output[0].engineTransform.engineY},${output[0].engineTransform.engineRotation},${output[0].engineTransform.engineLayer},${output[0].engineFlags}`,
  );
  print(
    `entity=${output[1].engineId},${output[1].engineTransform.engineX},${output[1].engineTransform.engineY},${output[1].engineTransform.engineRotation},${output[1].engineTransform.engineLayer},${output[1].engineFlags}`,
  );

  // Q18: folded u64 flag members combine without an implicit narrowing.
  const combined: EngineEntityFlags =
    ENGINE_ENTITY_FLAG_ACTIVE | ENGINE_ENTITY_FLAG_VISIBLE;
  const matched: u64 = engineWorldApplyFlags(
    world,
    new EngineEntityBatch(combined, [1, 2, 99]),
  );
  const flagged: EngineEntityState[] = [
    new EngineEntityState(0, placeholderTransform(), ENGINE_ENTITY_FLAG_NONE),
    new EngineEntityState(0, placeholderTransform(), ENGINE_ENTITY_FLAG_NONE),
  ];
  engineWorldReadEntities(world, flagged);
  print(`flags=${matched},${flagged[0].engineFlags},${flagged[1].engineFlags}`);

  engineWorldPump(world);
  print(
    `changed=${log.hits},${log.bytes},${counter.hits},${engineWorldLastEvent(world)}`,
  );

  // The host-owned loop records frame state before a zero-argument script
  // entry; these accessors are the boundary, not a synthetic update argument.
  engineFrameBegin(world, 0.125);
  const current: EngineWorld = engineFrameWorld();
  const fixedStep: f32 = engineFrameFixedStep();
  engineWorldStep(current, fixedStep);
  print(`frame=${engineFrameIndex()},${fixedStep}`);
  engineWorldPump(current);
  print(
    `stepped=${log.hits},${log.bytes},${counter.hits},${engineWorldLastEvent(current)}`,
  );

  engineWorldRelease(world);
  engineWorldRelease(world);

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
  // engine header's EngineEntityStateView and mutable EngineEntityStateOut.
  subDeviceSubmit(device, [2, 3, 4]);
  print(`fixture-deferred=${fixtureLog.hits}`);
  subDevicePump(device);
  print(`fixture=${fixtureLog.hits},${fixtureLog.bytes}`);
  subDeviceRelease(device);
}
