"""Regenerate the size-class allocator's IR (heap-small.ll) from heap-small.c using Clang; no build-time C dependency.

The text is embedded next to the hand-written heap-small-native.ll, which defines @tz.alloc, @tz.free, and @tz.realloc
over it. It has no attribute groups and no metadata, so it cannot collide with numeric.ll, math.ll, or debug
information, and it uses no IR syntax newer than the one numeric.ll uses.
"""
import os
from pathlib import Path
import re
import subprocess

directory = Path(__file__).resolve().parent
clang = os.environ.get("TSUZURI_CLANG", "clang")

# Defined in the module, so that LLVM can inline them and drop what a program does not use.
internal = ["tz_small_alloc", "tz_small_free", "tz_small_realloc"]
# Shared between the objects of one program; the linker keeps one definition of each.
shared_functions = ["tz_small_stats"]
shared_globals = ["tz_small_shared"]
# libc, and the intrinsics that the C builtins become.
allowed_declarations = {"malloc", "realloc", "free"}
kept_attributes = ("noinline", "nounwind")

result = subprocess.run(
    [clang, "--target=x86_64-unknown-linux-gnu", "-std=c11", "-O1", "-ffreestanding", "-fno-builtin",
     "-fno-stack-protector", "-fno-strict-aliasing", "-fno-unwind-tables", "-fno-asynchronous-unwind-tables",
     "-S", "-emit-llvm", str(directory / "heap-small.c"), "-o", "-"],
    check=True, capture_output=True, text=True,
)
text = result.stdout

groups = dict(re.findall(r"^attributes #(\d+) = \{(.*)\}$", text, flags=re.M))


def inline_attributes(line):
    if line.startswith("attributes"):
        return line
    # An intrinsic's attributes are its own; a call or a definition keeps only what the optimizer cannot infer.
    wanted = [] if line.startswith("declare") and "@llvm." in line else None

    def expand(match):
        names = wanted if wanted is not None else [a for a in kept_attributes if re.search(rf"\b{a}\b", groups[match[1]])]
        return "".join(" " + name for name in names)

    return re.sub(r" #(\d+)(?=[ {]|$)", expand, line)


text = "\n".join(inline_attributes(line) for line in text.split("\n"))
text = re.sub(r"^(source_filename|target |attributes #|!|; ModuleID|; Function Attrs).*?\n", "", text, flags=re.M)
# The C library's allocator never unwinds.
text = re.sub(rf"^(declare [^@]*@(?:{'|'.join(sorted(allowed_declarations))})\(.*?)$", r"\1 nounwind", text, flags=re.M)
# Instruction metadata: loop hints, branch weights, and the like.
text = re.sub(r"(?:,\s*![A-Za-z_.]+\s+!\d+)+(?=\s*(?:;.*)?$)", "", text, flags=re.M)
text = re.sub(r' "target-[^"]+"="[^"]*"', "", text)
text = re.sub(r" captures\([^)]*\)", "", text)
text = re.sub(r" range\([^)]*\)", "", text)
text = re.sub(r" initializes\(\([^)]*\)\)", "", text)
text = text.replace(" dead_on_unwind", "").replace(" writable", "")
text = text.replace("getelementptr inbounds nuw", "getelementptr inbounds")
text = text.replace("getelementptr nuw", "getelementptr")
text = text.replace("or disjoint", "or").replace("zext nneg", "zext").replace("icmp samesign", "icmp")
text = re.sub(r"trunc (?:nuw |nsw )+", "trunc ", text)
for name in internal:
    text = text.replace(f"define hidden ptr @{name}(", f"define internal ptr @{name}(")
    text = text.replace(f"define hidden void @{name}(", f"define internal void @{name}(")
for name in shared_functions:
    text = re.sub(rf"define hidden ((?:ptr|void|i\d+) @{name}\()", r"define weak hidden \1", text)
for name in shared_globals:
    text = re.sub(rf"^(@{name} = )hidden (?:local_unnamed_addr )?global", r"\1weak hidden global", text, flags=re.M)
text = re.sub(r"\n{3,}", "\n\n", text).strip() + "\n"

# What the embedding relies on.
if re.search(r"#\d|(?<![\w\"])![A-Za-z0-9_.]", text):
    raise RuntimeError("heap-small.ll still has attribute groups or metadata")
if re.search(r"\b(samesign|nneg|disjoint|captures|range|initializes)\b|inbounds nuw", text):
    raise RuntimeError("heap-small.ll uses IR syntax that older LLVM versions do not read")
defined = re.findall(r"^define (?:[a-z]+ )*[^@]*@([^ (]+)\(", text, flags=re.M)
for name in internal + shared_functions:
    if name not in defined:
        raise RuntimeError(f"heap-small.ll does not define {name}")
if any(not re.search(rf"^define (?:internal|weak hidden) .*@{re.escape(name)}\(", text, flags=re.M) for name in defined):
    raise RuntimeError(f"unexpected linkage among {defined}")
declared = re.findall(r"^declare [^@]*@([^ (]+)", text, flags=re.M)
if any(name not in allowed_declarations and not name.startswith("llvm.") for name in declared):
    raise RuntimeError(f"heap-small.ll has unresolved external dependencies: {declared}")
if len(re.findall(r"\bthread_local\b", text)) != 1 or "@tz_small_shared = weak hidden global" not in text:
    raise RuntimeError("heap-small.ll must have one thread-local variable and the shared pointer")
if re.search(r"\b(fast|reassoc|contract|afn|nnan|ninf)\b|x86_fp80|x86_|target-cpu", text):
    raise RuntimeError("heap-small.ll uses a target feature or a floating-point flag")

# It must assemble for every native target; a compiler without a target is skipped.
for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-apple-darwin",
               "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"]:
    check = subprocess.run([clang, f"--target={target}", "-O2", "-Wno-override-module", "-x", "ir", "-c", "-", "-o", os.devnull],
                           input=text, text=True, capture_output=True)
    if check.returncode and "No available targets" not in check.stderr and "unknown target" not in check.stderr:
        raise RuntimeError(f"heap-small.ll does not compile for {target}:\n{check.stderr}")

(directory / "heap-small.ll").write_text("; Generated from heap-small.c by generate_heap.py. Do not edit.\n" + text)
