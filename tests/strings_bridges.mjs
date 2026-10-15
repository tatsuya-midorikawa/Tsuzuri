export function storageBridges() {
  return ["16", "8"].map((bits) => {
    const type = bits === "16" ? "string" : "utf8string";
    const descriptor = (arg) => `  %${arg}0 = insertvalue %tz.${type} zeroinitializer, ptr %${arg}, 0
  %${arg}1 = insertvalue %tz.${type} %${arg}0, i64 %${arg}n, 1
`;
    const result = `  %data = extractvalue %tz.${type} %value, 0
  %size = extractvalue %tz.${type} %value, 1
  store ptr %data, ptr %out
  ret i64 %size
}
`;
    return `define i64 @test_clone${bits}(ptr %input, i64 %length, ptr %out) {
  %value = call %tz.${type} @tz.${type}.new(ptr %input, i64 %length)
${result}
define i64 @test_append${bits}(ptr %a, i64 %an, ptr %b, i64 %bn, ptr %out) {
${descriptor("a")}${descriptor("b")}  %value = call %tz.${type} @tz.${type}.append(%tz.${type} %a1, %tz.${type} %b1)
${result}
define i64 @test_concat_into${bits}(ptr %buffer, i64 %capacity, ptr %a, i64 %an, ptr %b, i64 %bn, ptr %out) {
${descriptor("a")}${descriptor("b")}  %value = call %tz.${type} @tz.${type}.concat_into(ptr %buffer, i64 %capacity, %tz.${type} %a1, %tz.${type} %b1)
${result}`;
  }).join("");
}
