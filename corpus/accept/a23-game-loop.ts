// corpus: accept/a23-game-loop
// purpose: Simulates a host-owned fixed-step loop for sixty deterministic frames.
// exercises: exported-lifecycle, fixed-dt, value-struct-array, state-checksum
// questions: Q1, Q2, Q4, Q12, Q14, Q15, Q17
// tsc: accepts; js-comparable: no C2: The CStruct decorator has no JavaScript shim.
const ENTITY_COUNT: i32 = 128;
const FRAME_COUNT: i32 = 60;
const FIXED_DT: f32 = 1.0 / 60.0;

@CStruct
class Entity {
  x: f32;
  y: f32;
  velocityX: f32;
  velocityY: f32;

  constructor(x: f32, y: f32, velocityX: f32, velocityY: f32) {
    this.x = x;
    this.y = y;
    this.velocityX = velocityX;
    this.velocityY = velocityY;
  }
}

let entities: Entity[] = [];

export function init(): void {
  entities = [];
  for (let index: i32 = 0; index < ENTITY_COUNT; index += 1) {
    const x: f32 = (index as f32) * 0.25;
    const y: f32 = ((index % 11) as f32) * 0.5;
    const velocityX: f32 = (((index % 7) - 3) as f32) * 0.125;
    const velocityY: f32 = (((index % 5) - 2) as f32) * 0.25;
    entities.push(new Entity(x, y, velocityX, velocityY));
  }
}

export function update(dtFixed: f32): void {
  for (let index: i32 = 0; index < entities.length; index += 1) {
    const entity: Entity = entities[index];
    entity.x += entity.velocityX * dtFixed;
    entity.y += entity.velocityY * dtFixed;
    entities[index] = entity;
  }
}

function stateChecksum(): f32 {
  let total: f32 = 0.0;
  for (let index: i32 = 0; index < entities.length; index += 1) {
    total += entities[index].x * 3.0 + entities[index].y * 5.0;
  }
  return total;
}

export function shutdown(): void {
  entities = [];
}

export function main(): void {
  init();
  for (let frame: i32 = 0; frame < FRAME_COUNT; frame += 1) {
    update(FIXED_DT);
  }
  const result: f32 = stateChecksum();
  print(`${result}`);
  shutdown();
}
