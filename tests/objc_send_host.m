// The Objective-C host of tests/objc_send.mjs (M02). It is compiled with -fobjc-arc and uses only
// <objc/NSObject.h>, so the test links with libobjc alone. `tzsend_live` counts the objects between
// init and dealloc. Every expected value of the suite is computed from this file's own arithmetic
// (in JavaScript, in the suite) and from these counts; the Tsuzuri compiler produces none of them.
#import <objc/NSObject.h>
#import <objc/runtime.h>
#include <string.h>

static long live;

// The shapes of CGPoint, CGRect, NSRange and of a struct that is passed in memory.
typedef struct { double x, y; } TzPoint;
typedef struct { double width, height; } TzSize;
typedef struct { TzPoint origin; TzSize size; } TzRect;
typedef struct { unsigned long location, length; } TzRange;
typedef struct { double a, b, c, d, e, f; } TzBig;
typedef struct { int left, right; } TzPair;
typedef struct { float r, g, b; } TzColor;

@interface TzSend : NSObject
@property(nonatomic) long value;
@end

@implementation TzSend {
  // Set by `init`: an object that `initWithValue:` refused was never counted.
  BOOL _counted;
}
- (instancetype)init {
  if ((self = [super init])) {
    _counted = YES;
    live++;
  }
  return self;
}
// The designated initializer of the tests: fails (returns nil) for a negative value.
- (instancetype)initWithValue:(long)value {
  if (value < 0) {
    return nil;
  }
  if ((self = [self init])) {
    _value = value;
  }
  return self;
}
- (void)dealloc {
  if (_counted) {
    live--;
  }
}
// +0: an autoreleased object, as a convenience constructor returns one.
+ (instancetype)makeWithValue:(long)value {
  return [[self alloc] initWithValue:value];
}
// +1 by the `new` family.
+ (instancetype)newWithValue:(long)value {
  return [[self alloc] initWithValue:value];
}
+ (long)answer {
  return 42;
}
// +1 by the `copy` family.
- (id)copyTwin {
  return [[[self class] alloc] initWithValue:_value];
}
// +0.
- (TzSend *)twin {
  return [[[self class] alloc] initWithValue:_value];
}
- (TzSend *)maybeTwin:(BOOL)present {
  return present ? [self twin] : nil;
}
- (long)tag {
  return 1;
}

// Scalars in their own widths.
- (BOOL)isPositive {
  return _value > 0;
}
- (BOOL)flip:(BOOL)flag {
  return !flag;
}
- (char)narrow:(char)value {
  return (char)(value - 1);
}
- (unsigned char)ubyte:(unsigned char)value {
  return (unsigned char)(value + 1);
}
- (short)shorter:(short)value {
  return (short)(value - 1);
}
- (unsigned short)ushorter:(unsigned short)value {
  return (unsigned short)(value + 1);
}
- (int)int32:(int)value {
  return value - 1;
}
- (unsigned int)uint32:(unsigned int)value {
  return value + 1;
}
- (float)half:(float)value {
  return value / 2;
}
- (double)twice:(double)value {
  return value * 2;
}
// Sign and zero extension: the callee widens each argument as its C type says.
- (long)takeChar:(char)c short:(short)s uchar:(unsigned char)uc ushort:(unsigned short)us flag:(BOOL)flag f:(float)f {
  return (long)c + (long)s + (long)uc + (long)us + (flag ? 1000000 : 0) + (long)(f * 2);
}
// More arguments than registers: the last ones are on the stack.
- (long)sum:(long)a b:(long)b c:(long)c d:(long)d e:(long)e f:(long)f g:(long)g h:(long)h i:(long)i j:(long)j {
  return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h + 9 * i + 10 * j;
}
- (double)mixInt:(long)a d:(double)b i:(int)c f:(float)d flag:(BOOL)e {
  return a + b + c + d + (e ? 0.5 : 0);
}

// Buffers and out-parameters.
- (long)sumBytes:(const unsigned char *)bytes length:(long)length {
  long total = 0;
  for (long index = 0; index < length; index++) {
    total += bytes[index];
  }
  return total;
}
- (void)fill:(long *)out {
  *out = _value * 3;
}
- (void)tell:(BOOL *)flag {
  *flag = !*flag;
}

// Objects, classes and selectors.
- (BOOL)isSame:(TzSend *)other {
  return self == other;
}
- (long)valueOr:(TzSend *)other {
  return other ? other.value : -1;
}
- (long)apply:(long (*)(long))function to:(long)value {
  return function(value) + _value;
}

// Structs by value.
- (TzPoint)point {
  return (TzPoint){(double)_value, (double)_value + 0.5};
}
- (TzRect)frame {
  return (TzRect){{1, 2}, {(double)_value, 4}};
}
- (TzRange)range {
  return (TzRange){(unsigned long)_value, 7};
}
- (TzBig)big {
  return (TzBig){1, 2, 3, 4, 5, (double)_value};
}
- (TzPair)pair {
  return (TzPair){(int)_value, -(int)_value};
}
- (TzColor)color {
  return (TzColor){0.25f, 0.5f, (float)_value};
}
- (double)areaOf:(TzRect)rect {
  return rect.size.width * rect.size.height + rect.origin.x + 10 * rect.origin.y;
}
- (TzPoint)offset:(TzPoint)point by:(double)delta {
  return (TzPoint){point.x + delta, point.y - delta};
}
- (TzRange)grow:(TzRange)range by:(unsigned long)amount {
  return (TzRange){range.location + amount, range.length * amount};
}
- (TzBig)scale:(TzBig)big by:(double)factor {
  return (TzBig){big.a * factor, big.b * factor, big.c * factor, big.d * factor, big.e * factor, big.f * factor};
}
- (long)sumPair:(TzPair)pair {
  return (long)pair.left * 100 + pair.right;
}
- (double)luma:(TzColor)color {
  return color.r + 2 * color.g + 4 * color.b;
}
// Ten doubles of HFAs: the eight floating-point registers run out, so the last point is on the stack.
- (double)mix:(TzRect)a with:(TzRect)b at:(TzPoint)c {
  return a.origin.x + 2 * a.origin.y + 3 * a.size.width + 4 * a.size.height + 5 * b.origin.x + 6 * b.origin.y
       + 7 * b.size.width + 8 * b.size.height + 9 * c.x + 10 * c.y;
}
// Integer registers run out before the range, which then goes to the stack as a whole.
- (long)spill:(long)a b:(long)b c:(long)c d:(long)d e:(long)e range:(TzRange)range {
  return a + b + c + d + e + (long)range.location * 100 + (long)range.length * 10000;
}
@end

@interface TzSendChild : TzSend
@end

@implementation TzSendChild
- (long)tag {
  return 2;
}
@end

@interface TzSend (Extras)
- (long)doubled;
+ (long)extra;
@end

@implementation TzSend (Extras)
- (long)doubled {
  return self.value * 2;
}
+ (long)extra {
  return 7;
}
@end

// A class whose objects any thread may use.
@interface TzSendShared : NSObject
- (long)ping;
@end

@implementation TzSendShared
- (long)ping {
  return 5;
}
@end

long tzsend_live(void) { return live; }
int tzsend_class_is(Class class, const char *name) { return strcmp(class_getName(class), name) == 0; }
long tzsend_triple(long value) { return value * 3; }
