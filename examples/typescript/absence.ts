// A value that can be missing. TypeScript has two words for "no value":
// `undefined` and `null`. subscript has only `null`, and only a class
// type can hold it. This script gives a pet to some of the players.

class Pet {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
}

class Player {
  name: string;
  // `Pet | null` says: a pet, or no pet. A string or a number cannot be
  // `null`, so `string | null` and `i32 | null` do not compile.
  pet: Pet | null;
  constructor(name: string, pet: Pet | null) {
    this.name = name;
    this.pet = pet;
  }
}

function forget(player: Player): void {
  player.pet = null;
}

function describe(player: Player): string {
  // Test the field before you read a member of it. Without the test,
  // `player.pet.name` does not compile.
  if (player.pet != null) {
    return `${player.name} has ${player.pet.name}`;
  }
  return `${player.name} has no pet`;
}

function describeThenForget(player: Player): string {
  // A call can change a field after the test. Here `forget` sets the
  // field to `null`. So copy the field into a `const`, test the `const`,
  // and read the `const`. If you read `player.pet.name` after the call,
  // the compiler rejects it. tsc accepts that read.
  const pet = player.pet;
  if (pet != null) {
    forget(player);
    return `${player.name} had ${pet.name}`;
  }
  return `${player.name} had no pet`;
}

export function main(): void {
  const ada = new Player("ada", new Pet("rex"));
  const bob = new Player("bob", null);
  print(describe(ada));
  print(describe(bob));

  // `??` gives the right side when the left side is `null`.
  const stray = new Pet("a stray cat");
  print(`bob feeds ${(bob.pet ?? stray).name}`);

  // `?.` works when the whole chain is the left side of `??`.
  print(`${ada.pet?.name ?? "nobody"} and ${bob.pet?.name ?? "nobody"}`);

  print(describeThenForget(ada));
  print(describe(ada));

  // `undefined` does not compile. Write `null` in its place.
}
