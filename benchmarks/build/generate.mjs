import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const FUNCTIONS_PER_MODULE = 20;
export const BUILD_LANGUAGES = Object.freeze(["tsuzuri", "c", "cpp", "rust", "go", "zig"]);
const multiplier = 6364136223846793005n;
const wrap = (value) => BigInt.asUintN(64, value);
const range = (count) => Array.from({ length: count }, (_, index) => index);
const modulesValid = (modules) => assert.ok(Number.isSafeInteger(modules) && modules >= 1 && modules <= 5000, "modules must be in 1..5000");
function optionsValid(index, { salt = 0, extra = null, language = "tsuzuri" }) {
  assert.ok(Number.isSafeInteger(index) && index >= 0 && index < 5000, "module index must be in 0..4999");
  assert.ok(Number.isSafeInteger(salt) && salt >= 0, "invalid salt");
  assert.ok(extra === null || Number.isSafeInteger(extra) && extra >= 0, "invalid extra");
  assert.ok(BUILD_LANGUAGES.includes(language), "unsupported build language");
}
const constant = (index, fn, salt) => BigInt(index * FUNCTIONS_PER_MODULE + fn + 3) + (fn === 0 ? BigInt(salt) : 0n);

function tsuzuriModule(index, { salt, extra }) {
  const functions = range(FUNCTIONS_PER_MODULE).map((fn) => {
    const c = constant(index, fn, salt);
    const bodies = [
      `\n    let mut total = x\n    let mut index = 0i64u\n    while index < 8 do\n        total = total * ${multiplier} + index + ${c}\n        index = index + 1\n    total`,
      `\n    let mut total = x\n    for i in 0 ..= 7 do\n        total = total * 31 + (i as i64u) + ${c}\n    total`,
      `\n    match x % 4 with\n    | 0 -> x + ${c}\n    | 1 -> x * 3\n    | 2 -> x + ${c + 4n}\n    | _ -> x / 2`,
      `\n    let pair = Pair { left: x, right: x + ${c} }\n    pair.left * pair.right`,
      `\n    let shape = if x % 3 == 0 then Dot x else (if x % 3 == 1 then Span (x, x + ${c}) else Empty)\n    match shape with\n    | Dot value -> value\n    | Span (left, right) -> left * right\n    | Empty -> x + ${c + 2n}`,
      ` twice x + twice ${c}i64u`,
      `\n    let offset = x % ${c}\n    let add = \\value -> value + offset\n    add (add x)`,
      `\n    let text = "m${index}-" + to_string x\n    x + (String.length text as i64u)`,
      `\n    let values = new [i64u](8, \\i -> (i as i64u) * x + ${c})\n    let mut total = 0i64u\n    for value in values do\n        total = total + value\n    total`,
      ` count (x % 16) x`,
    ];
    return `private def f${fn} :: i64u -> i64u = \\x ->${bodies[fn % 10]}\n`;
  }).join("\n");
  return `private record Pair { left: i64u, right: i64u }\n
private union Shape =
    | Dot of i64u
    | Span of i64u * i64u
    | Empty

private def twice :: Add<'a> -> 'a = \\value -> value + value

private def rec count :: i64u -> i64u -> i64u = \\n acc -> if n == 0 then acc else count (n - 1) (acc + n)

${functions}
def run :: i64u -> i64u = \\seed ->
    let mut state = seed
${range(FUNCTIONS_PER_MODULE).map((fn) => `    state = f${fn} state`).join("\n")}
    state
${extra === null ? "" : `def extra_${extra} :: i64u -> i64u = \\x -> x + ${extra}i64u\n`}`;
}

function cModule(index, { salt, extra }) {
  const functions = range(FUNCTIONS_PER_MODULE).map((fn) => {
    const c = constant(index, fn, salt);
    const bodies = [
      `uint64_t total=x; uint64_t i=0; while(i<8) { total=total*UINT64_C(${multiplier})+i+UINT64_C(${c}); ++i; } return total;`,
      `uint64_t total=x; for(uint64_t i=0;i<8;i++) total=total*31+i+UINT64_C(${c}); return total;`,
      `switch(x%4) { case 0:return x+UINT64_C(${c}); case 1:return x*3; case 2:return x+UINT64_C(${c + 4n}); default:return x/2; }`,
      `struct Pair pair={x,x+UINT64_C(${c})}; return pair.left*pair.right;`,
      `struct Shape shape={0,{.dot=0}}; if(x%3==0) { shape.tag=0; shape.value.dot=x; } else if(x%3==1) { shape.tag=1; shape.value.span=(struct Pair){x,x+UINT64_C(${c})}; } else shape.tag=2; switch(shape.tag) { case 0:return shape.value.dot; case 1:return shape.value.span.left*shape.value.span.right; default:return x+UINT64_C(${c + 2n}); }`,
      `return twice(x)+twice(UINT64_C(${c}));`,
      `struct Capture capture={x%UINT64_C(${c})}; uint64_t (*add)(const struct Capture *,uint64_t)=add_value; return add(&capture,add(&capture,x));`,
      `char number[32]; int length=snprintf(number,sizeof(number),"%" PRIu64,x); if(length<0 || length>=(int)sizeof(number)) abort(); char *text=malloc(${String(index).length + 2}+(size_t)length+1); if(!text) abort(); memcpy(text,"m${index}-",${String(index).length + 2}); memcpy(text+${String(index).length + 2},number,(size_t)length+1); uint64_t result=x+(uint64_t)strlen(text); free(text); return result;`,
      `uint64_t *values=malloc(8*sizeof(uint64_t)); if(!values) abort(); for(uint64_t i=0;i<8;i++) values[i]=i*x+UINT64_C(${c}); uint64_t total=0; for(unsigned i=0;i<8;i++) total+=values[i]; free(values); return total;`,
      `return count(x%16,x);`,
    ];
    return `static uint64_t f${fn}(uint64_t x) { ${bodies[fn % 10]} }\n`;
  }).join("");
  return `#include "Module${index}.h"
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
struct Pair { uint64_t left,right; };
struct Shape { unsigned tag; union { uint64_t dot; struct Pair span; } value; };
struct Capture { uint64_t offset; };
static uint64_t add_value(const struct Capture *capture,uint64_t x) { return x+capture->offset; }
static uint64_t twice_value(uint64_t x) { return x+x; }
#define twice(value) _Generic((value),uint64_t:twice_value)(value)
static uint64_t count(uint64_t n,uint64_t acc) { return n==0 ? acc : count(n-1,acc+n); }
${functions}
uint64_t module${index}_run(uint64_t seed) {
    uint64_t state=seed;
${range(FUNCTIONS_PER_MODULE).map((fn) => `    state=f${fn}(state);`).join("\n")}
    return state;
}
${extra === null ? "" : `uint64_t module${index}_extra_${extra}(uint64_t x) { return x+UINT64_C(${extra}); }\n`}`;
}

function cppModule(index, { salt, extra }) {
  const functions = range(FUNCTIONS_PER_MODULE).map((fn) => {
    const c = constant(index, fn, salt);
    const bodies = [
      `uint64_t total=x; uint64_t i=0; while(i<8) { total=total*UINT64_C(${multiplier})+i+UINT64_C(${c}); ++i; } return total;`,
      `uint64_t total=x; for(uint64_t i=0;i<8;i++) total=total*31+i+UINT64_C(${c}); return total;`,
      `switch(x%4) { case 0:return x+UINT64_C(${c}); case 1:return x*3; case 2:return x+UINT64_C(${c + 4n}); default:return x/2; }`,
      `Pair pair{x,x+UINT64_C(${c})}; return pair.left*pair.right;`,
      `Shape shape; if(x%3==0) shape=Dot{x}; else if(x%3==1) shape=Pair{x,x+UINT64_C(${c})}; else shape=std::monostate{}; return std::visit([x](const auto &value)->uint64_t { using T=std::decay_t<decltype(value)>; if constexpr(std::is_same_v<T,Dot>) return value.value; else if constexpr(std::is_same_v<T,Pair>) return value.left*value.right; else return x+UINT64_C(${c + 2n}); },shape);`,
      `return twice(x)+twice(UINT64_C(${c}));`,
      `auto offset=x%UINT64_C(${c}); auto add=[offset](uint64_t value) { return value+offset; }; return add(add(x));`,
      `auto text=std::string("m${index}-")+std::to_string(x); return x+text.size();`,
      `std::vector<uint64_t> values(8); for(uint64_t i=0;i<8;i++) values[i]=i*x+UINT64_C(${c}); uint64_t total=0; for(auto value:values) total+=value; return total;`,
      `return count(x%16,x);`,
    ];
    return `static uint64_t f${fn}(uint64_t x) { ${bodies[fn % 10]} }\n`;
  }).join("");
  return `#include "Module${index}.h"
#include <string>
#include <type_traits>
#include <variant>
#include <vector>
namespace {
struct Pair { uint64_t left,right; };
struct Dot { uint64_t value; };
using Shape=std::variant<Dot,Pair,std::monostate>;
template<class T> static T twice(T value) { return value+value; }
static uint64_t count(uint64_t n,uint64_t acc) { return n==0 ? acc : count(n-1,acc+n); }
${functions}}
uint64_t module${index}_run(uint64_t seed) {
    uint64_t state=seed;
${range(FUNCTIONS_PER_MODULE).map((fn) => `    state=f${fn}(state);`).join("\n")}
    return state;
}
${extra === null ? "" : `uint64_t module${index}_extra_${extra}(uint64_t x) { return x+UINT64_C(${extra}); }\n`}`;
}

function rustModule(index, { salt, extra }) {
  const functions = range(FUNCTIONS_PER_MODULE).map((fn) => {
    const c = constant(index, fn, salt);
    const bodies = [
      `let mut total=x; let mut i=0u64; while i<8 { total=total.wrapping_mul(${multiplier}).wrapping_add(i).wrapping_add(${c}); i+=1; } total`,
      `let mut total=x; for i in 0..8u64 { total=total.wrapping_mul(31).wrapping_add(i).wrapping_add(${c}); } total`,
      `match x%4 { 0=>x.wrapping_add(${c}), 1=>x.wrapping_mul(3), 2=>x.wrapping_add(${c + 4n}), _=>x/2 }`,
      `let pair=Pair{left:x,right:x.wrapping_add(${c})}; pair.left.wrapping_mul(pair.right)`,
      `let shape=match x%3 { 0=>Shape::Dot(x), 1=>Shape::Span(x,x.wrapping_add(${c})), _=>Shape::Empty }; match shape { Shape::Dot(value)=>value, Shape::Span(left,right)=>left.wrapping_mul(right), Shape::Empty=>x.wrapping_add(${c + 2n}) }`,
      `twice(std::num::Wrapping(x)).0.wrapping_add(twice(std::num::Wrapping(${c}u64)).0)`,
      `let offset=x%${c}; let add=|value:u64| value.wrapping_add(offset); add(add(x))`,
      `let text=String::from("m${index}-")+&x.to_string(); x.wrapping_add(text.len() as u64)`,
      `let values:Vec<u64>=(0..8u64).map(|i| i.wrapping_mul(x).wrapping_add(${c})).collect(); let mut total=0u64; for value in values { total=total.wrapping_add(value); } total`,
      `count(x%16,x)`,
    ];
    return `fn f${fn}(x:u64)->u64 { ${bodies[fn % 10]} }\n`;
  }).join("");
  return `struct Pair { left:u64,right:u64 }
enum Shape { Dot(u64),Span(u64,u64),Empty }
fn twice<T:Copy+std::ops::Add<Output=T>>(value:T)->T { value+value }
fn count(n:u64,acc:u64)->u64 { if n==0 { acc } else { count(n-1,acc.wrapping_add(n)) } }
${functions}
pub fn run(seed:u64)->u64 {
    let mut state=seed;
${range(FUNCTIONS_PER_MODULE).map((fn) => `    state=f${fn}(state);`).join("\n")}
    state
}
${extra === null ? "" : `#[allow(dead_code)]\npub fn extra_${extra}(x:u64)->u64 { x.wrapping_add(${extra}) }\n`}`;
}

function goModule(index, { salt, extra }) {
  const functions = range(FUNCTIONS_PER_MODULE).map((fn) => {
    const c = constant(index, fn, salt);
    const bodies = [
      `total:=x; i:=uint64(0); for i<8 { total=total*${multiplier}+i+${c}; i++ }; return total`,
      `total:=x; for i:=uint64(0);i<8;i++ { total=total*31+i+${c} }; return total`,
      `switch x%4 { case 0:return x+${c}; case 1:return x*3; case 2:return x+${c + 4n}; default:return x/2 }`,
      `value:=pair{x,x+${c}}; return value.left*value.right`,
      `var value shape; switch x%3 { case 0:value=shape{0,x,0}; case 1:value=shape{1,x,x+${c}}; default:value=shape{2,0,0} }; switch value.tag { case 0:return value.left; case 1:return value.left*value.right; default:return x+${c + 2n} }`,
      `return twice(x)+twice(uint64(${c}))`,
      `offset:=x%${c}; add:=func(value uint64) uint64 { return value+offset }; return add(add(x))`,
      `text:="m${index}-"+strconv.FormatUint(x,10); return x+uint64(len(text))`,
      `values:=make([]uint64,8); for i:=uint64(0);i<8;i++ { values[i]=i*x+${c} }; total:=uint64(0); for _,value:=range values { total+=value }; return total`,
      `return count(x%16,x)`,
    ];
    return `func f${fn}(x uint64) uint64 { ${bodies[fn % 10]} }\n`;
  }).join("");
  return `package module${index}
import "strconv"
type pair struct { left,right uint64 }
type shape struct { tag uint8; left,right uint64 }
func twice[T ~uint64](value T) T { return value+value }
func count(n,acc uint64) uint64 { if n==0 { return acc }; return count(n-1,acc+n) }
${functions}
func Run(seed uint64) uint64 {
    state:=seed
${range(FUNCTIONS_PER_MODULE).map((fn) => `    state=f${fn}(state)`).join("\n")}
    return state
}
${extra === null ? "" : `func Extra_${extra}(x uint64) uint64 { return x+${extra} }\n`}`;
}

function zigModule(index, { salt, extra }) {
  const functions = range(FUNCTIONS_PER_MODULE).map((fn) => {
    const c = constant(index, fn, salt);
    const bodies = [
      `var total=x; var i:u64=0; while(i<8):(i+=1) { total=total*%${multiplier}+%i+%${c}; } return total;`,
      `var total=x; for(0..8) |i| { total=total*%31+%@as(u64,@intCast(i))+%${c}; } return total;`,
      `return switch(x%4) { 0=>x+%${c}, 1=>x*%3, 2=>x+%${c + 4n}, else=>x/2 };`,
      `const pair=Pair{.left=x,.right=x+%${c}}; return pair.left*%pair.right;`,
      `const shape:Shape=switch(x%3) { 0=>.{.dot=x}, 1=>.{.span=.{.left=x,.right=x+%${c}}}, else=>.empty }; return switch(shape) { .dot=>|value| value, .span=>|pair| pair.left*%pair.right, .empty=>x+%${c + 2n} };`,
      `return twice(u64,x)+%twice(u64,${c});`,
      `const add=Capture{.offset=x%${c}}; return add.apply(add.apply(x));`,
      `const text=std.fmt.allocPrint(allocator,"m${index}-{d}",.{x}) catch @panic("allocation failed"); defer allocator.free(text); return x+%@as(u64,@intCast(text.len));`,
      `const values=allocator.alloc(u64,8) catch @panic("allocation failed"); defer allocator.free(values); for(values,0..) |*value,i| value.*=@as(u64,@intCast(i))*%x+%${c}; var total:u64=0; for(values) |value| total+%=value; return total;`,
      `return count(x%16,x);`,
    ];
    return `fn @"f${fn}"(x:u64) u64 { ${bodies[fn % 10]} }\n`;
  }).join("");
  return `const std=@import("std");
const allocator=std.heap.c_allocator;
const Pair=struct { left:u64,right:u64 };
const Shape=union(enum) { dot:u64,span:Pair,empty };
const Capture=struct { offset:u64,fn apply(self:@This(),value:u64) u64 { return value+%self.offset; } };
fn twice(comptime T:type,value:T) T { return value+%value; }
fn count(n:u64,acc:u64) u64 { return if(n==0) acc else count(n-1,acc+%n); }
${functions}
pub fn run(seed:u64) u64 {
    var state=seed;
${range(FUNCTIONS_PER_MODULE).map((fn) => `    state=@"f${fn}"(state);`).join("\n")}
    return state;
}
${extra === null ? "" : `pub fn extra_${extra}(x:u64) u64 { return x+%${extra}; }\n`}`;
}

export function moduleSource(index, { salt = 0, extra = null, language = "tsuzuri" } = {}) {
  optionsValid(index, { salt, extra, language });
  return { tsuzuri: tsuzuriModule, c: cModule, cpp: cppModule, rust: rustModule, go: goModule, zig: zigModule }[language](index, { salt, extra });
}

const utf8ReaderC = `static uint64_t read_seed(void) {
    uint64_t units=0; int previous=-1;
    for(;;) {
        int value=getchar();
        if(value==EOF || value=='\\n') break;
        if((value&0xc0)!=0x80) units+=(value>=0xf0 && value<0xf8) ? 2 : 1;
        previous=value;
    }
    if(previous=='\\r') --units;
    if(ferror(stdin)) { fputs("stdin read failed\\n",stderr); exit(1); }
    return units;
}\n`;

export function mainSource(modules, { language = "tsuzuri", rustLayout = "single" } = {}) {
  modulesValid(modules);
  assert.ok(BUILD_LANGUAGES.includes(language));
  assert.ok(["single", "workspace"].includes(rustLayout));
  if (language === "tsuzuri") return `private def total :: i64u -> i64u = \\seed ->
    let mut sum = 0i64u
${range(modules).map((index) => `    sum = sum + Module${index}.run seed`).join("\n")}
    sum

def main :: unit -> i32 = \\() ->
    let! line = IO.read_line ()
    let text = Maybe.default_value "" line
    let seed = String.length text as i64u
    do! IO.write_line (to_string (total seed))
    0
`;
  if (language === "c" || language === "cpp") return `#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
${range(modules).map((index) => `#include "Module${index}.h"`).join("\n")}
${utf8ReaderC}
int main(void) {
    uint64_t seed=read_seed(),sum=0;
${range(modules).map((index) => `    sum+=module${index}_run(seed);`).join("\n")}
    if(printf("%" PRIu64 "\\n",sum)<0) return 1;
    return 0;
}\n`;
  if (language === "rust") return `use std::io::{self,BufRead};
${rustLayout === "single" ? range(modules).map((index) => `mod module${index};`).join("\n") : ""}
fn main() {
    let mut line=String::new();
    io::stdin().lock().read_line(&mut line).expect("stdin read failed");
    if line.ends_with('\\n') { line.pop(); }
    if line.ends_with('\\r') { line.pop(); }
    let seed=line.encode_utf16().count() as u64;
    let mut sum=0u64;
${range(modules).map((index) => `    sum=sum.wrapping_add(module${index}::run(seed));`).join("\n")}
    println!("{sum}");
}\n`;
  if (language === "go") return `package main
import (
    "bufio"
    "fmt"
    "io"
    "os"
    "strings"
    "unicode/utf16"
${range(modules).map((index) => `    "tsuzuri-build-bench/module${index}"`).join("\n")}
)
func main() {
    line,err:=bufio.NewReader(os.Stdin).ReadString('\\n')
    if err!=nil && err!=io.EOF { panic(err) }
    line=strings.TrimSuffix(strings.TrimSuffix(line,"\\n"),"\\r")
    seed:=uint64(len(utf16.Encode([]rune(line))))
    sum:=uint64(0)
${range(modules).map((index) => `    sum+=module${index}.Run(seed)`).join("\n")}
    if _,err:=fmt.Println(sum);err!=nil { panic(err) }
}\n`;
  return `const c=@cImport({ @cInclude("stdio.h"); });
${range(modules).map((index) => `const module${index}=@import("Module${index}.zig");`).join("\n")}
pub fn main() void {
    var seed:u64=0;
    var previous:c_int=-1;
    while(true) {
        const value=c.getchar();
        if(value==c.EOF or value==10) break;
        if((value&0xc0)!=0x80) seed+%=if(value>=0xf0 and value<0xf8) 2 else 1;
        previous=value;
    }
    if(previous==13) seed-%=1;
    const input=if(@typeInfo(@TypeOf(c.stdin))==.@"fn") c.stdin() else c.stdin;
    if(c.ferror(input)!=0) @panic("stdin read failed");
    var sum:u64=0;
${range(modules).map((index) => `    sum+%=module${index}.run(seed);`).join("\n")}
    if(c.printf("%llu\\n",@as(c_ulonglong,sum))<0) @panic("stdout write failed");
}\n`;
}

export function moduleFiles(index, { language = "tsuzuri", rustLayout = "single", salt = 0, extra = null } = {}) {
  const source = moduleSource(index, { language, salt, extra });
  if (language === "tsuzuri") return [[`Module${index}.tz`, source]];
  if (language === "c" || language === "cpp") return [
    [`Module${index}.${language === "c" ? "c" : "cpp"}`, source],
    [`Module${index}.h`, `#pragma once\n#include <stdint.h>\nuint64_t module${index}_run(uint64_t);\n${extra === null ? "" : `uint64_t module${index}_extra_${extra}(uint64_t);\n`}`],
  ];
  if (language === "rust") return [[rustLayout === "workspace" ? `module${index}/src/lib.rs` : `src/module${index}.rs`, source]];
  if (language === "go") return [[`module${index}/module.go`, source]];
  return [[`Module${index}.zig`, source]];
}

export function generateProject(modules, { language = "tsuzuri", rustLayout = "single" } = {}) {
  modulesValid(modules);
  assert.ok(BUILD_LANGUAGES.includes(language));
  assert.ok(["single", "workspace"].includes(rustLayout));
  const files = range(modules).flatMap((index) => moduleFiles(index, { language, rustLayout }));
  const main = mainSource(modules, { language, rustLayout });
  if (language === "tsuzuri") files.push(["Main.tz", main]);
  else if (language === "c" || language === "cpp") {
    const extension = language === "c" ? "c" : "cpp";
    files.push([`Main.${extension}`, main], ["Makefile", `CC ?= clang
OPT ?= -O0
FLAGS = $(OPT) -fno-fast-math -ffp-contract=off -fno-lto -Wall -Wextra -Werror -std=${language === "c" ? "c11" : "c++20"}
SOURCES = Main.${extension} ${range(modules).map((index) => `Module${index}.${extension}`).join(" ")}
OBJECTS = $(SOURCES:${extension}=o)
.PHONY: all clean
all: app
app: $(OBJECTS)
\t"$(CC)" ${language === "cpp" ? "--driver-mode=g++ " : ""}$(FLAGS) $(OBJECTS) -o $@
%.o: %.${extension}
\t"$(CC)" ${language === "cpp" ? "--driver-mode=g++ " : ""}$(FLAGS) -MMD -MP -c $< -o $@
-include $(OBJECTS:.o=.d)
clean:
\trm -f app $(OBJECTS) $(OBJECTS:.o=.d)
`]);
  } else if (language === "rust") {
    files.push(["src/main.rs", main]);
    const dependencies = rustLayout === "workspace" ? `[dependencies]\n${range(modules).map((index) => `module${index} = { path = "module${index}" }`).join("\n")}\n` : "";
    const workspace = rustLayout === "workspace" ? `[workspace]\nresolver = "2"\nmembers = [${range(modules).map((index) => `"module${index}"`).join(",")}]\n` : "";
    files.push(["Cargo.toml", `[package]\nname = "tsuzuri-build-bench"\nversion = "0.0.0"\nedition = "2021"\n${dependencies}${workspace}[profile.dev]\npanic = "abort"\n[profile.release]\npanic = "abort"\nlto = false\n`]);
    if (rustLayout === "workspace") for (const index of range(modules)) files.push([`module${index}/Cargo.toml`,
      `[package]\nname = "module${index}"\nversion = "0.0.0"\nedition = "2021"\n`]);
  } else if (language === "go") files.push(["main.go", main], ["go.mod", "module tsuzuri-build-bench\n\ngo 1.24\n"]);
  else files.push(["Main.zig", main]);
  return files.sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
}

export function writeProject(directory, modules, options = {}) {
  const files = generateProject(modules, options);
  mkdirSync(directory);
  for (const [name, text] of files) {
    mkdirSync(dirname(join(directory, name)), { recursive: true });
    writeFileSync(join(directory, name), text, { flag: "wx" });
  }
  return { modules, files: files.length, lines: files.reduce((sum, [, text]) => sum + (text.match(/\n/g) ?? []).length, 0),
    bytes: files.reduce((sum, [, text]) => sum + Buffer.byteLength(text), 0) };
}

export function expectedOutput(modules, line, { index = null, salt = 0 } = {}) {
  modulesValid(modules);
  assert.equal(typeof line, "string");
  assert.ok(index === null || Number.isSafeInteger(index) && index >= 0 && index < modules, "invalid edited module");
  assert.ok(Number.isSafeInteger(salt) && salt >= 0, "invalid salt");
  let total = 0n;
  for (let module = 0; module < modules; module++) {
    let x = BigInt(line.length);
    for (let fn = 0; fn < FUNCTIONS_PER_MODULE; fn++) {
      const c = constant(module, fn, module === index ? salt : 0);
      switch (fn % 10) {
        case 0: for (let i = 0n; i < 8n; i++) x = wrap(x * multiplier + i + c); break;
        case 1: for (let i = 0n; i < 8n; i++) x = wrap(x * 31n + i + c); break;
        case 2: x = [x + c, x * 3n, x + c + 4n, x / 2n][Number(x % 4n)]; break;
        case 3: x = x * (x + c); break;
        case 4: x = [x, x * (x + c), x + c + 2n][Number(x % 3n)]; break;
        case 5: x = 2n * x + 2n * c; break;
        case 6: x += 2n * (x % c); break;
        case 7: x += BigInt(`m${module}-`.length + x.toString().length); break;
        case 8: x = 28n * x + 8n * c; break;
        case 9: { const n = x % 16n; x += n * (n + 1n) / 2n; break; }
      }
      x = wrap(x);
    }
    total = wrap(total + x);
  }
  return `${total}\n`;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const usage = "Usage: node benchmarks/build/generate.mjs --modules N --out dir [--language tsuzuri|c|cpp|rust|go|zig] [--rust-layout single|workspace]";
  const options = {};
  const args = process.argv.slice(2);
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index], value = args[index + 1];
    assert.ok(["--modules", "--out", "--language", "--rust-layout"].includes(flag) && value && !value.startsWith("-") && options[flag] === undefined, usage);
    options[flag] = value;
  }
  assert.ok(options["--modules"] && options["--out"], usage);
  if (options["--rust-layout"]) assert.equal(options["--language"], "rust", usage);
  console.log(JSON.stringify(writeProject(resolve(options["--out"]), Number(options["--modules"]),
    { language: options["--language"] ?? "tsuzuri", rustLayout: options["--rust-layout"] ?? "single" })));
}
