// corpus: accept/a147-switch-body-scope
// purpose: Exercises one switch-body scope without a cross-case read.
// exercises: switch-body-scope, distinct-case-declarations, fallthrough
// questions: §67
export function main(): void {
  const selected: i32 = 2;
  switch (selected) {
    case 0:
      const zeroText: string = "case0";
      print(zeroText);
      break;
    case 1:
      const oneText: string = "case1";
      print(oneText);
      break;
    case 2:
      const fallthroughText: string = "case2";
      print(fallthroughText);
    default:
      const defaultText: string = "default";
      print(defaultText);
      break;
  }
}
// tsc: accepts; js-comparable: yes
