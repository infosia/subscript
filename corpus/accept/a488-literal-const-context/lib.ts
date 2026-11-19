// corpus: accept/a488-literal-const-context/lib
// purpose: An imported const carries its literal.
// exercises: const, import
// questions: compiler.md §339 rule 1
// tsc: accepts
// js-comparable: yes
export const imported = "b.txt";
