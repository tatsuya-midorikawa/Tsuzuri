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
/* Review fixes: attributes that may change a layout, unnamed structs named like a tag,
   typeof, returns_twice, and a macro that #pragma pop_macro restores. */
enum __attribute__((aligned(8))) aligned_enum { ALIGNED_ENUM_ZERO };
int take_aligned_enum(enum aligned_enum value);
enum __attribute__((mode(QI))) byte_enum { BYTE_ENUM_ZERO };
enum byte_enum give_byte_enum(void);
enum __attribute__((flag_enum)) flag_bits { FLAG_A = 1 };
int take_flags(enum flag_bits flags);
struct __attribute__((randomize_layout)) shuffled { int a; int b; };
struct tagged { double big; };
typedef struct { int small; } tagged;
double use_tagged(const tagged *value);
double use_tag(const struct tagged *value);
struct holds_wide { enum wide_tag { WIDE_TAG_BIG = 0x7fffffffffff } w; long pad; };
typedef enum { WIDE_TAG_ZERO } wide_tag;
int take_wide_tag(enum wide_tag value);
typedef long long wide_aligned __attribute__((aligned(16)));
struct with_typeof { int a; __typeof__((wide_aligned)0) b; };
int fork_twice(void) __attribute__((returns_twice));
#define LEVEL 1
#pragma push_macro("LEVEL")
#undef LEVEL
#define LEVEL 2
#pragma pop_macro("LEVEL")
