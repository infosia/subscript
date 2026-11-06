<!-- §226 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 226. The x86-64 and Windows limits follow the measurement

*(Added 2026-11-06.)* Origin: §222.4 item 1. The owner selected the
values on 2026-11-06. The measurement is in
`specs/tracking/collect-perf-limit.md`, section "Measurement on x86-64
with `cl`".

Problem, measured at `75ef5164` on a 20-CPU x86-64 Windows host with
`cl` 19.44.35222: the highest `collect` ship figure in 30 runs is
1.68x. The limit is 7.50x, so the gate does not report a ship slowdown
under about 5x. At two busy processes per CPU, each of five runs
missed a dev-JIT limit: `collect` dev read 6.04x to 6.37x against 6.00x,
and `a22` dev read 26.60x against 25.00x. On Windows the dev-JIT figure
is a wall minimum (§213.1 rule 1a), and load moves it more than a CPU
time figure.

### 226.1 Rules

1. On x86-64, the `collect` ship-tier limit is 2.00x. It is 19% over
   the highest figure and 41% over the highest quiet figure.
2. On Windows, the `collect` dev-JIT limit is 7.50x. It is 18% over the
   highest figure.
3. On Windows, the `a22` dev-JIT limit is 31.00x. It is 17% over the
   highest figure.
4. Every other limit stays. §222 rules 1 and 3 stay for every host that
   is not Windows.
5. A limit that differs by host states its reason in the report, as the
   ship-tier scope does.

### 226.2 Acceptance

1. A unit test reads each limit of rules 1 to 3 for its host, and the
   limits of rule 4 for the other hosts.
2. `perf-gate --gate` meets each limit on this host on a quiet machine.
3. §3, §213, and §222 cite this section where they state these limits.
4. `tools/gate.sh full` is green, and no golden moves.

### 226.3 Open

1. The measurement is one host and one `cl` version. A miss on another
   x86-64 host gives the next figure.
