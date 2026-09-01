// example: e10-c-callbacks-and-handles
// teaches: Create, retain, and release an opaque handle, then register a two-userdata sink and pump its deferred event.
// differs-from-typescript: C5/Q13 use a noncapturing callback plus explicit userdata that outlives registration.
// see: corpus/accept/a30, corpus/accept/a35, collisions.md C5, collisions.md Q13, examples.md §4

// The same two userdata classes as e09. This example keeps the handle and the
// callback only, so one lifecycle is visible from creation to release.
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

// Q12: a zero-argument void export is a host-callable script entry.
export function main(): void {
  // Setup: three lifetime calls in a row. Read them as a count. Creation
  // gives one reference, retain gives two, and this release leaves one.
  // Q13: an opaque handle is host-created and uses explicit retain/release
  // lifetime instead of JavaScript object lifetime.
  const world: EngineWorld = engineWorldCreate(null);
  engineWorldRetain(world);
  engineWorldRelease(world);

  // Q13: the C string view carries the byte length and assumes no NUL.
  engineWorldSetName(world, "world");

  // The callback phase. The state a callback writes must exist before
  // registration and stay alive through every later pump.
  const log: EventLog = new EventLog();
  const counter: EventCounter = new EventCounter();
  // C5/Q13: a stored C callback cannot capture; both mutable objects cross
  // explicitly as userdata and remain alive through the later pump.
  // Rejected alternative: capturing log in this constructed sink is S009,
  // "capturing lambdas may not escape into constructed objects";
  // corpus/reject/r10-escaping-capture.ts pins C5.
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

  // Q13: registration stores the sink without firing; the host-owned pump
  // performs the deferred call on the calling thread.
  engineWorldSetEventSink(world, sink);
  // deferred=0,0 shows that registration alone calls nothing.
  print(`deferred=${log.hits},${counter.hits}`);
  engineWorldPump(world);
  // ready=1,5,1,0 is the state after the pump: one call, the five bytes of
  // the world name, one count, and the delivered event kind.
  print(
    `ready=${log.hits},${log.bytes},${counter.hits},${engineWorldLastEvent(world)}`,
  );

  // Q13: retain kept one reference alive across the first release; this
  // release ends the opaque handle's lifetime.
  engineWorldRelease(world);
}
