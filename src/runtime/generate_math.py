"""Generate portable math IR from pinned musl sources, retaining license notices."""
import os
from pathlib import Path
import re
import subprocess
import tempfile

directory = Path(__file__).resolve().parent
clang = os.environ.get("TSUZURI_CLANG", "clang")
linker = os.environ.get("TSUZURI_LLVM_LINK", "llvm-link")
operations = "sin cos tan asin acos atan atan2 exp exp2 log log2 log10 pow cbrt hypot".split()
support = "__sin __cos __tan __rem_pio2 __rem_pio2_large __sindf __cosdf __tandf __rem_pio2f scalbn floor scalbnf floorf".split()
support += [f"__math_{operation}{suffix}" for operation in ["xflow", "uflow", "oflow", "divzero", "invalid"] for suffix in ["", "f"]]
support += "exp_data exp2f_data log_data log2_data logf_data log2f_data pow_data powf_data".split()
sources = [name + suffix for name in operations for suffix in ["", "f"]] + support
renames = {name + suffix: f"tz_math_{name}_f{bits}" for name in operations + ["sqrt", "fabs", "floor", "scalbn"] for suffix, bits in [("", 64), ("f", 32)]}
renames["atan2"] = "tz_math_original_atan2_f64"
for name in support:
    if name.startswith("__"):
        renames[name] = "tz_math_" + name.lstrip("_")
for name in ["__exp_data", "__exp2f_data", "__log_data", "__log2_data", "__logf_data", "__log2f_data", "__pow_log_data", "__powf_log2_data"]:
    renames[name] = "tz_math_" + name.lstrip("_")
flags = ["--target=x86_64-unknown-linux-gnu", "-std=c11", "-O1", "-ffreestanding", "-fno-builtin", "-fno-stack-protector", "-fno-fast-math", "-ffp-contract=off", "-fno-math-errno", "-fexcess-precision=standard", "-fvisibility=hidden", "-U__FP_FAST_FMA", "-U__FP_FAST_FMAF", "-I" + str(directory / "math-include"), "-I" + str(directory / "musl")]
flags += [f"-D{source}={target}" for source, target in renames.items()]
with tempfile.TemporaryDirectory(prefix="tsuzuri-math-") as temporary:
    objects = []
    for source in [directory / "math.c"] + [directory / "musl" / f"{name}.c" for name in sources]:
        output = Path(temporary) / (source.stem + ".bc")
        subprocess.run([clang, *flags, "-emit-llvm", "-c", str(source), "-o", str(output)], check=True)
        objects.append(str(output))
    text = subprocess.run([linker, "-S", *objects, "-o", "-"], check=True, capture_output=True, text=True).stdout
text = re.sub(r"^(source_filename|target |!llvm\.|; ModuleID).*?\n", "", text, flags=re.M)
text = re.sub(r' "target-[^"]+"="[^"]*"', "", text)
text = re.sub(r" captures\([^)]*\)", "", text)
text = re.sub(r" range\([^)]*\)", "", text)
text = re.sub(r" initializes\(\([^)]*\)\)", "", text)
for annotation in [" dead_on_unwind", " writable", " nocreateundeforpoison"]:
    text = text.replace(annotation, "")
text = text.replace("getelementptr inbounds nuw", "getelementptr inbounds").replace("getelementptr nuw", "getelementptr")
text = text.replace("or disjoint", "or").replace("zext nneg", "zext")
text = re.sub(r"trunc (?:nuw |nsw )+", "trunc ", text)
text = text.replace("define hidden ", "define internal ").replace("hidden constant", "internal constant")
numeric = (directory / "numeric.ll").read_text()
attribute_offset = max(map(int, re.findall(r"#(\d+)", numeric)), default=-1) + 1
metadata_offset = max(map(int, re.findall(r"!(\d+)", numeric)), default=-1) + 1
text = re.sub(r"#(\d+)", lambda match: "#" + str(int(match[1]) + attribute_offset), text)
text = re.sub(r"!(\d+)", lambda match: "!" + str(int(match[1]) + metadata_offset), text)
if re.search(r"\b(fast|reassoc|contract|afn|nnan|ninf|x86_fp80)\b|llvm\.fmuladd|llvm\.fma\.", text):
    raise RuntimeError("math IR uses unsupported precision or floating-point flags")
external = re.findall(r"^declare [^@]*@([^ (]+)", text, re.M)
if any(not name.startswith("llvm.") for name in external):
    raise RuntimeError(f"math runtime has unresolved external dependencies: {external}")
subprocess.run([clang, "--target=x86_64-unknown-linux-gnu", "-Wno-override-module", "-x", "ir", "-c", "-", "-o", os.devnull], input=text, text=True, check=True)
(directory / "math.ll").write_text("; Generated from math.c and musl 1.2.5 by generate_math.py. Do not edit.\n" + text)