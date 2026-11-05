/* uuid.c — see uuid.h. Random (version 4) UUIDs from /dev/urandom, and their text form. */
#include "uuid.h"
#include <fcntl.h>
#include <openssl/sha.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

void uuid_generate_random(uuid_t out) {
  int fd = open("/dev/urandom", O_RDONLY | O_CLOEXEC);
  ssize_t got = fd >= 0 ? read(fd, out, 16) : -1;
  if (fd >= 0)
    close(fd);
  if (got != 16) { /* no /dev/urandom: the clock, so the result is at least not constant */
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    for (int i = 0; i < 16; i++)
      out[i] = (unsigned char)((ts.tv_nsec >> (i % 4 * 8)) ^ (ts.tv_sec >> (i % 8)) ^ (i * 37));
  }
  out[6] = (out[6] & 0x0F) | 0x40; /* version 4 */
  out[8] = (out[8] & 0x3F) | 0x80; /* RFC 4122 variant */
}

void uuid_generate(uuid_t out) { uuid_generate_random(out); }

void uuid_generate_sha1(uuid_t out, const uuid_t ns, const char *name, size_t len) {
  unsigned char d[SHA_DIGEST_LENGTH];
  SHA_CTX c;
  SHA1_Init(&c);
  SHA1_Update(&c, ns, 16);
  SHA1_Update(&c, name, len);
  SHA1_Final(d, &c);
  memcpy(out, d, 16);
  out[6] = (out[6] & 0x0F) | 0x50; /* version 5 */
  out[8] = (out[8] & 0x3F) | 0x80;
}

static int hex(int c) {
  if (c >= '0' && c <= '9')
    return c - '0';
  if (c >= 'a' && c <= 'f')
    return c - 'a' + 10;
  if (c >= 'A' && c <= 'F')
    return c - 'A' + 10;
  return -1;
}

int uuid_parse(const char *in, uuid_t uu) {
  if (in == NULL || strlen(in) != 36)
    return -1;
  for (int i = 0, b = 0; i < 36; i++) {
    if (i == 8 || i == 13 || i == 18 || i == 23) {
      if (in[i] != '-')
        return -1;
      continue;
    }
    int hi = hex(in[i]), lo = hex(in[i + 1]);
    if (hi < 0 || lo < 0)
      return -1;
    uu[b++] = (unsigned char)(hi << 4 | lo);
    i++;
  }
  return 0;
}

void uuid_unparse_lower(const uuid_t uu, char *out) {
  sprintf(out, "%02x%02x%02x%02x-%02x%02x-%02x%02x-%02x%02x-%02x%02x%02x%02x%02x%02x", uu[0], uu[1],
          uu[2], uu[3], uu[4], uu[5], uu[6], uu[7], uu[8], uu[9], uu[10], uu[11], uu[12], uu[13],
          uu[14], uu[15]);
}

#ifdef UUID_SELFTEST
int main(void) {
  uuid_t a, b;
  char s[37];
  uuid_generate_random(a);
  uuid_unparse_lower(a, s);
  /* RFC 4122's own example: the DNS namespace and "www.example.com" (as Python's uuid5 has it). */
  uuid_t dns, v5;
  char t[37];
  uuid_parse("6ba7b810-9dad-11d1-80b4-00c04fd430c8", dns);
  uuid_generate_sha1(v5, dns, "www.example.com", 15);
  uuid_unparse_lower(v5, t);
  int ok = strcmp(t, "2ed6657d-e927-568b-95e1-2665a8aea6a2") == 0 && strlen(s) == 36 && s[14] == '4' && strchr("89ab", s[19]) && uuid_parse(s, b) == 0 &&
           memcmp(a, b, 16) == 0 && uuid_parse("not-a-uuid", b) == -1 &&
           uuid_parse("123e4567-e89b-12d3-a456-42661417400Z", b) == -1;
  printf("uuid selftest: %s (%s)\n", ok ? "ok" : "FAILED", s);
  return ok ? 0 : 1;
}
#endif
