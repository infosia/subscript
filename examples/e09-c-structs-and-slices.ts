// example: e09-c-structs-and-slices
// teaches: Bind a game-shaped C facade with by-value structs, const and mutable slices, string views, enums, and flags.
// differs-from-typescript: invariant 1 makes each mirrored value class the C struct itself, with no marshaling copy.
// see: corpus/accept/a25-a33, corpus/accept/a89, compiler.md §23.7, examples.md §4

// C3: these counters have fixed-width i32 fields even though tsc sees
// number. Rejected alternative: a number field is S007, "no default
// numeric type; use a sized type"; corpus/reject/r08-bare-number.ts pins it.
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

function placeholderTransform(): EngTransform {
  return new EngTransform(false, 0.0, 0.0, 0.0, 0);
}

// Q12: a zero-argument void export is a host-callable script entry.
export function main(): void {
  const limit: EngEntityLimitOption = new EngEntityLimitOption(
    new EngWorldOption(EngWorldOptionKind.ENG_WORLD_OPTION_ENTITY_LIMIT, null),
    3,
  );
  // Q13 and invariant 1: the Struct | null chain slot receives the live
  // embedded header. corpus/accept/a89-interop-chain-payload.ts and
  // compiler.md §23.7a pin reading the enclosing option payload from it.
  const tick: EngTickOption = new EngTickOption(
    new EngWorldOption(
      EngWorldOptionKind.ENG_WORLD_OPTION_TICK,
      limit.engHeader,
    ),
    2,
  );

  // Q13: an opaque handle is host-created and uses explicit retain/release
  // lifetime instead of JavaScript object lifetime.
  const world: EngWorld = engWorldCreate(tick.engHeader);
  engWorldRetain(world);

  // Q13: a string crosses as an explicit byte-length view; no NUL is added.
  engWorldSetName(world, "world");

  const log: EventLog = new EventLog();
  const counter: EventCounter = new EventCounter();
  // C5/Q13: a stored C callback cannot capture; both mutable objects cross
  // explicitly as userdata and remain alive through each later pump.
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
  // Q13 and invariant 1: the mutable EngEntityStateOut writes this array's
  // own C-layout storage rather than a marshaled copy.
  const written: u64 = engWorldReadEntities(world, output);
  print(`read=${written}`);
  // Q14: f32 interpolation uses deterministic shortest-round-trip spelling,
  // so the C values print identically on both execution tiers.
  print(
    `entity=${output[0].engId},${output[0].engTransform.engX},${output[0].engTransform.engY},${output[0].engTransform.engRotation},${output[0].engTransform.engLayer},${output[0].engFlags}`,
  );
  print(
    `entity=${output[1].engId},${output[1].engTransform.engX},${output[1].engTransform.engY},${output[1].engTransform.engRotation},${output[1].engTransform.engLayer},${output[1].engFlags}`,
  );

  // Q18: folded u64 flag members combine without an implicit narrowing.
  const combined: EngEntityFlags =
    ENG_ENTITY_FLAG_ACTIVE | ENG_ENTITY_FLAG_VISIBLE;
  // Invariant 1: the id array becomes the batch's embedded count/pointer pair
  // without a wrapper allocation.
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

  // Q12: exported entries receive no arguments, so the host-owned loop records
  // frame state before the call and the script reads it through the facade.
  engFrameBegin(world, 0.125);
  const current: EngWorld = engFrameWorld();
  const fixedStep: f32 = engFrameFixedStep();
  engWorldStep(current, fixedStep);
  print(`frame=${engFrameIndex()},${fixedStep}`);
  engWorldPump(current);
  print(
    `stepped=${log.hits},${log.bytes},${counter.hits},${engWorldLastEvent(current)}`,
  );

  // Q13: the two explicit releases balance the host handle's initial
  // reference and the retained reference.
  engWorldRelease(world);
  engWorldRelease(world);
}
