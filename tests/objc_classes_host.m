// The Objective-C host of tests/objc_classes.mjs (M01). It is compiled with -fobjc-arc and uses
// only <objc/NSObject.h>, so the test links with libobjc alone. `tzobjc_live` counts the probe
// objects between init and dealloc: the expectations of the suite come from these counts, which
// the Tsuzuri compiler does not produce.
#import <objc/NSObject.h>
#import <objc/runtime.h>
#include <string.h>

static long live;

@interface TzProbe : NSObject
@end

@implementation TzProbe
- (instancetype)init {
  if ((self = [super init])) {
    live++;
  }
  return self;
}
- (void)dealloc {
  live--;
}
- (long)tag {
  return 21;
}
@end

@interface TzProbeChild : TzProbe
@end

@implementation TzProbeChild
@end

// A class whose objects any thread may use and release.
@interface TzShared : NSObject
@end

@implementation TzShared
- (instancetype)init {
  if ((self = [super init])) {
    __atomic_add_fetch(&live, 1, __ATOMIC_SEQ_CST);
  }
  return self;
}
- (void)dealloc {
  __atomic_sub_fetch(&live, 1, __ATOMIC_SEQ_CST);
}
@end

// +0 results, as a C function returns an object under ARC.
TzProbeChild *tzobjc_make_child(void) { return [TzProbeChild new]; }
TzProbe *tzobjc_make(void) { return [TzProbe new]; }
TzShared *tzobjc_make_shared(void) { return [TzShared new]; }

// A +1 result: the caller owns the reference.
// (NS_RETURNS_RETAINED of <Foundation/NSObjCRuntime.h>, spelled out to stay off Foundation.)
TzProbe *tzobjc_new_probe(void) __attribute__((ns_returns_retained));
TzProbe *tzobjc_new_probe(void) { return [TzProbe new]; }

// nil when `present` is 0.
TzProbe *tzobjc_maybe(int present) { return present ? [TzProbe new] : nil; }
TzProbe *tzobjc_nil(void) { return nil; }

long tzobjc_tag(TzProbe *probe) { return [probe tag]; }
long tzobjc_live(void) { return __atomic_load_n(&live, __ATOMIC_SEQ_CST); }

int tzobjc_sel_is(SEL selector, const char *name) {
  return strcmp(sel_getName(selector), name) == 0;
}

// A global object that a data symbol reads (M06 with M01).
TzProbe *tzobjc_global;
void tzobjc_set_global(int present) { tzobjc_global = present ? [TzProbe new] : nil; }
