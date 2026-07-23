// benchmark: particles
// 100000 @value-struct particles integrated over 1000 fixed-dt steps
// (velocity += acc*dt; position += velocity*dt). dt = 1.0 and integer-valued
// accelerations keep every intermediate an exact f64 integer, so all subjects
// agree bit-for-bit without depending on FMA contraction, while the M*K tight
// loop over array-of-value-structs is the real work.
// Checksum: i32-wrapping sum of each final position cast to i32.

@value
class Particle {
  position: f64;
  velocity: f64;

  constructor(position: f64, velocity: f64) {
    this.position = position;
    this.velocity = velocity;
  }
}

const COUNT: i32 = 100000;
const STEPS: i32 = 1000;
const DT: f64 = 1.0;

export function main(): void {
  const particles: Particle[] = [];
  for (let i: i32 = 0; i < COUNT; i += 1) {
    particles.push(new Particle(0.0, 0.0));
  }
  for (let step: i32 = 0; step < STEPS; step += 1) {
    for (let i: i32 = 0; i < COUNT; i += 1) {
      const acc: f64 = (i % 16) as f64;
      const p: Particle = particles[i];
      p.velocity += acc * DT;
      p.position += p.velocity * DT;
      particles[i] = p;
    }
  }
  let checksum: i32 = 0;
  for (let i: i32 = 0; i < COUNT; i += 1) {
    checksum += (particles[i].position as i32);
  }
  print(`${checksum}`);
}
