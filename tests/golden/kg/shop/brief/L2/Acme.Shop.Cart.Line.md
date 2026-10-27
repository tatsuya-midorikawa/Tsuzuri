Acme::Shop::Cart.Line
record | public | Cart.tz:4
sig: record Line
fields: sku: string, quantity: i64, unit_price: i64
doc: One line of a cart: a product, how many, and its price in cents.
used by (7): Acme::Shop::Cart.line_total, Acme::Shop::Cart.total, Acme::Shop::Main.main, Acme::Shop::Report.print_total, test:Acme::Shop::Cart.empty cart, …2 more
