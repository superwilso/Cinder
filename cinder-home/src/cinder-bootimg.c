/* cinder-bootimg — the player's boot image (the `bootimg` partition, p8), for the `kernel` option.
 *
 *   cinder-bootimg info SRC              len=N cinder=0|1, or exit 1 when SRC is not a boot image
 *   cinder-bootimg copy SRC DST          the image alone (its exact length), checked first
 *   cinder-bootimg swap SRC ZIMAGE DST   SRC with its kernel replaced by ZIMAGE, marked as Cinder's
 *   cinder-bootimg same SRC IMG          exit 0 when SRC starts with exactly IMG's bytes
 *
 * SRC may be the partition itself (/dev/block/mmcblk0p8): only the image's own length is read.
 *
 * WHY ON THE PLAYER. The ramdisk is the player's own (stock and Walkman One differ), so the image is
 * put together here from what the partition already holds: the header's addresses and command line
 * and the ramdisk are kept byte for byte, and only the kernel changes. The layout is kernel/bootimg.py's
 * (Android boot header v0, page 2048; kernel and ramdisk each behind MediaTek's 512-byte header), and
 * tools/test_kernel.sh checks this against it.
 *
 * THE MARK. The header's 16-byte name is empty in Sony's images; Cinder's say "cinder". That is how a
 * reinstall knows the partition holds Cinder's kernel and must not be saved as the original. */
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define PAGE 2048u
#define MAX_IMG (16u << 20)
#define MTK_MAGIC 0x58881688u
#define MARK "cinder"

static uint32_t rd32(const uint8_t* p) { return p[0] | p[1] << 8 | p[2] << 16 | (uint32_t)p[3] << 24; }
static void wr32(uint8_t* p, uint32_t v) { p[0] = v; p[1] = v >> 8; p[2] = v >> 16; p[3] = v >> 24; }
static uint32_t padded(uint32_t n) { return (n + PAGE - 1) / PAGE * PAGE; }

/* ── SHA-1, for the header's id (mkbootimg's: kernel, ramdisk, second stage, each with its size) ── */
typedef struct { uint32_t h[5]; uint64_t n; uint8_t buf[64]; uint32_t used; } Sha1;
static uint32_t rol(uint32_t x, int k) { return x << k | x >> (32 - k); }
static void sha1_block(Sha1* s, const uint8_t* b)
{
    uint32_t w[80], a = s->h[0], bb = s->h[1], c = s->h[2], d = s->h[3], e = s->h[4];
    for (int i = 0; i < 16; i++) w[i] = (uint32_t)b[4 * i] << 24 | b[4 * i + 1] << 16 | b[4 * i + 2] << 8 | b[4 * i + 3];
    for (int i = 16; i < 80; i++) w[i] = rol(w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16], 1);
    for (int i = 0; i < 80; i++) {
        uint32_t f, k;
        if (i < 20) { f = (bb & c) | (~bb & d); k = 0x5A827999; }
        else if (i < 40) { f = bb ^ c ^ d; k = 0x6ED9EBA1; }
        else if (i < 60) { f = (bb & c) | (bb & d) | (c & d); k = 0x8F1BBCDC; }
        else { f = bb ^ c ^ d; k = 0xCA62C1D6; }
        uint32_t t = rol(a, 5) + f + e + k + w[i];
        e = d; d = c; c = rol(bb, 30); bb = a; a = t;
    }
    s->h[0] += a; s->h[1] += bb; s->h[2] += c; s->h[3] += d; s->h[4] += e;
}
static void sha1_init(Sha1* s)
{
    static const uint32_t h0[5] = { 0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0 };
    memcpy(s->h, h0, sizeof h0);
    s->n = 0;
    s->used = 0;
}
static void sha1_add(Sha1* s, const uint8_t* p, size_t n)
{
    s->n += n;
    while (n) {
        uint32_t k = 64 - s->used < n ? 64 - s->used : (uint32_t)n;
        memcpy(s->buf + s->used, p, k);
        s->used += k; p += k; n -= k;
        if (s->used == 64) { sha1_block(s, s->buf); s->used = 0; }
    }
}
static void sha1_done(Sha1* s, uint8_t out[20])
{
    uint64_t bits = s->n * 8;
    uint8_t one = 0x80, zero = 0, len[8];
    sha1_add(s, &one, 1);
    while (s->used != 56) sha1_add(s, &zero, 1);
    for (int i = 0; i < 8; i++) len[i] = bits >> (56 - 8 * i);
    sha1_add(s, len, 8);
    for (int i = 0; i < 20; i++) out[i] = s->h[i / 4] >> (24 - 8 * (i % 4));
}

/* ── reading ── */
typedef struct { uint8_t* p; uint32_t len; } Img;

/* The image at the start of `path`: header checked, exactly its length read. */
static int load(const char* path, Img* im)
{
    FILE* f = fopen(path, "rb");
    if (!f) { fprintf(stderr, "%s: %s\n", path, strerror(errno)); return -1; }
    uint8_t h[PAGE];
    if (fread(h, 1, PAGE, f) != PAGE || memcmp(h, "ANDROID!", 8) != 0) {
        fprintf(stderr, "%s: not a boot image\n", path);
        fclose(f);
        return -1;
    }
    uint32_t ks = rd32(h + 8), rs = rd32(h + 16), ss = rd32(h + 24), page = rd32(h + 36);
    if (page != PAGE || ss != 0 || ks < 512 || rs < 512 || ks > MAX_IMG || rs > MAX_IMG) {
        fprintf(stderr, "%s: unexpected layout (page %u, second %u)\n", path, page, ss);
        fclose(f);
        return -1;
    }
    uint32_t len = PAGE + padded(ks) + padded(rs);
    if (len > MAX_IMG) { fprintf(stderr, "%s: %u bytes, over 16 MiB\n", path, len); fclose(f); return -1; }
    im->p = malloc(len);
    im->len = len;
    memcpy(im->p, h, PAGE);
    size_t got = fread(im->p + PAGE, 1, len - PAGE, f);
    fclose(f);
    if (got != len - PAGE) { fprintf(stderr, "%s: short read\n", path); return -1; }
    const uint8_t* k = im->p + PAGE;
    const uint8_t* r = im->p + PAGE + padded(ks);
    if (rd32(k) != MTK_MAGIC || rd32(k + 4) != ks - 512 || memcmp(k + 8, "KERNEL", 7) != 0
        || rd32(r) != MTK_MAGIC || rd32(r + 4) != rs - 512) {
        fprintf(stderr, "%s: the kernel or ramdisk is not in MediaTek's wrapper\n", path);
        return -1;
    }
    return 0;
}

static int marked(const Img* im) { return memcmp(im->p + 48, MARK, sizeof MARK) == 0; }

static int write_all(const char* path, const uint8_t* p, uint32_t n)
{
    FILE* f = fopen(path, "wb");
    if (!f || fwrite(p, 1, n, f) != n || fclose(f) != 0) {
        fprintf(stderr, "%s: cannot write\n", path);
        return -1;
    }
    return 0;
}

/* SRC with its kernel replaced: header fields kept, the ramdisk copied as it is. */
static int swap(const Img* src, const char* zpath, const char* out)
{
    FILE* f = fopen(zpath, "rb");
    if (!f) { fprintf(stderr, "%s: %s\n", zpath, strerror(errno)); return -1; }
    uint8_t* z = malloc(MAX_IMG);
    size_t zn = fread(z, 1, MAX_IMG, f);
    fclose(f);
    if (zn < 0x30 || zn >= MAX_IMG || rd32(z + 0x24) != 0x016F2818) {
        fprintf(stderr, "%s: not an ARM zImage\n", zpath);
        return -1;
    }
    uint32_t ks = 512 + (uint32_t)zn, rs = rd32(src->p + 16);
    const uint8_t* r = src->p + PAGE + padded(rd32(src->p + 8));
    uint32_t len = PAGE + padded(ks) + padded(rs);
    if (len > MAX_IMG) { fprintf(stderr, "the new image would be over 16 MiB\n"); return -1; }
    uint8_t* o = calloc(1, len);
    memcpy(o, src->p, PAGE);                 /* addresses, tags, page size, command line */
    wr32(o + 8, ks);
    memset(o + 48, 0, 16);
    memcpy(o + 48, MARK, sizeof MARK);
    uint8_t* k = o + PAGE;
    wr32(k, MTK_MAGIC);
    wr32(k + 4, (uint32_t)zn);
    memset(k + 8, 0, 32);
    memcpy(k + 8, "KERNEL", 6);
    memset(k + 40, 0xff, 512 - 40);
    memcpy(k + 512, z, zn);
    memcpy(o + PAGE + padded(ks), r, rs);
    Sha1 s;
    uint8_t sz[4], id[20];
    sha1_init(&s);
    sha1_add(&s, k, ks);   wr32(sz, ks); sha1_add(&s, sz, 4);
    sha1_add(&s, o + PAGE + padded(ks), rs); wr32(sz, rs); sha1_add(&s, sz, 4);
    wr32(sz, 0); sha1_add(&s, sz, 4);        /* no second stage */
    sha1_done(&s, id);
    memset(o + 576, 0, 32);
    memcpy(o + 576, id, 20);
    return write_all(out, o, len);
}

int main(int argc, char** argv)
{
    Img a, b;
    if (argc == 3 && strcmp(argv[1], "info") == 0) {
        if (load(argv[2], &a)) return 1;
        printf("len=%u cinder=%d\n", a.len, marked(&a));
        return 0;
    }
    if (argc == 4 && strcmp(argv[1], "copy") == 0)
        return load(argv[2], &a) || write_all(argv[3], a.p, a.len) ? 1 : 0;
    if (argc == 5 && strcmp(argv[1], "swap") == 0)
        return load(argv[2], &a) || swap(&a, argv[3], argv[4]) ? 1 : 0;
    if (argc == 4 && strcmp(argv[1], "same") == 0) {
        if (load(argv[3], &b)) return 2;
        FILE* f = fopen(argv[2], "rb");
        if (!f) return 2;
        uint8_t* p = malloc(b.len);
        size_t got = fread(p, 1, b.len, f);
        fclose(f);
        return got == b.len && memcmp(p, b.p, b.len) == 0 ? 0 : 1;
    }
    fputs("usage: cinder-bootimg info SRC | copy SRC DST | swap SRC ZIMAGE DST | same SRC IMG\n", stderr);
    return 2;
}
