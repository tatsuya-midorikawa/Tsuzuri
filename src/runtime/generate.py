"""Regenerate target-neutral numeric IR using Clang; no build-time C dependency."""
import os
from pathlib import Path
import re
import subprocess

directory = Path(__file__).resolve().parent
result = subprocess.run(
    [os.environ.get("TSUZURI_CLANG", "clang"), "--target=x86_64-unknown-linux-gnu",
     "-std=c11", "-O1", "-ffreestanding", "-fno-builtin", "-fno-stack-protector",
     "-S", "-emit-llvm", str(directory / "numeric.c"), "-o", "-"],
    check=True, capture_output=True, text=True,
)
text = result.stdout
text = re.sub(r"^(source_filename|target |!llvm\.|; ModuleID).*?\n", "", text, flags=re.M)
text = re.sub(r' "target-[^"]+"="[^"]*"', "", text)
text = re.sub(r" captures\([^)]*\)", "", text)
text = re.sub(r" range\([^)]*\)", "", text)
text = re.sub(r" initializes\(\([^)]*\)\)", "", text)
text = text.replace(" dead_on_unwind", "").replace(" writable", "")
text = text.replace("getelementptr inbounds nuw", "getelementptr inbounds")
text = text.replace("getelementptr nuw", "getelementptr")
text = text.replace("or disjoint", "or").replace("zext nneg", "zext")
text = re.sub(r"trunc (?:nuw |nsw )+", "trunc ", text)
text = text.replace("define hidden ", "define weak hidden ")
text = re.sub(r"!llvm\.loop !\d+", "", text)
text = re.sub(r",\s*\n", "\n", text)
text = "; Generated from numeric.c by generate.py. Do not edit.\n" + text
(directory / "numeric.ll").write_text(text)
