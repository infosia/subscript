// host example: context-per-scene
// proves: Script state belongs to one Context while the engine frame record belongs to the host thread.
// see: examples.md §5a, examples.md §5, compiler.md §8.1a

// The host runs two scenes in one process, each with a Context of its own.
// This script shows which state restarts and which state continues.

// Per-scene allocations. They stay reachable through the global below.
class SceneAllocation {
  frame: f32;

  constructor(frame: f32) {
    this.frame = frame;
  }
}

class SceneState {
  frame: f32;
  allocations: SceneAllocation[];

  constructor() {
    this.frame = 0.0;
    this.allocations = [];
  }
}

// subscript_init evaluates this initializer for every fresh Context. The host does
// not clear or replace this global between calls: rebuilding the Context is
// what makes the second scene start from zero.
let scene: SceneState = new SceneState();

// Frame phase: each call adds one allocation and steps the host's world. The
// scene counter restarts at 1 in scene two; the host's frame index counts on.
export function update(): void {
  const world: EngineWorld = engineFrameWorld();
  const fixedStep: f32 = engineFrameFixedStep();
  scene.frame += 1.0;
  scene.allocations.push(new SceneAllocation(scene.frame));
  engineWorldStep(world, fixedStep);

  // §5 keeps fractional formatting on the script side. `scene.frame` is an
  // f32 so the script-owned counter and fixed step share that deterministic
  // formatter.
  print(`script:scene-frame=${scene.frame},step=${fixedStep}`);
}

// Teardown phase: the host reads live_bytes after this call, then releases the
// Context. The reachable scene allocations end at that release.
export function finish(): void {
  // Collection answers a question inside this live Context. Every scene
  // allocation remains reachable through the module global, so collection
  // does not end the scene or replace Context release.
  Context.collect();
  print(`script:scene-end frame=${scene.frame}`);
}
