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

declare function CStruct<T extends abstract new (...args: never[]) => object>(
  target: T,
  context: ClassDecoratorContext,
): T;

declare interface FixedArray<T, N extends number> {
  [index: number]: T;
  readonly length: i32;
}

// Ambient augmentation of ES2022's Map surface; the compiler narrows
// `get` to nullable-capable values and accepts this total accessor for
// every storable value type.
declare interface Map<K, V> {
  getOr(key: K, fallback: V): V;
}

// N is deliberately unused structurally so plain array literals remain assignable.
