
// TODO
#define strlen(what) 1



#define strcmp(S1, S2) 0
#define strncmp(S1, S2, N) 0

void *memcpy(void *dest, const void * src, unsigned long n);

int memcmp(const void *str1, const void *str2, unsigned long n);
void * memset ( void * ptr, int value, unsigned long num );

void free(void *ptr);
void* malloc (unsigned long size);
void* realloc (void* ptr, unsigned long size);
unsigned long strspn ( const char * str1, const char * str2 );
const void * memchr ( const void * ptr, int value, unsigned long num );
const char * strchr ( const char * str, int character );
const char * strrchr ( const char * str, int character );