// The reference for the call types of M02 (stop condition S4): the same messages as
// `reference_calls` of tests/objc/send/Main.tz, in the same order, written in Objective-C against
// the declarations of tests/objc_send_host.m. tests/objc_send.mjs compiles this file with clang
// (`-S -emit-llvm`, no selector stubs, no runtime-call fast paths) and compares the type of every
// `objc_msgSend` call with the one that the Tsuzuri compiler emits for the same send.
#import <objc/NSObject.h>

typedef struct { double x, y; } TzPoint;
typedef struct { double width, height; } TzSize;
typedef struct { TzPoint origin; TzSize size; } TzRect;
typedef struct { unsigned long location, length; } TzRange;
typedef struct { double a, b, c, d, e, f; } TzBig;
typedef struct { int left, right; } TzPair;
typedef struct { float r, g, b; } TzColor;

@interface TzSend : NSObject
@property(nonatomic) long value;
- (instancetype)initWithValue:(long)value;
+ (instancetype)makeWithValue:(long)value;
+ (instancetype)newWithValue:(long)value;
- (BOOL)isPositive;
- (BOOL)flip:(BOOL)flag;
- (char)narrow:(char)value;
- (unsigned char)ubyte:(unsigned char)value;
- (short)shorter:(short)value;
- (unsigned short)ushorter:(unsigned short)value;
- (int)int32:(int)value;
- (unsigned int)uint32:(unsigned int)value;
- (float)half:(float)value;
- (double)twice:(double)value;
- (long)takeChar:(char)c short:(short)s uchar:(unsigned char)uc ushort:(unsigned short)us flag:(BOOL)flag f:(float)f;
- (long)sum:(long)a b:(long)b c:(long)c d:(long)d e:(long)e f:(long)f g:(long)g h:(long)h i:(long)i j:(long)j;
- (double)mixInt:(long)a d:(double)b i:(int)c f:(float)d flag:(BOOL)e;
- (long)sumBytes:(const unsigned char *)bytes length:(long)length;
- (void)fill:(long *)out;
- (void)tell:(BOOL *)flag;
- (BOOL)isSame:(TzSend *)other;
- (long)valueOr:(TzSend *)other;
- (long)apply:(long (*)(long))function to:(long)value;
- (TzPoint)point;
- (TzRect)frame;
- (TzRange)range;
- (TzBig)big;
- (TzPair)pair;
- (TzColor)color;
- (double)areaOf:(TzRect)rect;
- (TzPoint)offset:(TzPoint)point by:(double)delta;
- (TzRange)grow:(TzRange)range by:(unsigned long)amount;
- (TzBig)scale:(TzBig)big by:(double)factor;
- (long)sumPair:(TzPair)pair;
- (double)luma:(TzColor)color;
- (double)mix:(TzRect)a with:(TzRect)b at:(TzPoint)c;
- (long)spill:(long)a b:(long)b c:(long)c d:(long)d e:(long)e range:(TzRange)range;
@end

@interface TzSend (Extras)
- (long)doubled;
+ (long)extra;
@end

long tzsend_triple(long value);

void reference_calls(const unsigned char *bytes, long count) {
  TzSend *object = [TzSend newWithValue:5];
  TzSend *made = [TzSend makeWithValue:6];
  TzSend *built = [[TzSend alloc] initWithValue:7];
  [object isPositive];
  [object flip:YES];
  [object narrow:1];
  [object ubyte:2];
  [object shorter:3];
  [object ushorter:4];
  [object int32:5];
  [object uint32:6];
  [object half:1.5f];
  [object twice:2.5];
  [object takeChar:1 short:2 uchar:3 ushort:4 flag:YES f:1.5f];
  [object sum:1 b:2 c:3 d:4 e:5 f:6 g:7 h:8 i:9 j:10];
  [object mixInt:1 d:2.5 i:3 f:0.25f flag:YES];
  [object sumBytes:bytes length:count];
  long filled = 0;
  [object fill:&filled];
  BOOL flag = NO;
  [object tell:&flag];
  [object isSame:made];
  [object valueOr:built];
  [object apply:tzsend_triple to:4];
  TzPoint point = [object point];
  TzRect frame = [object frame];
  TzRange range = [object range];
  TzBig big = [object big];
  TzPair pair = [object pair];
  TzColor color = [object color];
  [object areaOf:frame];
  [object offset:point by:0.25];
  [object grow:range by:5];
  [object scale:big by:1.5];
  [object sumPair:pair];
  [object luma:color];
  [object mix:frame with:frame at:point];
  [object spill:1 b:2 c:3 d:4 e:5 range:range];
  [TzSend class];
  [object respondsToSelector:@selector(twin)];
  [object doubled];
  [TzSend extra];
}
