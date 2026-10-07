/* One declaration for each reason that bindgen skips with W2002, in the order of
   the reasons; the E2E test checks every message. */
static int internal(void) { return 1; }
inline int inlined(void) { return 2; }
int variadic(int count, ...);
int unprototyped();
int name$dollar(void);
int __hidden(void);
int tz_export(void);
int putchar(int character);
int renamed(void) __asm__("renamed_impl");
int convention(int value) __attribute__((preserve_most));
int fortified(void *buffer __attribute__((pass_object_size(0))));
int pointer(int *value);
char character(void);
int collide(void);
int Collide(void);
union either { int a; float b; };
typedef struct { int x; } untagged;
#pragma pack(push, 1)
struct packed { int a; long b; };
#pragma pack(pop)
struct empty {};
struct bits { int flag : 1; };
struct aligned_field { int a; long b __attribute__((aligned(16))); };
struct holder { char c; };
struct quirky { int ok; int odd$name; };
struct same_fields { int fooBar; int foo_bar; };
struct hello { int x; };
typedef int Hello;
enum { WIDE = 0x100000000 };
enum { snake_case_name = 1 };
int snakeCaseName(void);
extern int counter;
