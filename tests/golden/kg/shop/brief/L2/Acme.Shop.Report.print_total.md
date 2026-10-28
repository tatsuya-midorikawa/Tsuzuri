Acme::Shop::Report.print_total
def | public | Report.tz:5
sig: def print_total :: ref [Line] -> unit
params: lines:ref
effects: io
callees (3): Acme::Shop::Cart.total, IO.run, IO.write_line
callers (1): Acme::Shop::Main.main
