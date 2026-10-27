Acme::Shop::Cart.line_total
def | public | Cart.tz:7
sig: def line_total :: ref Line -> i64
params: line:ref
doc: The price of one line in cents.
callers (2): Acme::Shop::Cart.total, test:Acme::Shop::Cart.line total
tests (3): test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Cart.line total, test:Acme::Shop::Main.main runs
