# Allocations that do not escape

Measurement rounds (Step 0). No section exists. Nothing landed: each
round kept its scratch material outside the repository.

Question: the collector and `Context.free` stay. What does the
compiler gain if it proves that a new block does not escape? Three
consumers of one proof were measured: a release at the last use, the
reuse of an input buffer by a built-in array method, and no heap
allocation.

## N1 — the value of each consumer, by hand, at `a3ead965`

Method: arm64 macOS, the ship tier, the release CLI emits C, clang
`-O2 -fwrapv -ffp-contract=off`. A timing entry makes a new Context
for each sample. The timed span is the exported call through the
outermost exit. Each figure is the median of 10 samples after a
warm-up of 3 calls and 200 ms. The variants ran in the sequence a, b,
c, d, d, c, b, a. Each variant printed the checksum of its original.
A variant that the source cannot state is a hand change of the
emitted C.

Workload `callbacks` (20 rounds; map, filter, reduce over 1,000,000
`i32`):

| Variant | Collection | ms | Automatic collections | Live bytes after | Peak RSS, MiB |
|---|---|---|---|---|---|
| Original | on | 36.09 | 4 | 28,194,688 | 66.4 |
| Original | off | 36.22 | 0 | 164,196,320 | 164.1 |
| Release of each buffer at its last use | on | 35.31 | 0 | 4,194,400 | 20.2 |
| Release of each buffer at its last use | off | 37.76 | 0 | 4,194,400 | 18.0 |
| The filter writes into the buffer of its input | on | 24.91 | 3 | 24,194,640 | 43.3 |
| The filter writes into the buffer of its input | off | 27.41 | 0 | 84,195,360 | 87.3 |
| Reuse and release | on | 24.55 | 0 | 4,194,400 | 16.4 |
| Reuse and release | off | 24.71 | 0 | 4,194,400 | 14.2 |

The collection time of the original is below 60 µs for the call: an
array of `i32` holds no handle (§344).

Workload "temporaries beside a live graph": N kept objects with one
reference field each, then 5,000,000 iterations. Each iteration
makes one class instance of two `f64` fields, reads the fields, and
drops it.

| Variant | Collection | ms, N = 20,000 | ms, N = 200,000 | Reserved after, N = 20,000 |
|---|---|---|---|---|
| Collector only | on | 52.64 | 125.59 | 35,084,272 |
| Collector only | off | 43.52 | 46.89 | 172,050,416 |
| `Context.free` at the last use | on | 77.04 | 76.66 | 2,078,704 |
| `Context.free` at the last use | off | 76.92 | 76.67 | 2,078,704 |
| A C local, the constructor call stays | on | 19.38 | 22.31 | 2,078,704 |
| A C local, the constructor is two stores | on | 8.94 | 11.25 | 2,078,704 |

The collector-only form ran 5 automatic collections, with a total
time of 8.4 to 9.0 ms at N = 20,000 and 77.3 to 79.0 ms at
N = 200,000. The other forms ran none.

Workload `collect`: one allocation has a visible last use (a string
of each iteration). Its release changed the time from 30.46 to
30.63 ms.

Results:

- A release of a small object costs about 6.7 ns (the difference
  between `Context.free` and no release, with the collection off). A
  profile shows `arena_release`, `Context::delete`, and the
  allocation. The release is slower than the collector at
  N = 20,000 and faster at N = 200,000.
- No heap allocation is 2.7 to 5.6 times faster than the collector
  with the constructor call, and 5.9 to 11.2 times faster with the
  constructor as stores.
- The reuse of a buffer removes 31 % of the time of `callbacks`, with
  the collection on or off.
- The release of a large buffer does not change the time. It lowers
  the live bytes of the Context from 28.2 to 4.2 MB with the
  collection on, and from 164.2 to 4.2 MB with it off.
- `Context.free` accepts an `object` only (Q6). The source cannot
  release an array or a string: the checker gives S100.

## N2 — the coverage of the proof, at `a3ead965`

Method: a scratch analysis over the public LIR, run from a scratch
test. A site is an instruction that makes a block: a class instance,
an array, a string that the program allocates, a closure environment
with a capture, a Map, a Set, a regex value, a boundary box. A value
escapes at a store into a field, an element, a Map, a Set, a global,
or a captured variable; at a return, a throw, a host call, a worker
post; and where it is live across a suspension. Level L0: each pass
to a user function is an escape. Level L1: a summary for each
parameter ("does not escape") by a fixpoint over the call graph; an
indirect call is an escape. Level L2: L1, and a function value that
the function itself makes takes the summary of its target.

Populations: 498 entries of `corpus/accept`; 17 of the 18 benchmark
workloads (`bound-call` needs a host header); 9 of the 12 examples
(three need a host header).

| Kind | Corpus, sites | L0 | L1 | L2 |
|---|---|---|---|---|
| Class instance | 1,085 | 87 | 367 | 369 |
| Array literal | 762 | 234 | 346 | 347 |
| Array of an inline map or filter | 79 | 71 | 73 | 73 |
| Array from an array method | 42 | 27 | 30 | 30 |
| Array from another intrinsic | 36 | 20 | 35 | 35 |
| Array spread literal | 24 | 20 | 21 | 21 |
| Closure environment | 179 | 78 | 88 | 89 |
| Map | 76 | 23 | 26 | 26 |
| Set | 43 | 13 | 13 | 13 |
| Regex value | 61 | 6 | 9 | 9 |
| Boundary box | 66 | 0 | 0 | 0 |
| String literal | 3,506 | 1,978 | 2,299 | 2,300 |
| String template | 2,905 | 2,723 | 2,724 | 2,724 |
| String from an intrinsic | 674 | 396 | 402 | 402 |
| String `+` | 84 | 40 | 50 | 50 |
| All | 9,622 | 5,716 | 6,483 | 6,488 |

| Population | Class sites | Proved at L1 | Array sites | Proved at L1 | Reuse pattern |
|---|---|---|---|---|---|
| Corpus | 1,085 | 367 | 943 | 505 | 12 |
| Benchmarks | 4 | 0 | 9 | 7 | 1 |
| Examples | 17 | 1 | 3 | 3 | 0 |

The reuse pattern is a new array that exactly one built-in method
reads, where that method returns a new array of the same element
type. The one benchmark site is `mapped` of `callbacks`.

The first reason of a class site of the corpus that L1 does not
prove (718): throw 218, return 99, store to an element 93, store to
a field 65, closure capture 62, a parameter that escapes 47, across
a suspension 27, worker post 37, Map or Set 18, async call argument
16, global 14, indirect call 12, `Context.free` 5, yield 5.

The four class sites of the benchmarks are those of `tree` (return)
and `collect` (a field): each one escapes in the source. Of the 17
class sites of the examples, 7 are thrown, 5 go to a Worker, and 4
go to `Context.free`.

Of the 367 class sites that L1 proves, 173 have a constructor that
only stores parameters and constants, 165 have no constructor, and
29 have another constructor. 172 have a reference field.

A hand check of 19 proved sites against the source found no escape.
Ten sites that L1 does not prove do not escape in the source: a
value in a local array, Map, or descriptor that only reads follow; a
parameter that the callee returns; a capture by a lambda that a
built-in runs at once; a `using` resource across an `await`.

LIR does not carry these facts, and the prototype used a
conservative form or a name list for each:

1. Which operand the result of an intrinsic can alias.
2. Which operand an intrinsic keeps (the counted class covers arrays
   only). The prototype read each operand of the String, Json, Regex,
   Number, Date, Text, and Set families as a borrow. No run of the
   runtime checked that reading.
3. The target set of an indirect call.
4. The map from a call operand to a parameter for a closure call.
5. That a coroutine frame owns its parameters from its creation.

## Reading

- The large gain (no heap allocation) has no site in the benchmarks
  and one site in the examples. The corpus has sites, and the corpus
  is a set of tests, not a sample of host programs.
- A class that holds numbers only already has a form with no heap
  allocation: `@ValueType`. The temporary of the N1 workload is such
  a class.
- The reuse of a buffer has one site in the benchmarks, with a
  measured gain of 31 % there.
- A release at a last use is a gain for a large buffer in memory, not
  in time, and a loss for a small object beside a small live graph.

## N3 — the two sections that landed

- §358: `Context.free` releases a dynamic array of an uncounted
  type, its header and its buffer. `tree` in the ship tier is 93.0 ms
  against 95.1 ms before the section (a median of 10 for each).
- §359: a static `filter` of a new array with one reader writes into
  the buffer of its receiver. `callbacks` in the ship tier goes from
  36.47 to 25.24 ms with the collection on and from 36.91 to
  25.70 ms with it off; the development tier goes from 255.57 to
  251.16 ms.

Not landed, with the reason in the reading above: a class instance
with no heap allocation, a release at a last use, `Context.move`,
and `Move<T>`. A later decision on the first needs the count of the
proved class sites of a host program.
