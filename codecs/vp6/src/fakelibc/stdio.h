#include <stdarg.h>

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

#define snprintf(STR, SIZE, FORMAT, ...) 0
int vsnprintf (char * s, unsigned long n, const char * format, va_list arg );
int fprintf ( int* stream, const char * format, ... );
unsigned long fwrite(const void *ptr, unsigned long size, unsigned long nmemb, void *stream);