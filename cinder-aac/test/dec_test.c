/* dec_test LIB IN.aac OUT.s16 — decode an ADTS stream with LIB's aacDecoder_* exactly as cinder-home's
 * radio does (main.cpp RadioDec: open ADTS, fill, decode until NOT_ENOUGH_BITS), interleaved 16-bit
 * out. Run under qemu with each decoder and compare with ffmpeg (build.sh test). */
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>

typedef void* (*open_f)(int, unsigned);
typedef int (*fill_f)(void*, unsigned char**, const unsigned*, unsigned*);
typedef int (*dec_f)(void*, short*, int, unsigned);
typedef const int* (*info_f)(void*);

int main(int argc, char** argv)
{
    if (argc != 4) return 2;
    void* l = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!l) { fprintf(stderr, "%s\n", dlerror()); return 1; }
    open_f op = (open_f)dlsym(l, "aacDecoder_Open");
    fill_f fi = (fill_f)dlsym(l, "aacDecoder_Fill");
    dec_f de = (dec_f)dlsym(l, "aacDecoder_DecodeFrame");
    info_f in = (info_f)dlsym(l, "aacDecoder_GetStreamInfo");
    FILE* f = fopen(argv[2], "rb");
    FILE* o = fopen(argv[3], "wb");
    if (!op || !fi || !de || !in || !f || !o) return 1;
    void* h = op(2 /*TT_MP4_ADTS*/, 1);
    static unsigned char buf[4096];
    static short pcm[16384];
    long frames = 0;
    int hz = 0, ch = 0;
    size_t n;
    while ((n = fread(buf, 1, sizeof buf, f)) > 0) {
        unsigned char* p = buf;
        unsigned size = (unsigned)n, valid = (unsigned)n;
        while (valid > 0) {
            fi(h, &p, &size, &valid);
            p = buf + (n - valid);
            size = valid;
            int e;
            while ((e = de(h, pcm, 16384, 0)) == 0) {
                const int* si = in(h);
                hz = si[0]; ch = si[2];
                fwrite(pcm, sizeof(short), (size_t)si[1] * si[2], o);
                frames += si[1];
            }
            if (e != 0x1002) { fprintf(stderr, "decode error 0x%x\n", e); }
        }
    }
    printf("%s: %ld frames, %d Hz, %d ch\n", argv[1], frames, hz, ch);
    return 0;
}
