// corpus: accept/a05-nominal-identity
// purpose: Uses two same-shaped nominal value types without interchanging them.
// exercises: nominal-identity, value-struct, same-shape-types
// questions: Q1, Q2, Q12

@CStruct
class Metres {
  value: f32;

  constructor(value: f32) {
    this.value = value;
  }
}

@CStruct
class Seconds {
  value: f32;

  constructor(value: f32) {
    this.value = value;
  }
}

function speed(distance: Metres, duration: Seconds): f32 {
  return distance.value / duration.value;
}

export function main(): void {
  const distance: Metres = new Metres(42.0);
  const duration: Seconds = new Seconds(6.0);
  print(`${speed(distance, duration)}`);
}
