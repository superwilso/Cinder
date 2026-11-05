/* uuid.h — the libuuid calls shairport-sync's AirPlay 2 code makes (uuid_generate_random,
 * uuid_generate_sha1, uuid_parse, uuid_unparse_lower), and uuid_generate, which its configure
 * looks for, so the build needs no util-linux. uuid.c has them; SHA-1 is OpenSSL's. */
#ifndef CINDER_UUID_H
#define CINDER_UUID_H
#include <stddef.h>
#define UUID_STR_LEN 37 /* 36 characters and the NUL */
typedef unsigned char uuid_t[16];
void uuid_generate_random(uuid_t out);
/* A name-based (version 5, SHA-1) UUID: the same name in the same namespace, the same UUID. */
void uuid_generate_sha1(uuid_t out, const uuid_t ns, const char *name, size_t len);
void uuid_generate(uuid_t out); /* what configure probes for: the same random UUID */
int uuid_parse(const char *in, uuid_t uu);
void uuid_unparse_lower(const uuid_t uu, char *out);
#endif
