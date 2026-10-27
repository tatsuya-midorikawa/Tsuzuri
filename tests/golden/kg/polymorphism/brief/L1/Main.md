# Main (Main.tz)
6 items, 6 public | 0 tests

## Types
- record Point {x, y}

## Functions
- def add :: own 'a -> own 'a -> 'a
- def score :: Classes.Score<'a> => ref 'a -> i32

## Instances
- instance Add<Point>
- instance Eq<Point>
- instance Classes.Score<Point>
