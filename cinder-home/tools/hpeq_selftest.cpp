// Host self-test for src/hp_eq.h (run by tools/selftests.sh, from build.sh). Plays the control
// file cinder-ui writes (hpeq_fixture.txt: preamp -6 dB, +6 dB peak at 1 kHz, -6 dB low shelf at
// 100 Hz) and measures what comes out.
#include "../src/hp_eq.h"

#include <cmath>
#include <cstdio>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

static int g_fail = 0;
static void check(bool ok, const char* what)
{
    std::printf("  %s %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) g_fail = 1;
}

static std::string g_file;

// Gain in dB through the EQ of a sine at `hz`, S16 stereo at `rate` (both channels must agree).
static double gain_db(double hz, unsigned rate, int fmt = CM_FMT_S16_LE, double amp = 0.25)
{
    const int sb = cm_sample_bytes(fmt);
    const double full = fmt == CM_FMT_S16_LE ? 32767.0 : fmt == CM_FMT_S32_LE ? 2147483647.0 : 8388607.0;
    const size_t n = rate;   // one second; the first half lets the filters settle
    std::vector<uint8_t> buf(n * 2 * sb);
    std::vector<double> in(n);
    for (size_t i = 0; i < n; i++) {
        in[i] = amp * full * std::sin(2 * M_PI * hz * (double)i / rate);
        for (int c = 0; c < 2; c++) hpeq_store(&buf[(i * 2 + c) * sb], fmt, sb, in[i]);
    }
    hpeq_state s = {};
    // In uneven pieces, as the hooks get them: the filters must carry across calls.
    size_t done = 0, piece = 333;
    while (done < n) {
        const size_t k = done + piece > n ? n - done : piece;
        if (!hpeq_apply(&s, g_file.data(), g_file.size(), 1, &buf[done * 2 * sb], k, fmt, 2, rate)) return 99;
        done += k;
        piece = piece * 7 % 1000 + 50;
    }
    double si = 0, so = 0;
    for (size_t i = n / 2; i < n; i++) {
        const double l = hpeq_load(&buf[i * 2 * sb], fmt), r = hpeq_load(&buf[(i * 2 + 1) * sb], fmt);
        if (l != r) return 98;
        si += in[i] * in[i];
        so += l * l;
    }
    return 10 * std::log10(so / si);
}

int main()
{
    std::ifstream f("hpeq_fixture.txt");
    std::stringstream ss;
    ss << f.rdbuf();
    g_file = ss.str();
    check(!g_file.empty(), "hpeq_fixture.txt read");

    double c[HPEQ_MAX][5];
    check(hpeq_parse(g_file.data(), g_file.size(), 44100, c) == 2, "44100 line: two filters");
    check(hpeq_parse(g_file.data(), g_file.size(), 384000, c) == 2, "384000 line: two filters");
    check(hpeq_parse(g_file.data(), g_file.size(), 11025, c) == 0, "no line for 11025");
    check(hpeq_parse("44100 1 0 0 0\n", 14, 44100, c) == 0, "a line short of five numbers is refused");

    for (unsigned rate : { 44100u, 48000u, 96000u, 192000u }) {
        char what[96];
        const double g1k = gain_db(1000, rate), g10k = gain_db(10000, rate), g30 = gain_db(30, rate);
        std::snprintf(what, sizeof what, "%u Hz: 1 kHz %.2f dB (0), 10 kHz %.2f (-6), 30 Hz %.2f (-12)",
                      rate, g1k, g10k, g30);
        check(std::fabs(g1k) < 0.3 && std::fabs(g10k + 6) < 0.3 && std::fabs(g30 + 12) < 0.6, what);
    }
    check(std::fabs(gain_db(1000, 44100, CM_FMT_S24_3LE)) < 0.3, "S24_3LE: 1 kHz at 0 dB");
    check(std::fabs(gain_db(10000, 44100, CM_FMT_S24_LE) + 6) < 0.3, "S24_LE: 10 kHz at -6 dB");
    check(std::fabs(gain_db(10000, 44100, CM_FMT_S32_LE) + 6) < 0.3, "S32_LE: 10 kHz at -6 dB");

    // A rate with no line passes byte for byte, and says it did nothing.
    {
        std::vector<uint8_t> b(400);
        for (size_t i = 0; i < b.size(); i++) b[i] = (uint8_t)(i * 37);
        const std::vector<uint8_t> orig = b;
        hpeq_state s = {};
        const int did = hpeq_apply(&s, g_file.data(), g_file.size(), 1, b.data(), 100, CM_FMT_S16_LE, 2, 11025);
        check(!did && b == orig, "11025 Hz: untouched");
    }
    // Full scale in, never wrapped: a +6 dB filter with no level cut clamps.
    {
        const std::string hot = "44100 1.9953 0 0 0 0\n";
        std::vector<uint8_t> b(4);
        hpeq_store(&b[0], CM_FMT_S16_LE, 2, 30000);
        hpeq_store(&b[2], CM_FMT_S16_LE, 2, -30000);
        hpeq_state s = {};
        hpeq_apply(&s, hot.data(), hot.size(), 1, b.data(), 1, CM_FMT_S16_LE, 2, 44100);
        check(hpeq_load(&b[0], CM_FMT_S16_LE) == 32767 && hpeq_load(&b[2], CM_FMT_S16_LE) == -32768,
              "over full scale: clamped, not wrapped");
    }
    // A new file generation restarts the filters with the new coefficients.
    {
        const std::string half = "44100 0.5 0 0 0 0\n";
        std::vector<uint8_t> b(4);
        hpeq_store(&b[0], CM_FMT_S16_LE, 2, 1000);
        hpeq_store(&b[2], CM_FMT_S16_LE, 2, 1000);
        hpeq_state s = {};
        hpeq_apply(&s, g_file.data(), g_file.size(), 1, b.data(), 1, CM_FMT_S16_LE, 2, 44100);
        hpeq_store(&b[0], CM_FMT_S16_LE, 2, 1000);
        hpeq_apply(&s, half.data(), half.size(), 2, b.data(), 1, CM_FMT_S16_LE, 1, 44100);
        check(hpeq_load(&b[0], CM_FMT_S16_LE) == 500, "new generation: new coefficients, mono too");
    }
    return g_fail;
}
