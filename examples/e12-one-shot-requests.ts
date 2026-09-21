// example: e12-one-shot-requests
// teaches: A request registration the host ends: per-request state in userdata, one completion inside the start call, the rest at a pump, a refused start that ends its registration at once, and the memory that returns after the release.
// differs-from-typescript: C5/Q13 put per-request state in userdata because a boundary callback cannot capture; §111 gives that registration an explicit end the host states.
// see: corpus/accept/a247, corpus/accept/a249, collisions.md C5, collisions.md Q13, compiler.md §111

// The state of one request. A boundary callback does not capture (C5),
// so every fact a completion reads travels in userdata. The world is a
// field for the same reason: the completion of one request starts the
// next one, and a non-capturing lambda reaches no local.
// C3: `follows` is a fixed-width i32 field even though tsc sees number.
// The array makes one request's graph far larger than the strings this
// program allocates, so the reclamation below is not a rounding effect.
class Request {
  world: EngineWorld;
  name: string;
  follows: i32;
  steps: i32[];

  constructor(world: EngineWorld, name: string, follows: i32) {
    this.world = world;
    this.name = name;
    this.follows = follows;
    this.steps = [];
    for (let index: i32 = 0; index < 4096; index = index + 1) {
      this.steps.push(index);
    }
  }
}

// One crossing of EngineRequestInfo creates one registration (§111 rule
// 3). Two crossings never share a record, so each request ends on its
// own. The caller keeps no reference to the Request, so that
// registration is the only thing holding the state until the host ends
// it. The answer is the engine's request number, or -1 for a start the
// engine refuses.
function startRequest(
  world: EngineWorld,
  request: Request,
  immediate: boolean,
): i32 {
  // §111 rule 1: the mirror was generated with
  // `subscript bind --header engine.h --explicit-callback-lifetime
  // EngineRequestInfo`. This text is the same with and without that
  // option; only the lifetime of the crossing differs.
  const info: EngineRequestInfo = new EngineRequestInfo(
    (message, userdata1, userdata2) => {
      if (userdata1 !== null) {
        // C7/Q13: !== null removes null from object | null, and C1 names
        // Request as the distinct nominal target of the checked cast.
        // §111 rule 4: the host passes the registration back in the
        // first userdata slot, and the runtime answers with the object.
        const state = userdata1 as Request;
        print(`done ${state.name} ${state.steps.length + message.length}`);
        if (state.follows > 0) {
          // The completion starts the next request while its own call
          // runs, so the host holds two registrations at that moment.
          // The new request completes at a later pump.
          startRequest(
            state.world,
            new Request(state.world, `${state.name}-next`, state.follows - 1),
            false,
          );
        }
      }
    },
    request,
    null,
  );
  return engineRequestStart(world, immediate, info);
}

// Q12: a zero-argument void export is a host-callable script entry.
export function main(): void {
  const world: EngineWorld = engineWorldCreate(null);
  // Q13: the C string view carries its byte length; those five bytes are
  // the message every completion reads.
  engineWorldSetName(world, "world");

  // This request completes inside the start call. The host ends its
  // registration on the path the pump uses, so the count is 1 before any
  // pump runs.
  startRequest(world, new Request(world, "alpha", 0), true);
  print(`released ${engineRequestReleaseCount(world)}`);

  // Two more complete at a pump. Each carries its own state, and
  // "gamma" starts one follow-up from inside its own completion.
  startRequest(world, new Request(world, "beta", 0), false);
  startRequest(world, new Request(world, "gamma", 1), false);
  // Starting a request ends nothing: the count does not move.
  print(`released ${engineRequestReleaseCount(world)}`);

  // Both of this engine's request slots are full now. This start still
  // crosses the boundary, so it still creates a registration (§111 rule
  // 3); the engine refuses the request, ends that registration at once,
  // and answers -1. This request's callback never runs, and the count
  // moves before any pump.
  print(`refused ${startRequest(world, new Request(world, "delta", 0), false)}`);
  print(`released ${engineRequestReleaseCount(world)}`);

  // No script reference reaches a Request now: each one was the argument
  // of a call that returned, and no lambda captured it. The two open
  // registrations are the only roots left, so this collection keeps
  // their state (invariant 2: a collection runs when the program asks
  // for it). The mark then reads the live bytes of the rooted graphs.
  Context.collect();
  engineRequestMarkLiveBytes(world);

  engineRequestPump(world);
  print(`released ${engineRequestReleaseCount(world)}`);
  // The follow-up that "gamma" started during the drain waits for this
  // pump, and its registration ends the same way.
  engineRequestPump(world);
  print(`released ${engineRequestReleaseCount(world)}`);

  // §111 rule 7: a release removes a root and does nothing else. The
  // host promises no later call, and the memory of the completed
  // requests returns at the next collection this program asks for.
  Context.collect();
  print(`reclaimed ${engineRequestLiveBytesFellBy(world, 8192)}`);

  engineWorldRelease(world);
}
