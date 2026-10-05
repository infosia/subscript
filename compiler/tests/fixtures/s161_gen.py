"""Generate the §161 checker cost programs."""
import argparse
from pathlib import Path


def generate(form, n):
    out = ["class P { a: i32 = 1; b: i32 = 2; p: P | null = null; }",
           "function touch(): void { }"]
    if form in ("generic", "generic_i32"):
        ty = "T" if form == "generic" else "i32"
        out.append("function ident<T>(a: T): T {")
        for i in range(n):
            value = "a" if form == "generic" else "1"
            out.append(f"const x{i}: {ty} = {value};")
        out.append("return a; }")
        call = "ident<P>(new P()).a"
        out.append(f"export function main(): void {{ print(`${{{call}}}`); }}")
    elif form == "blocks":
        for i in range(n):
            out.append(f"function block{i}(root: P): i32 {{ let s: i32 = 0;")
            for j in range(8):
                out.extend([f"const o{j} = new P();",
                            f"if (o{j}.a > 0) {{ s = s + o{j}.b; }}", "touch();",
                            f"o{j}.a = s;"])
            out.extend(["const p = root.p;",
                        "if (p !== null) { s = s + p.b; }",
                        "for (let k: i32 = 0; k < 2; k++) { s = s + k; }",
                        "root.a = s;", "touch();", "return s; }"])
        out.append("export function main(): void { print(`${block0(new P())}`); }")
    else:
        out.append("export function main(): void { let s: i32 = 0;")
        if form == "same":
            out.append("const o = new P();")
        if form == "narrowed":
            out.extend(["const root = new P(); const p = root.p;", "if (p !== null) {"])
        for i in range(n):
            if form == "narrowed":
                out.extend(["s = s + p.b;", "touch();"])
            if form != "same":
                out.append(f"const o{i} = new P();")
            name = "o" if form == "same" else f"o{i}"
            out.extend([f"if ({name}.a > 0) {{ s = s + {name}.b; }}", "touch();"])
        if form == "narrowed":
            out.append("}")
        out.append("print(`${s}`); }")
    return "\n".join(out) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--sizes", type=int, nargs="+", default=[1000, 2000, 4000])
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=True)
    for n in args.sizes:
        for form in ("distinct", "narrowed", "blocks", "same", "generic", "generic_i32"):
            (args.directory / f"{form}{n}.ts").write_text(generate(form, n), encoding="utf-8")


if __name__ == "__main__":
    main()
