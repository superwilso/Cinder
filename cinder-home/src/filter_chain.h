/* filter_chain.h — the effect-chain list libcinder_mono.so hands Sony's FilterChain::Create.
 *
 * WHY. Sony's sound library builds its effect chain from a fixed list of filter names, one list per
 * firmware (analysis/RE_sound_service.md section 3):
 *
 *   stock A50 1.02  i2f dseeai heq dynamicnormalizer attn eq6band vpt clearphase alc dcphaselinear vinylizer f2i
 *   Walkman One     i2f dseehxcustom heq dynamicnormalizer attn eq10band eqtone alc dcphaselinear vinylizer f2i
 *
 * The stock list has no `eq10band` and no `eqtone`, so on a player with neither Wampy nor Walkman One
 * Cinder's 10-band EQ (Bluetooth and USB-DAC; the jack's curve is in the DAC) and Tone Control had
 * nowhere to run. The same stock library builds both filters — its CreateFilter knows all 18 names —
 * so the fix is two names in the list, after `eq6band`. Walkman One's library is a WM1-class build
 * whose list already carries them, which was the one audible reason left to install it over Cinder.
 * A list that already has `eq10band` (Walkman One, Wampy's own) or has no `eq6band` is left alone.
 *
 * THE LAYOUT is libc++'s (std::__1, the device's libc++ 3.9; read back from Sony's own Create, which
 * tests bit 0 of byte 0 and takes the data at +1 or from the third word):
 *   short (bit 0 of byte 0 clear): byte 0 = length << 1, the bytes from byte 1, NUL-terminated
 *   long  (bit 0 set):             word 0 = capacity | 1, word 1 = length, word 2 = pointer
 * std::vector<std::string> is three pointers: begin, end, end of capacity.
 *
 * Plain C, no allocation, so the shim and its test (tools/mono_shim_test.c) share it.
 */
#ifndef CINDER_FILTER_CHAIN_H
#define CINDER_FILTER_CHAIN_H

#include <stddef.h>
#include <string.h>

/* pst::services::sound::mobile::FilterChain::Create(DynamicAllocPacketPool const*,
 * std::vector<std::string> const&), a member function returning void. */
#define FC_CREATE "_ZN3pst8services5sound6mobile11FilterChain6CreateEPKNS1_22DynamicAllocPacketPoolERKNSt3__16vectorINS7_12basic_stringIcNS7_11char_traitsIcEENS7_9allocatorIcEEEENSC_ISE_EEEE"

typedef struct { size_t w[3]; } cxx_str;
typedef struct { const cxx_str *b, *e, *cap; } cxx_vec;

/* Longest list the edit takes; Sony's are 12 names. */
#define FC_MAX 32

static inline const char* cxx_str_data(const cxx_str* s, size_t* n)
{
    const unsigned char* p = (const unsigned char*)s;
    if (!(p[0] & 1)) { *n = p[0] >> 1; return (const char*)p + 1; }
    *n = s->w[1];
    return (const char*)s->w[2];
}

static inline int cxx_str_is(const cxx_str* s, const char* name)
{
    size_t n;
    const char* d = cxx_str_data(s, &n);
    return n == strlen(name) && memcmp(d, name, n) == 0;
}

/* A string pointing at `name`, which must outlive it; never freed, so only for a const& argument. */
static inline cxx_str cxx_str_make(const char* name)
{
    cxx_str s;
    size_t n = strlen(name);
    memset(&s, 0, sizeof s);
    if (n < sizeof s - 1) {
        ((unsigned char*)&s)[0] = (unsigned char)(n << 1);
        memcpy((char*)&s + 1, name, n);
    } else {
        s.w[0] = (n + 1) | 1;
        s.w[1] = n;
        s.w[2] = (size_t)name;
    }
    return s;
}

/* Writes the stock list with `eq10band eqtone` after `eq6band` into out[FC_MAX] and returns its
 * length, or returns 0 when the list is to pass unchanged. The other names are bitwise copies that
 * share the caller's storage, which Create only reads. */
static inline size_t fc_add_eq(const cxx_vec* in, cxx_str* out)
{
    size_t n = (size_t)(in->e - in->b), six = n;
    if (n + 2 > FC_MAX) return 0;
    for (size_t i = 0; i < n; i++) {
        if (cxx_str_is(&in->b[i], "eq10band")) return 0;
        if (cxx_str_is(&in->b[i], "eq6band")) six = i;
    }
    if (six == n) return 0;
    memcpy(out, in->b, (six + 1) * sizeof *out);
    out[six + 1] = cxx_str_make("eq10band");
    out[six + 2] = cxx_str_make("eqtone");
    memcpy(out + six + 3, in->b + six + 1, (n - six - 1) * sizeof *out);
    return n + 2;
}

#endif
