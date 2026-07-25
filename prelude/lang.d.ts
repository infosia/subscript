// Ambient declarations make the accept corpus tsc-clean; semantic enforcement lives in the language compiler.

declare type i32 = number;
declare type u32 = number;
declare type i64 = number;
declare type u64 = number;
declare type f32 = number;
declare type f64 = number;
declare type i8 = number;
declare type u8 = number;
declare type i16 = number;
declare type u16 = number;
declare type f16 = number;

declare function print(message: string): void;
declare function collect(): void;
declare function unsafeDelete(value: object): void;

declare class JsonResult<T> {
  private constructor();
  ok: boolean;
  value: T;
}

declare interface JSON {
  parse<T>(text: string): JsonResult<T>;
}

declare function CStruct<T extends abstract new (...args: never[]) => object>(
  target: T,
  context: ClassDecoratorContext,
): T;

declare interface FixedArray<T, N extends number> {
  [index: number]: T;
  readonly length: i32;
  forEach(callback: (value: T, index: i32) => void): void;
  map<U>(callback: (value: T, index: i32) => U): U[];
  filter(callback: (value: T, index: i32) => boolean): T[];
  some(callback: (value: T, index: i32) => boolean): boolean;
  every(callback: (value: T, index: i32) => boolean): boolean;
  findIndex(callback: (value: T, index: i32) => boolean): i32;
  reduce<U>(callback: (acc: U, value: T, index: i32) => U, init: U): U;
  reduceRight<U>(callback: (acc: U, value: T, index: i32) => U, init: U): U;
}

// Ambient augmentation of ES2022's Map surface; the compiler narrows
// `get` to nullable-capable values and accepts this total accessor for
// every storable value type.
declare interface Map<K, V> {
  getOr(key: K, fallback: V): V;
}

declare interface MapConstructor {
  groupBy<K, T>(items: T[], callback: (value: T) => K): Map<K, T[]>;
}

declare interface Set<T> {
  union(other: Set<T>): Set<T>;
  intersection(other: Set<T>): Set<T>;
  difference(other: Set<T>): Set<T>;
  symmetricDifference(other: Set<T>): Set<T>;
  isSubsetOf(other: Set<T>): boolean;
  isSupersetOf(other: Set<T>): boolean;
  isDisjointFrom(other: Set<T>): boolean;
}

// N is deliberately unused structurally so plain array literals remain assignable.
