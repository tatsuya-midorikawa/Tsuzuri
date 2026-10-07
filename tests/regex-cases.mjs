// The regex cases of the `regex` suite in tests/features.mjs: the single source of truth for the patterns
// and inputs of tests/fixtures/regex/Cases.tz, which `node tests/regex-cases.mjs --write` regenerates.
// Expected values are computed here with V8 (`u` flag) or written by hand from the D09 specification;
// tests/features.mjs never copies the compiler's output.
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// JavaScript spellings of the constructs whose meaning differs from V8's.
const W = "[\\p{Alphabetic}\\p{M}\\p{Nd}\\p{Pc}\\p{Join_Control}]";
const NW = "[^\\p{Alphabetic}\\p{M}\\p{Nd}\\p{Pc}\\p{Join_Control}]";
const D = "\\p{Nd}";
const S = "\\p{White_Space}";
const B = `(?:(?<=${W})(?!${W})|(?<!${W})(?=${W}))`;
const NB = `(?:(?<=${W})(?=${W})|(?<!${W})(?!${W}))`;
const DOT = "[^\\n]";
const BOL = "(?<![^\\n])";
const EOL = "(?![^\\n])";
const BOT = "(?<![\\s\\S])";
const EOT = "(?![\\s\\S])";
const AW = "[0-9A-Za-z_]";

// Matching cases. `js` is V8's spelling (default: the pattern) and `flags` its flags besides `u`. `starts`
// lists the `find_at` offsets; the ones that are not scalar boundaries expect `None`. `captures` gives
// hand-computed group ranges where V8 differs (groups inside repetitions reset per iteration in V8).
export const matchCases = [
  { pattern: "a|ab", input: "ab" },
  { pattern: ".", js: DOT, input: "\u{1F600}", starts: [0, 1, 2, 3, -1] },
  { pattern: ".", js: DOT, input: "\uD800" },
  { pattern: "(?i)ß", js: "ß", flags: "i", input: "\u1E9E" },
  { pattern: "(?i)k", js: "k", flags: "i", input: "\u212A" },
  { pattern: "\\w", js: W, input: "é" },
  { pattern: "(?-u)\\w", js: AW, input: "é" },
  { pattern: "a*", input: "abc", starts: [0, 1, 2, 3, 4] },
  { pattern: ",\\s*", js: `,${S}*`, input: "a, b,c" },
  { pattern: "(\\w+)@(\\w+)", js: `(${W}+)@(${W}+)`, input: "x@y", replacement: "${2} at ${1}" },
  { pattern: "ab|a", input: "ab" },
  { pattern: "(a|ab)(c|bcd)", input: "abcd" },
  { pattern: "(a|ab)(c|bcd)(d*)", input: "abcd" },
  { pattern: "^a", input: "ba\na", starts: [0, 1, 3] },
  { pattern: "(?m)^a", js: `${BOL}a`, input: "ba\na\r\na\u2028a", starts: [0, 2, 3, 4] },
  { pattern: "a$", input: "a\na" },
  { pattern: "a$", input: "a\n" },
  { pattern: "(?m)a$", js: `a${EOL}`, input: "a\na\r\na\u2028a" },
  { pattern: "\\Aa", js: `${BOT}a`, input: "aa", starts: [0, 1] },
  { pattern: "a\\z", js: `a${EOT}`, input: "aa\n" },
  { pattern: "(?m)\\Aa|b\\z", js: `${BOT}a|b${EOT}`, input: "a\nb\na\nb" },
  { pattern: "(?m)^$", js: `${BOL}${EOL}`, input: "\n\nx\n" },
  { pattern: "^$", input: "" },
  { pattern: "a.c", js: `a${DOT}c`, input: "abc a\nc a\rc" },
  { pattern: "(?s)a.c", js: "a[\\s\\S]c", input: "abc a\nc a\rc" },
  { pattern: "a(?s:.)c", js: "a[\\s\\S]c", input: "a\nc" },
  { pattern: "a*?", input: "aaa" },
  { pattern: "a+?", input: "aaa" },
  { pattern: "a??b", input: "aab" },
  { pattern: "a{2,3}?", input: "aaaa" },
  { pattern: "<.+?>", js: `<${DOT}+?>`, input: "<a><b>" },
  { pattern: "<.+>", js: `<${DOT}+>`, input: "<a><b>" },
  { pattern: "a{0}b", input: "ab" },
  { pattern: "a{1}", input: "aa" },
  { pattern: "a{0,1}", input: "aa" },
  { pattern: "a{2,}", input: "a aa aaaa" },
  { pattern: "a{1000}", input: "a".repeat(999) + "b" + "a".repeat(1001) },
  { pattern: "(?:ab){3,5}", input: "abababababab" },
  { pattern: "", input: "abc" },
  { pattern: "", input: "" },
  { pattern: "a|", input: "bab" },
  { pattern: "|a", input: "bab" },
  { pattern: "()", input: "ab" },
  { pattern: "(a)|(b)", input: "xb" },
  { pattern: "[-a]+", input: "x-a-y" },
  { pattern: "[a-]+", input: "x-a-y" },
  { pattern: "[^a]", input: "a\uD800b" },
  { pattern: "[^a]+", input: "aa\uDC00\uD83D\uDE00" },
  { pattern: "\\u{10FFFF}", input: "a\u{10FFFF}" },
  { pattern: "\\uD83D\\uDE00", input: "x\u{1F600}" },
  { pattern: "\\uD83D", input: "\uD83D\u{1F600}" },
  { pattern: "[\\u{1F600}-\\u{1F64F}]+", input: "a\u{1F601}\u{1F64F}\u{1F650}" },
  { pattern: "(?i)\\u{1E9E}", js: "\\u{1E9E}", flags: "i", input: "xß" },
  { pattern: "(?i)K", js: "K", flags: "i", input: "\u212Ak" },
  { pattern: "(?i)\u03C2", js: "\u03C2", flags: "i", input: "\u03A3\u03C3" },
  { pattern: "(?i)\u0345", js: "\u0345", flags: "i", input: "\u0399\u03B9\u1FBE" },
  { pattern: "(?i)[a-z]+", js: "[a-z]+", flags: "i", input: "Hello WORLD \u212A" },
  { pattern: "(?i)[^a-z]+", js: "[^a-z]+", flags: "i", input: "aBc12\u212A" },
  { pattern: "(?i-u)k", js: "[Kk]", input: "\u212AkK" },
  { pattern: "(?i-u)[a-z]+", js: "[A-Za-z]+", input: "Hello\u212A" },
  { pattern: "(?i)straße", js: "straße", flags: "i", input: "STRASSE Strasse STRA\u1E9EE" },
  { pattern: "a(?i:b)c", js: "a[bB]c", input: "abc aBc ABC abC" },
  { pattern: "(?i)a(?-i:b)c", js: "[aA]b[cC]", input: "AbC aBc" },
  { pattern: "\\d+", js: `${D}+`, input: "a12\u0663\u0664b" },
  { pattern: "(?-u)\\d+", js: "[0-9]+", input: "a12\u0663\u0664b" },
  { pattern: "\\D+", js: `\\P{Nd}+`, input: "12ab\u0663" },
  { pattern: "\\s+", js: `${S}+`, input: "a \t\u00A0\u3000\u2028b" },
  { pattern: "(?-u)\\s+", js: "[\\t\\n\\v\\f\\r ]+", input: "a \t\u00A0b" },
  { pattern: "\\S+", js: `\\P{White_Space}+`, input: " ab\u00A0c " },
  { pattern: "\\w+", js: `${W}+`, input: "héllo_wörld \u0915\u094D\u0937 \u200D" },
  { pattern: "\\W+", js: `${NW}+`, input: "ab, cd!" },
  { pattern: "\\bfoo\\b", js: `${B}foo${B}`, input: "foo foobar barfoo foo", starts: [0, 1, 4, 18] },
  { pattern: "\\Boo\\B", js: `${NB}oo${NB}`, input: "foo fooo" },
  { pattern: "\\b", js: B, input: "ab cd" },
  { pattern: "\\b\\w", js: `${B}${W}`, input: "é école" },
  { pattern: "(?-u)\\b\\w+", js: `(?:(?<=${AW})(?!${AW})|(?<!${AW})(?=${AW}))${AW}+`, input: "éa b" },
  { pattern: "\\p{Lu}+", input: "aBCdÉ\u212A" },
  { pattern: "\\P{L}+", input: "ab12!?cd" },
  { pattern: "[\\p{Nd}\\p{Lu}]+", input: "ab12CDe" },
  { pattern: "[^\\p{L}\\p{Nd}]+", input: "ab, 12; cd" },
  { pattern: "\\p{Alphabetic}+\\p{White_Space}\\p{Join_Control}", input: "ab \u200C" },
  { pattern: "\\p{LC}+", input: "a\u01C5B\u02B0" },
  { pattern: "\\p{Cn}", input: "a\u0378" },
  { pattern: "\\p{Cs}", input: "a\uDC00" },
  { pattern: "(?i)\\p{Lu}+", js: "\\p{Lu}+", flags: "i", input: "abCD\u03C2" },
  { pattern: "[\\-\\/\\]\\[\\\\]+", js: "[\\-\\/\\]\\[\\\\]+", input: "a-/][\\b" },
  { pattern: "\\-\\/", js: "-\\/", input: "a-/" },
  { pattern: "\\^\\$\\.\\*\\+\\?\\(\\)\\{\\}\\|", input: "x^$.*+?(){}|" },
  { pattern: "\\x41\\u0042\\u{43}", input: "zABC" },
  { pattern: "\\n\\r\\t\\f\\v\\0", input: "a\n\r\t\f\v\0" },
  { pattern: "[\\n-\\r]+", input: "a\n\v\f\rb" },
  { pattern: "(a+)+b", input: "aaaab" },
  { pattern: "(a|b)*c", input: "abac", captures: [[0, 4], [2, 3]] },
  { pattern: "(?:(a)|b)+", input: "ab", captures: [[0, 2], [0, 1]] },
  { pattern: "(a*)*", input: "b", captures: [[0, 0], [-1, -1]] },
  { pattern: "(a*)+", input: "b", captures: [[0, 0], [0, 0]] },
  { pattern: "(a)|b", input: "b" },
  { pattern: "((a)(b))", input: "ab" },
  { pattern: "(a)(?:b(c))?", input: "ab ac abc" },
  { pattern: "(\\d+)-(\\d+)", js: `(${D}+)-(${D}+)`, input: "10-20 3-4", replacement: "${2}-${1}" },
  { pattern: "a", input: "banana", replacement: "$$${0}$" },
  { pattern: "(n)", input: "banana", replacement: "[${1}${9}${01}${}$x${1]" },
  { pattern: "x*", input: "abc", replacement: "-" },
  { pattern: "", input: "\u{1F600}a", replacement: "|" },
  { pattern: "b", input: "abc", replacement: "" },
  { pattern: "\\p{Lu}", input: "aÉb\u{1D400}c", replacement: "<${0}>" },
  { pattern: "😀|é", input: "a😀é", replacement: "${0}${0}" },
  { pattern: "[😀-😂]", input: "😀😁😂😃" },
  { pattern: "(?i)é", js: "é", flags: "i", input: "É" },
  { pattern: "(?i)ǆ", js: "ǆ", flags: "i", input: "ǄǅǆDŽ" },
  { pattern: "(?i)[ǅ]", js: "[ǅ]", flags: "i", input: "Ǆǆ" },
  { pattern: "(?i)θ", js: "θ", flags: "i", input: "ΘϑϴΣ" },
  { pattern: "(?i)[^k]", js: "[^k]", flags: "i", input: "K\u212Akx" },
  { pattern: "(?s).+", js: "[\\s\\S]+", input: "a\nb" },
  { pattern: ".+", js: `${DOT}+`, input: "a\u2028b\nc" },
  { pattern: "(?ms)^.+$", js: `${BOL}[\\s\\S]+${EOL}`, input: "ab\ncd" },
  { pattern: "a{3}|a", input: "aaaaa" },
  { pattern: "(?:a|b|c|d)+", input: "xabcdx" },
  { pattern: "x(?:)y", js: "xy", input: "xy" },
  { pattern: "a\\b", js: `a${B}`, input: "a" },
  { pattern: "(a)\\b(b)?", js: `(a)${B}(b)?`, input: "ab a" },
  { pattern: "[\\d\\s]+", js: `[${D}${S}]+`, input: "a1 2b" },
  { pattern: "[^\\d\\s]+", js: `[^${D}${S}]+`, input: "1ab 2" },
  { pattern: "[\\W\\d]+", js: `(?:${NW}|${D})+`, input: "ab1,2cd" },
  { pattern: "[^\\W]+", js: `${W}+`, input: "ab,cd" },
  { pattern: "\\D\\S\\W", js: `\\P{Nd}\\P{White_Space}${NW}`, input: "ab!1 c" },
];

// Rejected and accepted patterns: [pattern, kind or null, offset, message]. `repeat` builds a pattern from
// [text, count, text, count, ...]. `jsAccepts` marks Syntax errors that V8 accepts in `u` mode.
const NOTHING = "repetition operator has nothing to repeat; add an expression before it or escape it";
const COUNTED = "invalid counted repetition; use {n}, {n,} or {n,m} with n <= m";
const RANGE = "invalid class range; use single characters with start <= end";
const CODE_POINT = "invalid code point escape; use \\u{0} through \\u{10FFFF}";
const GROUP = "unsupported group syntax; lookaround, named groups and atomic groups are not supported";
const BACKREF = "backreferences are not supported; the engine guarantees linear time";
const INLINE = "inline flags are allowed only at the start; use (?flags:...)";
const UNCLOSED = "unclosed group; add ')'";
const UNCLOSED_CLASS = "unclosed character class; add ']'";
const EMPTY_CLASS = "empty character class; escape ']' as '\\]'";
const PROGRAM = "compiled program exceeds 10000 instructions; simplify the pattern or reduce counted repetitions";
const SLOTS = "too many capture groups for the pattern size; use (?:...) for groups that are not read";
const property = (name) => `unknown Unicode property '${name}'; use a general category such as Lu, or Alphabetic, White_Space, Join_Control`;
const unescaped = (text) => `unescaped '${text}'; escape it as '\\${text}'`;

export const compileCases = [
  { pattern: "(", kind: "Syntax", offset: 0, message: UNCLOSED },
  { pattern: "a**", kind: "Syntax", offset: 2, message: NOTHING },
  { pattern: "(?=a)", kind: "Unsupported", offset: 0, message: GROUP },
  { pattern: "\\1", kind: "Unsupported", offset: 0, message: BACKREF },
  { pattern: "a{1001}", kind: "TooLarge", offset: 1, message: "repetition count exceeds 1000" },
  { pattern: "(?:a{1000}){1000}", kind: "TooLarge", offset: 0, message: PROGRAM },
  { pattern: "a(b(c)", kind: "Syntax", offset: 1, message: UNCLOSED },
  { pattern: "(a))", kind: "Syntax", offset: 3, message: "unmatched ')'; escape it as '\\)'" },
  { pattern: ")", kind: "Syntax", offset: 0, message: "unmatched ')'; escape it as '\\)'" },
  { pattern: "a[bc", kind: "Syntax", offset: 1, message: UNCLOSED_CLASS },
  { pattern: "[\\]", kind: "Syntax", offset: 0, message: UNCLOSED_CLASS },
  { pattern: "[]", kind: "Syntax", offset: 0, message: EMPTY_CLASS, jsAccepts: true },
  { pattern: "x[^]", kind: "Syntax", offset: 1, message: EMPTY_CLASS, jsAccepts: true },
  { pattern: "{", kind: "Syntax", offset: 0, message: unescaped("{") },
  { pattern: "a}", kind: "Syntax", offset: 1, message: unescaped("}") },
  { pattern: "]", kind: "Syntax", offset: 0, message: unescaped("]") },
  { pattern: "a]b", kind: "Syntax", offset: 1, message: unescaped("]") },
  { pattern: "*a", kind: "Syntax", offset: 0, message: NOTHING },
  { pattern: "a|+", kind: "Syntax", offset: 2, message: NOTHING },
  { pattern: "(?)", kind: "Syntax", offset: 2, message: "unknown flag ')'; use i, m, s or u" },
  { pattern: "(*)", kind: "Syntax", offset: 1, message: NOTHING },
  { pattern: "^*", kind: "Syntax", offset: 1, message: NOTHING },
  { pattern: "$?", kind: "Syntax", offset: 1, message: NOTHING },
  { pattern: "\\b+", kind: "Syntax", offset: 2, message: NOTHING },
  { pattern: "a{2}*", kind: "Syntax", offset: 4, message: NOTHING },
  { pattern: "a*?+", kind: "Syntax", offset: 3, message: NOTHING },
  { pattern: "{1}", kind: "Syntax", offset: 0, message: NOTHING },
  { pattern: "a{1,0}", kind: "Syntax", offset: 1, message: COUNTED, jsAccepts: false },
  { pattern: "a{", kind: "Syntax", offset: 1, message: COUNTED },
  { pattern: "a{x}", kind: "Syntax", offset: 1, message: COUNTED },
  { pattern: "a{,3}", kind: "Syntax", offset: 1, message: COUNTED },
  { pattern: "a{1,2", kind: "Syntax", offset: 1, message: COUNTED },
  { pattern: "[b-a]", kind: "Syntax", offset: 1, message: RANGE },
  { pattern: "[\\w-a]", kind: "Syntax", offset: 1, message: RANGE },
  { pattern: "[a-\\d]", kind: "Syntax", offset: 1, message: RANGE },
  { pattern: "[a-c-e]", kind: "Syntax", offset: 4, message: RANGE, jsAccepts: true },
  { pattern: "\\q", kind: "Syntax", offset: 0, message: "unknown escape '\\q'; escape only syntax characters" },
  { pattern: "[\\b]", kind: "Syntax", offset: 1, message: "unknown escape '\\b'; escape only syntax characters", jsAccepts: true },
  { pattern: "\\😀", kind: "Syntax", offset: 0, message: "unknown escape '\\😀'; escape only syntax characters" },
  { pattern: "a\\", kind: "Syntax", offset: 1, message: "pattern ends with '\\'; escape a backslash as '\\\\'" },
  { pattern: "\\x4", kind: "Syntax", offset: 0, message: CODE_POINT },
  { pattern: "a\\u{110000}", kind: "Syntax", offset: 1, message: CODE_POINT },
  { pattern: "\\u{}", kind: "Syntax", offset: 0, message: CODE_POINT },
  { pattern: "\\u12", kind: "Syntax", offset: 0, message: CODE_POINT },
  { pattern: "(?x)", kind: "Syntax", offset: 2, message: "unknown flag 'x'; use i, m, s or u" },
  { pattern: "(?i-)", kind: "Syntax", offset: 4, message: "unknown flag ')'; use i, m, s or u" },
  { pattern: "(?i", kind: "Syntax", offset: 0, message: UNCLOSED },
  { pattern: "(?!a)", kind: "Unsupported", offset: 0, message: GROUP },
  { pattern: "a(?<=a)", kind: "Unsupported", offset: 1, message: GROUP },
  { pattern: "(?<!a)", kind: "Unsupported", offset: 0, message: GROUP },
  { pattern: "(?<name>a)", kind: "Unsupported", offset: 0, message: GROUP },
  { pattern: "(?P<n>a)", kind: "Unsupported", offset: 0, message: GROUP },
  { pattern: "(?>a)", kind: "Unsupported", offset: 0, message: GROUP },
  { pattern: "(?#c)", kind: "Unsupported", offset: 0, message: GROUP },
  { pattern: "(a)\\9", kind: "Unsupported", offset: 3, message: BACKREF },
  { pattern: "\\k<a>", kind: "Unsupported", offset: 0, message: BACKREF },
  { pattern: "\\01", kind: "Unsupported", offset: 0, message: BACKREF },
  { pattern: "a(?i)b", kind: "Unsupported", offset: 1, message: INLINE },
  { pattern: "(?i)(?m)a", kind: "Unsupported", offset: 4, message: INLINE },
  { pattern: "\\p{Foo}", kind: "Unsupported", offset: 0, message: property("Foo") },
  { pattern: "x\\P{Script=Latin}", kind: "Unsupported", offset: 1, message: property("Script=Latin") },
  { pattern: "[\\p{gc=Lu}]", kind: "Unsupported", offset: 1, message: property("gc=Lu") },
  { pattern: "\\p{Lowercase_Letter}", kind: "Unsupported", offset: 0, message: property("Lowercase_Letter") },
  { pattern: "\\pL", kind: "Unsupported", offset: 0, message: property("L") },
  { pattern: "\\p{L", kind: "Unsupported", offset: 0, message: property("L") },
  { pattern: "[a[b]]", kind: "Unsupported", offset: 2, message: "unsupported syntax '['" },
  { pattern: "[a&&b]", kind: "Unsupported", offset: 2, message: "unsupported syntax '&&'" },
  { pattern: "a*+", kind: "Unsupported", offset: 1, message: "unsupported syntax '*+'" },
  { pattern: "a{2}+", kind: "Unsupported", offset: 1, message: "unsupported syntax '{2}+'" },
  { pattern: "\\G", kind: "Unsupported", offset: 0, message: "unsupported syntax '\\G'" },
  { pattern: "a\\Z", kind: "Unsupported", offset: 1, message: "unsupported syntax '\\Z'" },
  { pattern: "\\X", kind: "Unsupported", offset: 0, message: "unsupported syntax '\\X'" },
  { pattern: "\\R", kind: "Unsupported", offset: 0, message: "unsupported syntax '\\R'" },
  { pattern: "\\K", kind: "Unsupported", offset: 0, message: "unsupported syntax '\\K'" },
  { pattern: "a{0,1001}", kind: "TooLarge", offset: 1, message: "repetition count exceeds 1000" },
  { pattern: "a{2000,1}", kind: "TooLarge", offset: 1, message: "repetition count exceeds 1000" },
  { pattern: "(?:a{100}){100}", kind: "TooLarge", offset: 0, message: PROGRAM },
  { pattern: "(?:a{999}){10}a{8}", kind: "TooLarge", offset: 0, message: PROGRAM },
  { pattern: "(?:a{999}){10}a{7}", kind: null },
  { repeat: ["(", 65, ")", 65], kind: "TooLarge", offset: 64, message: "groups are nested deeper than 64" },
  { repeat: ["(", 64, ")", 64], kind: null },
  { repeat: ["a", 65537], kind: "TooLarge", offset: 0, message: "pattern is longer than 65536 code units" },
  { repeat: ["a", 65536], kind: "TooLarge", offset: 0, message: PROGRAM },
  { repeat: ["(a)", 209], kind: "TooLarge", offset: 0, message: SLOTS },
  { repeat: ["(a)", 208], kind: null },
  { repeat: ["[\\w]", 82], kind: "TooLarge", offset: 0, message: "character classes exceed 65536 ranges in total" },
  { repeat: ["[\\w]", 81], kind: null },
  // A class counts its ranges once, after merging: repeated and overlapping escapes in it add nothing (`\w` has
  // 802 ranges, so 82 classes of `\w` exceed 65,536 and 81 do not). Review fix: 164 or more `\w` in one class
  // used to be TooLarge.
  { repeat: ["[", 1, "\\w", 170, "]", 1], kind: null },
  { repeat: ["[", 1, "\\w\\p{L}\\p{Alphabetic}\\d\\W", 2000, "]", 1], kind: null },
  { repeat: ["[", 1, "\\w", 170, "]", 1, "[\\w]", 80], kind: null },
  { repeat: ["[", 1, "\\w", 170, "]", 1, "[\\w]", 81], kind: "TooLarge", offset: 0, message: "character classes exceed 65536 ranges in total" },
  { repeat: ["\\w", 2000], kind: null },
  { pattern: "a{1000}", kind: null },
  { pattern: "a{0,1000}?", kind: null },
  { pattern: "a{1000,}", kind: null },
  { pattern: "(?i-msu)a", kind: null },
  { pattern: "(?-u:\\w)(?s:.)", kind: null },
  { pattern: "", kind: null },
  { pattern: "a|", kind: null },
  { pattern: "()", kind: null },
  { pattern: "\\-", kind: null },
  { pattern: "[\\-]", kind: null },
];

// `property_hash` names: every name of `\p{...}` and two unknown ones.
export const propertyNames = ["Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl", "No", "Pc", "Pd", "Ps",
  "Pe", "Pi", "Pf", "Po", "Sm", "Sc", "Sk", "So", "Zs", "Zl", "Zp", "Cc", "Cf", "Cs", "Co", "Cn", "L", "LC", "M",
  "N", "P", "S", "Z", "C", "Alphabetic", "White_Space", "Join_Control", "Greek", "lu"];

// `linear` cases: the steps of `find` on unit * n + suffix must grow linearly in n.
export const linearCases = [
  ["(a*)*b", "a", ""], ["(a|a)*b", "a", ""], ["(a|aa)*c", "a", ""], ["(x+x+)+y", "x", ""],
  ["(.*a){20}", "a", ""], ["^(a+)+$", "a", "!"], ["(?i)(ß|ss)*x", "ss", ""], ["(?:a?){50}a{50}", "a", "b"],
  ["\\b\\w+\\b", "ab ", "!"], ["[\\p{L}\\d]+@", "a1", ""],
];

export const escapeInputs = ["", "abc", "^$\\.*+?()[]{}|", "a.b*c", "-/ ,é😀", "\\\\"];

function literal(text) {
  let out = "\"";
  for (let index = 0; index < text.length; index++) {
    const code = text.codePointAt(index);
    if (code > 0xffff) index++;
    if (code === 0x22 || code === 0x5c) out += `\\${String.fromCharCode(code)}`;
    else if (code >= 0x20 && code < 0x7f) out += String.fromCharCode(code);
    else if (code >= 0xd800 && code <= 0xdfff) out += `\\u${code.toString(16).toUpperCase().padStart(4, "0")}`;
    else out += `\\u{${code.toString(16).toUpperCase()}}`;
  }
  return `${out}"`;
}

function table(name, values) {
  const arms = values.map((value, index) => `    | ${index} -> ${value}`).join("\n");
  return `def ${name} :: i64 -> string\nfn ${name} which =\n    match which with\n${arms}\n    | _ -> ""\n`;
}

const builtPattern = (item) => item.repeat
  ? Array.from({ length: item.repeat.length / 2 }, (_, index) => `String.repeat (ref ${literal(item.repeat[2 * index])}) ${item.repeat[2 * index + 1]}`).join(" + ")
  : literal(item.pattern);

export function casesSource() {
  return "// Generated by `node tests/regex-cases.mjs --write` from tests/regex-cases.mjs. Do not edit.\n\n"
    + [
      table("pattern", matchCases.map((item) => literal(item.pattern))),
      table("input", matchCases.map((item) => literal(item.input))),
      table("replacement", matchCases.map((item) => literal(item.replacement ?? "<${0}>"))),
      table("error_pattern", compileCases.map(builtPattern)),
      table("property_name", propertyNames.map(literal)),
      table("linear_pattern", linearCases.map(([pattern]) => literal(pattern))),
      table("linear_unit", linearCases.map(([, unit]) => literal(unit))),
      table("linear_suffix", linearCases.map(([, , suffix]) => literal(suffix))),
      table("escape_input", escapeInputs.map(literal)),
    ].join("\n");
}

export const casesPath = resolve(dirname(fileURLToPath(import.meta.url)), "fixtures/regex/Cases.tz");

// The hash of the fixture: mix h v = (h * 31 + v + 1) mod 1000000007.
const MOD = 1000000007n;
const mix = (hash, value) => (hash * 31n + BigInt(value) + 1n) % MOD;
const utf16 = (text) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
const utf8 = (text) => [...Buffer.from(text, "utf8")];
const byteOffset = (text, offset) => Buffer.byteLength(text.slice(0, offset), "utf8");
const hashUnits = (hash, units) => units.reduce(mix, mix(hash, units.length));
const span = ([first, last]) => BigInt(first) * 1000000n + BigInt(last);

function jsRegex(item, extra) {
  return new RegExp(item.js ?? item.pattern, `u${item.flags ?? ""}${extra}`);
}

function allMatches(item) {
  const re = jsRegex(item, "gd");
  return [...item.input.matchAll(re)];
}

function expand(replacement, groups) {
  let out = "";
  for (let index = 0; index < replacement.length;) {
    if (replacement[index] === "$" && replacement[index + 1] === "$") { out += "$"; index += 2; continue; }
    const named = /^\$\{(\d+)\}/.exec(replacement.slice(index));
    if (named) {
      const group = Number(named[1]);
      out += group < groups.length && groups[group] !== undefined ? groups[group] : "";
      index += named[0].length;
      continue;
    }
    out += replacement[index++];
  }
  return out;
}

const scalarStart = (text, offset) => offset >= 0 && offset <= text.length
  && !(offset > 0 && offset < text.length && /[\uDC00-\uDFFF]/.test(text[offset]) && /[\uD800-\uDBFF]/.test(text[offset - 1]));

export function expectedCases() {
  const cases = [];
  matchCases.forEach((item, which) => {
    const w = BigInt(which);
    const lone = /[\uD800-\uDFFF]/u.test(item.input.replace(/[\uD800-\uDBFF][\uDC00-\uDFFF]/g, ""));
    const matches = allMatches(item);
    const first = matches[0];
    const firstSpan = first ? [first.index, first.index + first[0].length] : null;
    const groups = first?.indices ?? [];
    const captures = item.captures ?? (first ? groups.map((range) => range ?? [-1, -1]) : null);
    const ranges = matches.map((match) => [match.index, match.index + match[0].length]);
    const pieces = [];
    let start = 0;
    for (const [first, last] of ranges) { pieces.push(item.input.slice(start, first)); start = last; }
    pieces.push(item.input.slice(start));
    let replaced = "";
    start = 0;
    for (const match of matches) {
      replaced += item.input.slice(start, match.index) + expand(item.replacement ?? "<${0}>", match);
      start = match.index + match[0].length;
    }
    replaced += item.input.slice(start);
    const repeatGroups = item.captures !== undefined;
    cases.push(["is_match16", [w], first ? 1 : 0]);
    cases.push(["find16", [w], firstSpan ? span(firstSpan) : -1n]);
    if (captures) cases.push(["captures16", [w], captures.reduce((hash, range) => mix(mix(hash, range[0]), range[1]), mix(0n, captures.length))]);
    else cases.push(["captures16", [w], -1n]);
    cases.push(["find_all16", [w], ranges.reduce((hash, range) => mix(mix(hash, range[0]), range[1]), mix(0n, ranges.length))]);
    cases.push(["split16", [w], pieces.reduce((hash, piece) => hashUnits(hash, utf16(piece)), mix(0n, pieces.length))]);
    if (!repeatGroups) cases.push(["replace16", [w], hashUnits(0n, utf16(replaced))]);
    for (const offset of item.starts ?? [0, 1]) {
      let expected = -1n;
      if (scalarStart(item.input, offset)) {
        const re = jsRegex(item, "g");
        re.lastIndex = offset;
        const found = re.exec(item.input);
        if (found) expected = span([found.index, found.index + found[0].length]);
      }
      cases.push(["find_at16", [w, BigInt(offset)], expected]);
    }
    if (lone) return;
    const toBytes = ([first, last]) => [byteOffset(item.input, first), byteOffset(item.input, last)];
    cases.push(["is_match8", [w], first ? 1 : 0]);
    cases.push(["find8", [w], firstSpan ? span(toBytes(firstSpan)) : -1n]);
    cases.push(["captures8", [w], captures ? captures.map((range) => range[0] < 0 ? range : toBytes(range)).reduce((hash, range) => mix(mix(hash, range[0]), range[1]), mix(0n, captures.length)) : -1n]);
    cases.push(["find_all8", [w], ranges.map(toBytes).reduce((hash, range) => mix(mix(hash, range[0]), range[1]), mix(0n, ranges.length))]);
    cases.push(["split8", [w], pieces.reduce((hash, piece) => hashUnits(hash, utf8(piece)), mix(0n, pieces.length))]);
    if (!repeatGroups) cases.push(["replace8", [w], hashUnits(0n, utf8(replaced))]);
    for (const offset of item.starts ?? [0, 1]) {
      if (!scalarStart(item.input, offset)) continue;
      const re = jsRegex(item, "g");
      re.lastIndex = offset;
      const found = re.exec(item.input);
      const bytes = byteOffset(item.input, offset);
      cases.push(["find_at8", [w, BigInt(bytes)], found ? span(toBytes([found.index, found.index + found[0].length])) : -1n]);
      // A byte offset inside a scalar is not a boundary.
      if (offset < item.input.length && item.input.codePointAt(offset) > 0x7f) cases.push(["find_at8", [w, BigInt(bytes + 1)], -1n]);
    }
    if (item.starts) {
      cases.push(["find_at8", [w, -1n], -1n]);
      cases.push(["find_at8", [w, BigInt(Buffer.byteLength(item.input, "utf8") + 1)], -1n]);
    }
  });
  const kinds = { Syntax: 0n, Unsupported: 1n, TooLarge: 2n };
  compileCases.forEach((item, which) => {
    const w = BigInt(which);
    if (!item.kind) {
      cases.push(["compile_result", [w], -1n]);
      return;
    }
    if (item.kind === "Syntax" && item.pattern !== undefined) {
      let accepted = true;
      try { new RegExp(item.pattern, "u"); } catch { accepted = false; }
      assert.equal(accepted, item.jsAccepts ?? false, `V8 and D09 disagree on ${item.pattern}`);
    }
    cases.push(["compile_result", [w], kinds[item.kind] * 1000000n + BigInt(item.offset)]);
    cases.push(["message_hash", [w], hashUnits(0n, utf16(item.message))]);
  });
  escapeInputs.forEach((input, which) => {
    const escaped = input.replace(/[\^$\\.*+?()[\]{}|]/g, (character) => `\\${character}`);
    assert.ok(new RegExp(`^(?:${escaped})$`, "u").test(input), escaped);
    cases.push(["escape16", [BigInt(which)], hashUnits(0n, utf16(escaped))]);
  });
  propertyNames.forEach((name, which) => {
    let expected = -1n;
    let re = null;
    try { re = new RegExp(`^\\p{${name}}$`, "u"); } catch { re = null; }
    if (re && !["Greek"].includes(name)) {
      const ranges = [];
      for (let code = 0; code <= 0x10ffff; code++) {
        const text = code >= 0xd800 && code <= 0xdfff ? String.fromCharCode(code) : String.fromCodePoint(code);
        if (!re.test(text)) continue;
        const last = ranges.at(-1);
        if (last && last[1] + 1 === code) last[1] = code; else ranges.push([code, code]);
      }
      expected = ranges.reduce((hash, range) => mix(mix(hash, range[0]), range[1]), mix(0n, ranges.length));
    }
    cases.push(["property_hash", [BigInt(which)], expected]);
  });
  cases.push(["fold_orbit_hash", [], foldOrbitHash()]);
  linearCases.forEach((_, which) => cases.push(["linear", [BigInt(which)], 1]));
  return cases;
}

// The orbits of simple case folding, as V8's `iu` matching sees them: every scalar that a case mapping
// changes, grouped by which of them `[c]` matches; each orbit of two or more scalars counts, in order of
// its smallest member.
function foldOrbitHash() {
  const candidates = [];
  for (let code = 0; code <= 0x10ffff; code++) {
    if (code >= 0xd800 && code <= 0xdfff) continue;
    const text = String.fromCodePoint(code);
    if (text.toLowerCase() !== text || text.toUpperCase() !== text) candidates.push(code);
  }
  const all = String.fromCodePoint(...candidates);
  const seen = new Set();
  let hash = 0n;
  let orbits = 0;
  for (const code of candidates) {
    if (seen.has(code)) continue;
    const pattern = new RegExp(`[\\u{${code.toString(16)}}]`, "giu");
    const members = [...all.matchAll(pattern)].map((match) => match[0].codePointAt(0)).sort((a, b) => a - b);
    for (const member of members) seen.add(member);
    if (members.length < 2) continue;
    orbits++;
    hash = members.reduce(mix, mix(hash, members.length));
  }
  return mix(hash, orbits);
}

if (process.argv.includes("--write")) {
  writeFileSync(casesPath, casesSource());
  console.log(`wrote ${casesPath}`);
} else if (process.argv[1] === fileURLToPath(import.meta.url)) {
  assert.equal(readFileSync(casesPath, "utf8"), casesSource(), "run node tests/regex-cases.mjs --write");
  console.log(`regex cases: ${expectedCases().length} expectations`);
}
