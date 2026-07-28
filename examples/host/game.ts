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

function emptyTransform(): EngTransform {
  return new EngTransform(false, 0.0, 0.0, 0.0, 0);
}

// Q12: the host supplies frame state through accessors because exported
// entries are zero-argument and void.
export function init(): void {
  const world: EngWorld = engFrameWorld();
  const fixedStep: f32 = engFrameFixedStep();
  const frameIndex: u64 = engFrameIndex();
  session = new SessionState();
  engWorldSetName(world, "capstone");
  engWorldSetTransform(
    world,
    1,
    new EngTransform(false, 0.0, fixedStep, 0.0, frameIndex as u16),
  );
  const appliedFlags: EngEntityFlags = engWorldApplyFlags(
    world,
    new EngEntityBatch(
      ENG_ENTITY_FLAG_ACTIVE | ENG_ENTITY_FLAG_VISIBLE,
      [1],
    ),
  );
  if (appliedFlags !== ENG_ENTITY_FLAG_NONE) {
    // Q14: fractional output uses the runtime formatter, not host libc.
    print(`script:init step=${fixedStep}`);
  }
}

// Q12: each host frame records the world, fixed step, and index before this
// zero-argument entry reads them.
export function update(): void {
  const world: EngWorld = engFrameWorld();
  const fixedStep: f32 = engFrameFixedStep();
  const frameIndex: u64 = engFrameIndex();
  if (session !== null) {
    session.distance += fixedStep;
    engWorldSetTransform(
      world,
      1,
      new EngTransform(
        false,
        session.distance,
        fixedStep,
        0.0,
        frameIndex as u16,
      ),
    );
    engWorldStep(world, fixedStep);
  }

  const states: EngEntityState[] = [
    new EngEntityState(0, emptyTransform(), ENG_ENTITY_FLAG_NONE),
  ];
  const stateCount: u64 = engWorldReadEntities(world, states);
  if (stateCount !== 0) {
    // Q14: positions and fixed time remain script-formatted floats.
    print(
      `script:update x=${states[0].engTransform.engX},step=${fixedStep}`,
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
