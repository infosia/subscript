# Vendor patches

`regress-budget.patch` applies to `regress` 0.10.4 from
<https://github.com/ridiculousfish/regress>. It adds a per-call matching
budget, reports exhaustion separately from no match, and accounts for
optimized single-character scan loops.

Apply it from the root of a pristine 0.10.4 checkout:

```sh
git apply regress-budget.patch
```
