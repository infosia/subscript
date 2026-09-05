// corpus: accept/a105-interop-recursive-string-pair-elements
// interpreter: no — calls the synthetic native interop library
// purpose: Rebuilds a collapsed constants pair as a scratch array whose entries each expand a string key view.
// exercises: recursive-boundary-lowering, string-field-struct-elements, scratch-element-array, programmable-stage-constants
// questions: Q13, C4
// tsc: accepts; js-comparable: no Q13: The host C boundary has no JavaScript shim.
export function main(): void {
  const constants: SGPUProbeConstantEntry[] = [
    new SGPUProbeConstantEntry("alpha", 125),
    new SGPUProbeConstantEntry("beta", 250),
  ];
  const stage: SGPUProbeProgrammableStage = new SGPUProbeProgrammableStage(constants, 6);
  print(`${subProbeProgrammableStageCheck(stage, 0)}`);
  print(`${subProbeProgrammableStageCheck(stage, 1)}`);
  print(`${subProbeProgrammableStageCheck(stage, 2)}`);
  print(`${subProbeProgrammableStageCheck(stage, 3)}`);
  print(`${subProbeProgrammableStageCheck(stage, 4)}`);
  print(`${subProbeProgrammableStageCheck(stage, 5)}`);
  print(`${subProbeProgrammableStageCheck(stage, 6)}`);
  print(`${subProbeProgrammableStageCheck(stage, 7)}`);
}
