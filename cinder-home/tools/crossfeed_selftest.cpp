// Host self-test for src/crossfeed.h (run by tools/selftests.sh, from build.sh).
#include "../src/crossfeed.h"

#include <cmath>
#include <cstdio>
#include <vector>

static int g_fail = 0;
static void check(bool ok, const char* what)
{
    std::printf("  %s %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) g_fail = 1;
}

// RMS in dB (re full scale) of one channel of an S16 stereo buffer, skipping the first `skip`
// frames while the filter settles.
static double rms_db(const std::vector<int16_t>& v, int ch, size_t skip)
{
    double s = 0;
    size_t n = 0;
    for (size_t i = skip; i < v.size() / 2; i++, n++) s += (double)v[2 * i + ch] * v[2 * i + ch];
    return 20 * std::log10(std::sqrt(s / (double)n) / 32768.0 + 1e-12);
}

static std::vector<int16_t> tone(double hz, double l_amp, double r_amp, unsigned rate, size_t n)
{
    std::vector<int16_t> v(2 * n);
    for (size_t i = 0; i < n; i++) {
        const double x = std::sin(2 * M_PI * hz * (double)i / rate);
        v[2 * i] = (int16_t)std::lrint(32767 * l_amp * x);
        v[2 * i + 1] = (int16_t)std::lrint(32767 * r_amp * x);
    }
    return v;
}

int main()
{
    const unsigned rate = 44100;
    const size_t n = rate;   // one second

    std::printf("test 1: the centre passes exactly unchanged\n");
    {
        auto v = tone(100, 0.5, 0.5, rate, n);
        const auto orig = v;
        cf_state s = {};
        check(cf_apply(&s, (uint8_t*)v.data(), n, CM_FMT_S16_LE, rate) == 1, "applied");
        check(v == orig, "S16: L = R in, the same bytes out");
        std::vector<int32_t> w(2 * 1000);
        for (size_t i = 0; i < 1000; i++) w[2 * i] = w[2 * i + 1] = (int32_t)(i * 2000003u);
        const auto w0 = w;
        cf_state s2 = {};
        cf_apply(&s2, (uint8_t*)w.data(), 1000, CM_FMT_S32_LE, 192000);
        check(w == w0, "S32: the same");
    }

    std::printf("test 2: a hard-left bass note reaches the right ear 4.5 dB down\n");
    {
        auto v = tone(60, 0.5, 0.0, rate, n);
        const double in = rms_db(v, 0, 4410);
        cf_state s = {};
        cf_apply(&s, (uint8_t*)v.data(), n, CM_FMT_S16_LE, rate);
        const double near = rms_db(v, 0, 4410) - in, far = rms_db(v, 1, 4410) - in;
        char m[96];
        std::snprintf(m, sizeof m, "near %.2f dB, far %.2f dB, apart %.2f dB", near, far, near - far);
        check(std::fabs((near - far) - 4.5) < 0.6, m);
        check(far > -10.0 && far < -8.0, "far ear about -8.6 dB");
    }

    std::printf("test 3: treble stays where it was panned\n");
    {
        auto v = tone(10000, 0.5, 0.0, rate, n);
        const double in = rms_db(v, 0, 4410);
        cf_state s = {};
        cf_apply(&s, (uint8_t*)v.data(), n, CM_FMT_S16_LE, rate);
        char m[96];
        std::snprintf(m, sizeof m, "near %.2f dB, far %.2f dB", rms_db(v, 0, 4410) - in, rms_db(v, 1, 4410) - in);
        check(std::fabs(rms_db(v, 0, 4410) - in) < 0.3 && rms_db(v, 1, 4410) - in < -25.0, m);
    }

    std::printf("test 4: opposite full-scale swings clamp, never wrap\n");
    {
        std::vector<int16_t> v(2 * 4000);
        for (size_t i = 0; i < 4000; i++) {
            const int16_t x = (i / 200) % 2 ? 32767 : -32768;
            v[2 * i] = x;
            v[2 * i + 1] = (int16_t)(-1 - x);
        }
        cf_state s = {};
        cf_apply(&s, (uint8_t*)v.data(), 4000, CM_FMT_S16_LE, rate);
        bool sane = true;
        for (size_t i = 200; i < 4000; i++) {
            const int16_t x = (i / 200) % 2 ? 32767 : -32768;
            // Each output keeps the sign of its own input: a wrap would flip it.
            if ((x > 0) != (v[2 * i] >= 0)) sane = false;
        }
        check(sane, "no sample flipped sign");
    }

    std::printf("test 5: both 24-bit layouts match S32's arithmetic\n");
    {
        const size_t m = 2000;
        std::vector<uint8_t> p3(m * 6), p4(m * 8);
        std::vector<int32_t> p32(m * 2);
        for (size_t i = 0; i < m; i++) {
            const int32_t l = (int32_t)std::lrint(4000000 * std::sin(i * 0.01));
            const int32_t r = (int32_t)std::lrint(-3000000 * std::sin(i * 0.013));
            const int32_t lr[2] = { l, r };
            for (int c = 0; c < 2; c++) {
                for (int k = 0; k < 3; k++) {
                    p3[i * 6 + c * 3 + k] = (uint8_t)(lr[c] >> (8 * k));
                    p4[i * 8 + c * 4 + k] = (uint8_t)(lr[c] >> (8 * k));
                }
                p4[i * 8 + c * 4 + 3] = lr[c] < 0 ? 0xFF : 0;
                p32[i * 2 + c] = lr[c];
            }
        }
        cf_state a = {}, b = {}, c = {};
        cf_apply(&a, p3.data(), m, CM_FMT_S24_3LE, 96000);
        cf_apply(&b, p4.data(), m, CM_FMT_S24_LE, 96000);
        cf_apply(&c, (uint8_t*)p32.data(), m, CM_FMT_S32_LE, 96000);
        bool same = true;
        for (size_t i = 0; i < m && same; i++)
            for (int ch = 0; ch < 2; ch++) {
                const int32_t v3 = cm_s24(&p3[i * 6 + ch * 3]), v4 = cm_s24(&p4[i * 8 + ch * 4]);
                if (v3 != p32[i * 2 + ch] || v4 != p32[i * 2 + ch]) same = false;
                if ((p4[i * 8 + ch * 4 + 3] == 0xFF) != (v4 < 0)) same = false;
            }
        check(same, "S24_3LE, S24_LE and S32 agree sample for sample");
    }

    std::printf("test 6: what it does not know, it leaves alone\n");
    {
        uint8_t b[16] = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16 };
        cf_state s = {};
        check(cf_apply(&s, b, 2, 14 /* FLOAT_LE */, rate) == 0 && b[0] == 1 && b[15] == 16, "float");
        check(cf_apply(&s, b, 2, CM_FMT_S16_LE, 0) == 0 && b[0] == 1, "no rate");
    }

    std::printf("test 7: the series is exp\n");
    {
        bool close = true;
        for (double x = 0; x <= 1.0; x += 0.05) close = close && std::fabs(cf_exp_neg(x) - std::exp(-x)) < 1e-12;
        check(close, "cf_exp_neg(x) = exp(-x) on 0..1");
    }

    std::printf(g_fail ? "FAILED\n" : "all passed\n");
    return g_fail;
}
