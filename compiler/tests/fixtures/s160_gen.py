# Generate the §160 checker cost programs in the output directory.
import argparse
from pathlib import Path

parser = argparse.ArgumentParser(description="Generate the §160 checker cost programs.")
parser.add_argument("directory", type=Path)
d = parser.parse_args().directory
d.mkdir(parents=True, exist_ok=True)
def gen(n, ann):
    T=lambda t: (": "+t) if ann else ""
    out=[]
    for i in range(n):
        out.append(f"const K{i}{T('i32')} = {i};")
        out.append(f"class C{i} {{ a{T('i32')} = {i}; b{T('f64')} = 1.5; s{T('string')} = \"x\"; xs{T('i32[]')} = [1, 2, 3];")
        out.append(f"  m(k{T('i32')} = 2, j: i32): i32 {{ let t: i32 = 0; for (let q = 0; q < j; q++) {{ t += q * k; }} if (t > 3) {{ t = t - 1; }} else {{ t = t + 1; }} return t + this.a + this.xs.length; }}")
        out.append(f"  n(): i32 {{ const r = this.xs.map((v: i32): i32 => v * 2); return r[0] + K{i}; }} }}")
        out.append(f"function fn{i}(x: i32, y{T('i32')} = {i}): i32 {{ const c = new C{i}(); return c.m(x, y) + c.n(); }}")
    out.append("export function main(): void { let s: i32 = 0;")
    for i in range(0,n,max(1,n//50)): out.append(f"  s += fn{i}(1);")
    out.append("  print(`${s}`); }")
    return "\n".join(out)+"\n"
for n in (200,1000,3000):
    (d / f"ann{n}.ts").write_text(gen(n, True), encoding="utf-8")
    (d / f"inf{n}.ts").write_text(gen(n, False), encoding="utf-8")

# Annotated fields use the body checker without a type decision.
for reads in (False, True):
    shape = "reads" if reads else "constants"
    fields = ("a: i32 = 1; b: i32 = this.a + 1; "
              "c: i32 = this.b + 1; d: i32 = this.c + 1;" if reads else
              "a: i32 = 1; b: i32 = 2; c: i32 = 3; d: i32 = 4;")
    text = "\n".join(f"class C{i} {{ {fields} }}" for i in range(6000)) + "\n"
    (d / f"fields-{shape}6000.ts").write_text(text, encoding="utf-8")

# One long function compares initialized and uninitialized local flow.
for n in (1000, 2000, 4000):
    for shape in ("if", "switch"):
        for initialized in (False, True):
            out = ["export function main(flag: i32): void {"]
            for i in range(n):
                out.append(f"let v{i}: i32" + (" = 0;" if initialized else ";"))
                if shape == "if":
                    out.append(f"if (flag > 0) {{ v{i} = 1; }} else {{ v{i} = 2; }}")
                else:
                    out.append(f"switch (flag) {{ case 0: v{i} = 1; break; default: v{i} = 2; break; }}")
                out.append(f"print(`${{v{i}}}`);")
            out.append("}")
            suffix = "initialized" if initialized else "uninitialized"
            (d / f"locals-{shape}-{suffix}{n}.ts").write_text("\n".join(out) + "\n", encoding="utf-8")
