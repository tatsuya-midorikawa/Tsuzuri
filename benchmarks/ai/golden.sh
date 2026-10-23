#!/bin/sh
# Independent appendix D calculation: no harness imports or inferred expectations.
set -eu
cd "$(dirname "$0")/../.."
export LC_ALL=en_US.UTF-8
shop=benchmarks/ai/corpus/shop
measure() {
    id=$1; patterns=$2; reads=$3
    chars=0; calls=0
    for pattern in $patterns; do
        if [ "$id" = A-05 ]; then pattern='record Item'; fi
        n=$(grep -nF "$pattern" "$shop"/*.tz | LC_ALL=C sort -t: -k1,1 -k2,2n | wc -m)
        chars=$((chars + n)); calls=$((calls + 1))
    done
    for file in $reads; do
        n=$(wc -m < "$shop/$file.tz")
        chars=$((chars + n)); calls=$((calls + 1))
    done
    printf '%s %s %s %s\n' "$id" "$chars" "$calls" "$(((chars + 3) / 4))"
}
measure A-05 record ""
measure B-05 order_total "Orders Report"
measure B-06 percent_of ""
measure F-01 "pad2 show discounted summary main" "Report"
measure F-02 "percent_of with_discount with_tax total_with_discount gross discounted price_of main line_total order_total summary" "Catalog Orders Report"
measure F-03 "position price_of line_total order_total total_with_discount summary discounted main" "Catalog Orders Report"
measure F-04 "available can_fill fillable main" "Orders Stock"
measure F-05 take ""
measure F-06 "line_total order_total total_with_discount summary discounted main" "Orders Report"
