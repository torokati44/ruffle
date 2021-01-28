

#define getenv(NAME) 0

long int strtol(const char *str, char **endptr, int base);

long long int strtoll (const char* str, char** endptr, int base);
void qsort(void *base, unsigned long nitems, unsigned long size, int (*compar)(const void *, const void*));
void abort(void);

double strtod(const char *str, char **endptr);