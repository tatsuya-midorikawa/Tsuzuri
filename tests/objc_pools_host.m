// The Objective-C host of tests/objc_pools.mjs (M10). It is compiled without ARC, so that
// `tzpool_autorelease` and `tzpool_make` put their objects into the autorelease pool of the
// calling thread for certain (under ARC a +0 result can skip the pool). `tzpool_live` counts the
// objects between init and dealloc: the expectations of the suite come from these counts, which
// the Tsuzuri compiler does not produce.
#import <Foundation/Foundation.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static _Atomic long live;
// The stack of the thread that called `tzpool_stack_base`: where it was, and the lowest address
// that a `-description` saw since. One tzroutine uses the two at a time.
static uintptr_t stack_base;
static uintptr_t stack_lowest;

@interface TzPooled : NSObject
@end

@implementation TzPooled
- (instancetype)init {
  if ((self = [super init])) {
    atomic_fetch_add(&live, 1);
  }
  return self;
}
- (void)dealloc {
  atomic_fetch_sub(&live, 1);
  [super dealloc];
}
- (long)tag {
  return 21;
}
// Foundation calls this from inside its formatting, below its own frames.
- (NSString *)description {
  char mark;
  uintptr_t here = (uintptr_t)&mark;
  if (stack_base != 0 && here < stack_lowest) {
    stack_lowest = here;
  }
  return @"pooled";
}
@end

// Autoreleases `count` new objects on the calling thread.
void tzpool_autorelease(long count) {
  for (long i = 0; i < count; i++) {
    [[[TzPooled alloc] init] autorelease];
  }
}

long tzpool_live(void) { return atomic_load(&live); }

// A +0 result that is in the pool of the calling thread.
TzPooled *tzpool_make(void) { return [[[TzPooled alloc] init] autorelease]; }

long tzpool_tag(TzPooled *pooled) { return [pooled tag]; }

// The calling OS thread.
long tzpool_thread(void) { return (long)(uintptr_t)pthread_self(); }

static void report_at_exit(void) {
  printf("exit %ld\n", atomic_load(&live));
  fflush(stdout);
}

// Prints the count of live objects when the process exits: after `main` has returned and the
// `@main` that the compiler wrote has popped its pool.
void tzpool_report_at_exit(void) { atexit(report_at_exit); }

void tzpool_stack_base(void) {
  char mark;
  stack_base = (uintptr_t)&mark;
  stack_lowest = stack_base;
}

// The bytes of stack between `tzpool_stack_base` and the deepest `-description` since.
long tzpool_stack_used(void) { return (long)(stack_base - stack_lowest); }

// Foundation at work on the calling thread: a dictionary and an array of objects, formatted
// (which calls `-description` on each from inside Foundation) `rounds` times. Everything is
// autoreleased. Returns the total length of the texts.
long tzpool_foundation_work(long rounds) {
  long total = 0;
  for (long round = 0; round < rounds; round++) {
    TzPooled *pooled = [[[TzPooled alloc] init] autorelease];
    NSDictionary *table = [NSDictionary dictionaryWithObjectsAndKeys:pooled, @"object",
                                                                      [NSNumber numberWithLong:round], @"round",
                                                                      [NSArray arrayWithObjects:pooled, pooled, nil], @"pair",
                                                                      nil];
    NSString *text = [NSString stringWithFormat:@"%@ %ld %@", table, round, [NSDate date]];
    total += (long)[text length];
  }
  return total;
}
