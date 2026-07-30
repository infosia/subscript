// example: e09-c-structs-and-slices
// teaches: Bind a game-shaped C facade with by-value structs, const and mutable slices, string views, enums, and flags.
// differs-from-typescript: invariant 1 makes each mirrored value class the C struct itself, with no marshaling copy.
// see: corpus/accept/a25-a33, corpus/accept/a89, compiler.md §23.7, examples.md §4

// C3: these counters have fixed-width i32 fields even though tsc sees
// number. Rejected alternative: a number field is S007; diagnostic excerpt:
// "bare `number` is rejected; there is no default numeric type — use a sized type";
// corpus/reject/r08-bare-number.ts pins it.
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

function placeholderTransform(): EngineTransform {
  return new EngineTransform(false, 0.0, 0.0, 0.0, 0);
}

// Q12: a zero-argument void export is a host-callable script entry.
export function main(): void {
  const limit: EngineEntityLimitOption = new EngineEntityLimitOption(
    new EngineWorldOption(EngineWorldOptionKind.ENGINE_WORLD_OPTION_ENTITY_LIMIT, null),
    3,
  );
  // Q13 and invariant 1: the Struct | null chain slot receives the live
  // embedded header. corpus/accept/a89-interop-chain-payload.ts and
  // compiler.md §23.7a pin reading the enclosing option payload from it.
  const tick: EngineTickOption = new EngineTickOption(
    new EngineWorldOption(
      EngineWorldOptionKind.ENGINE_WORLD_OPTION_TICK,
      limit.engineHeader,
    ),
    2,
  );

  // Q13: an opaque handle is host-created and uses explicit retain/release
  // lifetime instead of JavaScript object lifetime.
  const world: EngineWorld = engineWorldCreate(tick.engineHeader);
  engineWorldRetain(world);

  // Q13: a string crosses as an explicit byte-length view; no NUL is added.
  engineWorldSetName(world, "world");

  const log: EventLog = new EventLog();
  const counter: EventCounter = new EventCounter();
  // C5/Q13: a stored C callback cannot capture; both mutable objects cross
  // explicitly as userdata and remain alive through each later pump.
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
  // Q13 and invariant 1: the mutable EngineEntityStateOut writes this array's
  // own C-layout storage rather than a marshaled copy.
  const written: u64 = engineWorldReadEntities(world, output);
  print(`read=${written}`);
  // Q14: f32 interpolation uses deterministic shortest-round-trip spelling,
  // so the C values print identically on both execution tiers.
  print(
    `entity=${output[0].engineId},${output[0].engineTransform.engineX},${output[0].engineTransform.engineY},${output[0].engineTransform.engineRotation},${output[0].engineTransform.engineLayer},${output[0].engineFlags}`,
  );
  print(
    `entity=${output[1].engineId},${output[1].engineTransform.engineX},${output[1].engineTransform.engineY},${output[1].engineTransform.engineRotation},${output[1].engineTransform.engineLayer},${output[1].engineFlags}`,
  );

  // Q18: folded u64 flag members combine without an implicit narrowing.
  const combined: EngineEntityFlags =
    ENGINE_ENTITY_FLAG_ACTIVE | ENGINE_ENTITY_FLAG_VISIBLE;
  // Invariant 1: the id array becomes the batch's embedded count/pointer pair
  // without a wrapper allocation.
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

  // Q12: exported entries receive no arguments, so the host-owned loop records
  // frame state before the call and the script reads it through the facade.
  engineFrameBegin(world, 0.125);
  const current: EngineWorld = engineFrameWorld();
  const fixedStep: f32 = engineFrameFixedStep();
  engineWorldStep(current, fixedStep);
  print(`frame=${engineFrameIndex()},${fixedStep}`);
  engineWorldPump(current);
  print(
    `stepped=${log.hits},${log.bytes},${counter.hits},${engineWorldLastEvent(current)}`,
  );

  // Q13: the two explicit releases balance the host handle's initial
  // reference and the retained reference.
  engineWorldRelease(world);
  engineWorldRelease(world);
}
