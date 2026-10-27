Acme::Shop::Cart.total
def | public | Cart.tz:10
sig: def total :: ref [Line] -> i64
params: lines:ref
doc: The price of all lines in cents.
sites: trap 1
callees (1): Acme::Shop::Cart.line_total
callers (3): Acme::Shop::Report.print_total, test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Main.main runs
tests (2): test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Main.main runs
