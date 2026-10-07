/* Names that bindgen must rename for Tsuzuri (GUIDE D6 rules of E11). */
int type(int value);
double sqrt(double x);
int abs(int x);
void glClearColor(float red, float green, float blue, float alpha);
int SDL_Init(unsigned int flags);
int XMLParse(int depth);
int fooBar(void);
int foo_bar(void);
struct point { int type; int camelCase; };
typedef long point_t;
typedef int point;
struct SDL_Rect { int x, y, w, h; };
struct Vec { double x; };
struct Display { double x; };
typedef int _1x;
enum keywords { match, then, _, ignore, red_light };
int redLight(void);
struct twice { int fooBar; int foo_bar; };
