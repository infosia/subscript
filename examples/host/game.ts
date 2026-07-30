// capstone: host/game
// proves: A C host owns the loop and drives zero-argument script exports through the engine facade.
// see: examples.md §5, collisions.md Q12-Q14, compiler.md §18.1a-§18.2d

class SessionState {
  distance: f32;

  constructor() {
    this.distance = 0.0;
  }
}

let session: SessionState | null = null;

function emptyTransform(): EngineTransform {
  return new EngineTransform(false, 0.0, 0.0, 0.0, 0);
}

// Q12: the host supplies frame state through accessors because exported
// entries are zero-argument and void.
export function init(): void {
  const world: EngineWorld = engineFrameWorld();
  const fixedStep: f32 = engineFrameFixedStep();
  const frameIndex: u64 = engineFrameIndex();
  session = new SessionState();
  engineWorldSetName(world, "capstone");
  engineWorldSetTransform(
    world,
    1,
    new EngineTransform(false, 0.0, fixedStep, 0.0, frameIndex as u16),
  );
  const appliedFlags: EngineEntityFlags = engineWorldApplyFlags(
    world,
    new EngineEntityBatch(
      ENGINE_ENTITY_FLAG_ACTIVE | ENGINE_ENTITY_FLAG_VISIBLE,
      [1],
    ),
  );
  if (appliedFlags !== ENGINE_ENTITY_FLAG_NONE) {
    // Q14: fractional output uses the runtime formatter, not host libc.
    print(`script:init step=${fixedStep}`);
  }
}

// Q12: each host frame records the world, fixed step, and index before this
// zero-argument entry reads them.
export function update(): void {
  const world: EngineWorld = engineFrameWorld();
  const fixedStep: f32 = engineFrameFixedStep();
  const frameIndex: u64 = engineFrameIndex();
  if (session !== null) {
    session.distance += fixedStep;
    engineWorldSetTransform(
      world,
      1,
      new EngineTransform(
        false,
        session.distance,
        fixedStep,
        0.0,
        frameIndex as u16,
      ),
    );
    engineWorldStep(world, fixedStep);
  }

  const states: EngineEntityState[] = [
    new EngineEntityState(0, emptyTransform(), ENGINE_ENTITY_FLAG_NONE),
  ];
  const stateCount: u64 = engineWorldReadEntities(world, states);
  if (stateCount !== 0) {
    // Q14: positions and fixed time remain script-formatted floats.
    print(
      `script:update x=${states[0].engineTransform.engineX},step=${fixedStep}`,
    );
  }
}

export function shutdown(): void {
  // C7 and invariant 2: removing the last root does not collect by itself;
  // this explicit call is the event the host measures around.
  session = null;
  Context.collect();
  print("script:shutdown");
}
