// Host self-test for src/rgain.h (run by tools/selftests.sh, from build.sh).
#include "../src/rgain.h"

#include <cmath>
#include <cstdio>
#include <vector>

static int g_fail = 0;
static void check(bool ok, const char* what)
{
    std::printf("  %s %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) g_fail = 1;
}

int main()
{
    const int32_t half = RG_UNITY / 2;

    std::printf("test 1: the control file\n");
    check(rg_parse("8388608\n", 8) == half, "Q24 text");
    check(rg_parse("", 0) == RG_UNITY, "empty is unity");
    check(rg_parse("12x", 3) == RG_UNITY, "junk is unity");
    check(rg_parse("99999999999", 11) == RG_UNITY, "over 4x is unity, no overflow");
    check(rg_parse("1000", 4) == RG_UNITY, "under 1/64 is unity");

    std::printf("test 2: unity leaves the bytes alone\n");
    {
        std::vector<int16_t> v = { 1000, -1000, 32767, -32768 };
        const auto o = v;
        rg_state s = {};
        check(rg_apply(&s, (uint8_t*)v.data(), 2, 2, CM_FMT_S16_LE, RG_UNITY) == 0 && v == o, "S16");
    }

    std::printf("test 3: a held gain is exact, after the first buffer's ramp\n");
    {
        rg_state s = { half };
        std::vector<int16_t> v(2 * 100, 10000);
        rg_apply(&s, (uint8_t*)v.data(), 100, 2, CM_FMT_S16_LE, half);
        bool ok = true;
        for (int16_t x : v) ok = ok && x == 5000;
        check(ok, "-6 dB: 10000 -> 5000");
    }

    std::printf("test 4: a change ramps across the buffer, it does not step\n");
    {
        rg_state s = {};
        std::vector<int16_t> v(1000, 20000);   // mono, 1000 frames
        rg_apply(&s, (uint8_t*)v.data(), 1000, 1, CM_FMT_S16_LE, half);
        bool smooth = true;
        for (size_t i = 1; i < v.size(); i++) smooth = smooth && std::abs(v[i] - v[i - 1]) <= 11;
        char m[96];
        std::snprintf(m, sizeof m, "first %d, last %d, no jump over 11", v[0], v.back());
        check(smooth && v[0] > 19900 && v.back() == 10000, m);
        check(s.q == half, "the next buffer starts where this one ended");
    }

    std::printf("test 5: a boost clamps, never wraps; every layout agrees\n");
    {
        rg_state s = { 2 * RG_UNITY };
        std::vector<int16_t> v = { 30000, -30000 };
        rg_apply(&s, (uint8_t*)v.data(), 1, 2, CM_FMT_S16_LE, 2 * RG_UNITY);
        check(v[0] == 32767 && v[1] == -32768, "S16 at +6 dB");
        const int32_t x[3] = { 4000000, -3000001, 123 };
        std::vector<uint8_t> p3(9), p4(12);
        std::vector<int32_t> p32(x, x + 3);
        for (int i = 0; i < 3; i++)
            for (int k = 0; k < 3; k++) {
                p3[i * 3 + k] = (uint8_t)(x[i] >> (8 * k));
                p4[i * 4 + k] = (uint8_t)(x[i] >> (8 * k));
                p4[i * 4 + 3] = x[i] < 0 ? 0xFF : 0;
            }
        rg_state a = { half }, b = { half }, c = { half };
        rg_apply(&a, p3.data(), 3, 1, CM_FMT_S24_3LE, half);
        rg_apply(&b, p4.data(), 3, 1, CM_FMT_S24_LE, half);
        rg_apply(&c, (uint8_t*)p32.data(), 3, 1, CM_FMT_S32_LE, half);
        bool same = true;
        for (int i = 0; i < 3; i++) {
            const int32_t want = (int32_t)std::lround(x[i] / 2.0 + 0.0001);
            same = same && cm_s24(&p3[i * 3]) == want && cm_s24(&p4[i * 4]) == want && p32[i] == want;
            same = same && (p4[i * 4 + 3] == 0xFF) == (want < 0);
        }
        check(same, "S24_3LE, S24_LE and S32 halve alike");
    }

    std::printf("test 6: what it does not know, it leaves alone\n");
    {
        uint8_t b[8] = { 1, 2, 3, 4, 5, 6, 7, 8 };
        rg_state s = {};
        check(rg_apply(&s, b, 1, 2, 14 /* FLOAT_LE */, half) == 0 && b[0] == 1, "float");
    }

    std::printf(g_fail ? "FAILED\n" : "all passed\n");
    return g_fail;
}
