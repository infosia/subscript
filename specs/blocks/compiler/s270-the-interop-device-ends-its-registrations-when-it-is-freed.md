<!-- §270 of the compiler contract. The index is `specs/blocks/compiler.md` §0. -->

## 270. The interop device ends its registrations when it is freed

*(Added 2026-11-13.)* Origin: a code review of 2026-11-13, finding R14.
Selected by the owner on 2026-11-13.

Problem, read at `baf82d60`. The native interop fixture
(`corpus/interop/interop.c`) is the example of the explicit callback
lifetime of §111. A device holds one-shot request registrations and one
subscription registration. The release helpers end a registration on a
normal completion and on a cancel. `subDeviceRelease` frees the device
when its count reaches 0, and does not end a registration that is still
open. If a script starts a request or a subscription and releases the
last reference to the device before the next pump, the Context keeps
the registration, and its userdata stays a collection root, until the
Context drops. The fixture header requires the adapter to end a
registration when no later call can occur, and the fixture does not do
it on this path.

### 270.1 Rules

1. Before `subDeviceRelease` frees a device, it ends each open
   registration of the device once, through the release helper of that
   kind: each one-shot slot and the subscription.
2. A registration that a normal completion or a cancel already ended is
   not ended again: the helpers clear the slot.
3. The callback of an ended registration does not run after the free.
4. The pump and an immediate start hold a reference to the device while
   they call a callback. A callback that releases the last reference
   does not free the device before the dispatch ends, so the fixture
   reads no freed device after the callback returns.
5. No corpus entry changes its output.

### 270.2 Acceptance

1. Red first, at the pin: a corpus interop test, or a fixture unit test,
   starts a one-shot request and a subscription, releases the last
   device reference before the pump, and reads the open registration
   count of the Context (the count that §111 tests read). At the pin
   the count is 2 after the release. The tracking note records it.
2. After the change, the count is 0. The release count of the device
   cannot be read after the free, so the test reads the Context count
   only.
3. A control: a request that completes before the release ends once,
   and the release ends nothing more.
4. Rule 4: a test releases the last reference to the device inside its
   request callback, and then pumps. The run ends with no read of the
   freed device, and the open registration count is 0.
5. No golden moves.

### 270.3 Phase Review

One fresh review of the implementation. No CRITICAL finding. The review
read each path that opens a registration: each one reads the Context of
the registration first, so the release helper does not return early for
an open registration.

1. MAJOR, realistic, fixed: a callback that releases the last reference
   to the device freed it during the pump, and the pump then read the
   freed device. The defect is older than this section. Rule 4 states
   the fix.
2. MINOR, realistic, fixed: a test of the two tiers could not fail at
   the pin, and it built a C program and ran a process. It is deleted.
3. MINOR, realistic, recorded: §270.4 item 1.
4. MINOR, fixed: acceptance item 2 asked for a count that does not
   exist after the free.
5. MINOR, contrived, recorded: §270.4 item 2.
6. MINOR, fixed: the order of items at the top of a test file.

### 270.4 Open

1. The control of acceptance item 3 cannot find a second release: a
   release of an ended registration returns 0 and changes nothing
   (§111 rule 5). Rule 2 rests on the code that clears each slot.
2. A device that outlives its Context with an open registration reads
   the freed Context when it is released. No script reaches it.
