// corpus: accept/a24-particle-system
// purpose: Updates equivalent array-of-structs and struct-of-arrays particle layouts.
// exercises: value-struct, array-of-structs, struct-of-arrays, tight-loop, checksum
// questions: Q1, Q2, Q4, Q12, Q14, Q15, Q17

const PARTICLE_COUNT: i32 = 2048;
const STEP_COUNT: i32 = 50;

@CStruct
class Particle {
  position: f32;
  velocity: f32;

  constructor(position: f32, velocity: f32) {
    this.position = position;
    this.velocity = velocity;
  }
}

function updateArrayOfStructs(particles: Particle[]): void {
  for (let step: i32 = 0; step < STEP_COUNT; step += 1) {
    for (let index: i32 = 0; index < particles.length; index += 1) {
      const particle: Particle = particles[index];
      particle.velocity += (((index % 9) - 4) as f32) * 0.0001;
      particle.position += particle.velocity;
      particles[index] = particle;
    }
  }
}

function updateStructOfArrays(positions: f32[], velocities: f32[]): void {
  for (let step: i32 = 0; step < STEP_COUNT; step += 1) {
    for (let index: i32 = 0; index < positions.length; index += 1) {
      velocities[index] += (((index % 9) - 4) as f32) * 0.0001;
      positions[index] += velocities[index];
    }
  }
}

export function main(): void {
  const particles: Particle[] = [];
  const positions: f32[] = [];
  const velocities: f32[] = [];

  for (let index: i32 = 0; index < PARTICLE_COUNT; index += 1) {
    const position: f32 = (index as f32) * 0.01;
    const velocity: f32 = (((index % 13) - 6) as f32) * 0.001;
    particles.push(new Particle(position, velocity));
    positions.push(position);
    velocities.push(velocity);
  }

  updateArrayOfStructs(particles);
  updateStructOfArrays(positions, velocities);

  let checksum: f32 = 0.0;
  for (let index: i32 = 0; index < PARTICLE_COUNT; index += 1) {
    checksum += particles[index].position + positions[index];
  }
  print(`${checksum}`);
}
