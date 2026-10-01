// soundscape_selftest — host test for the procedural soundscapes in src/soundscape.h.
//
// What a listener would catch, measured instead: each sound has a level in a sane window and no
// peak that reaches full scale; the three noises have the slopes their names promise; rain is
// bright, wind and the fire's roar are dark, crickets sit where crickets sing; a beach comes in
// waves, and they are not the same wave every time. Then what a listener would only catch too late:
// silence is bit-exact silence (the music is untouched while it is off), a level change or a
// switch of sound is a ramp and not a click, a full-scale sample saturates instead of wrapping,
// every format and rate works, and it is cheap.
//
//   SS_WAV_DIR=/tmp/ss tools/soundscape_selftest     also writes 20 s of each sound as a WAV
//   SS_VERBOSE=1 …                                    prints every measurement
#include "../src/soundscape.h"
#include <algorithm>
#include <chrono>
#include <cmath>
#include <complex>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

static int fails = 0;
static bool verbose = false;
static void check(bool ok, const char* what) {
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}
static void checkf(bool ok, const char* fmt, double a, double b = 0, double c = 0) {
    char m[256];
    std::snprintf(m, sizeof m, fmt, a, b, c);
    check(ok, m);
}

static const char* const NAMES[SS_COUNT] = { "off", "white", "pink", "dark", "rain", "storm",
                                             "beach", "stream", "wind", "fire", "night" };

static std::vector<float> render(int sound, unsigned rate, double secs, uint32_t seed = 1, float gain = 1.0f) {
    ss_state s;
    ss_init(&s, seed);
    ss_set(&s, sound, gain);
    const size_t n = (size_t)(secs * rate);
    std::vector<float> out(2 * n, 0.0f);
    // in odd-sized pieces, as an audio callback would ask for them
    size_t done = 0, piece = 333;
    while (done < n) {
        const size_t k = n - done < piece ? n - done : piece;
        ss_render(&s, &out[2 * done], k, rate);
        done += k;
        piece = piece == 333 ? 1024 : piece == 1024 ? 17 : 333;
    }
    return out;
}

// ── measurements ─────────────────────────────────────────────────────────────────────────────────
static double rms(const std::vector<float>& v, size_t from = 0, size_t to = 0) {
    if (!to) to = v.size();
    double a = 0;
    for (size_t i = from; i < to; i++) a += (double)v[i] * v[i];
    return std::sqrt(a / (double)(to - from));
}
static double peak(const std::vector<float>& v) {
    double p = 0;
    for (float x : v) p = std::fmax(p, std::fabs(x));
    return p;
}
static double db(double x) { return 20.0 * std::log10(x > 1e-12 ? x : 1e-12); }

static void fft(std::vector<std::complex<double>>& a) {
    const size_t n = a.size();
    for (size_t i = 1, j = 0; i < n; i++) {
        size_t bit = n >> 1;
        for (; j & bit; bit >>= 1) j ^= bit;
        j ^= bit;
        if (i < j) std::swap(a[i], a[j]);
    }
    for (size_t len = 2; len <= n; len <<= 1) {
        const double ang = -2 * M_PI / (double)len;
        const std::complex<double> wl(std::cos(ang), std::sin(ang));
        for (size_t i = 0; i < n; i += len) {
            std::complex<double> w(1);
            for (size_t j = 0; j < len / 2; j++) {
                const auto u = a[i + j], v = a[i + j + len / 2] * w;
                a[i + j] = u + v;
                a[i + j + len / 2] = u - v;
                w *= wl;
            }
        }
    }
}

// Average power spectrum of the left channel (Hann, 4096 points, half overlap).
static std::vector<double> spectrum(const std::vector<float>& v) {
    const size_t N = 4096;
    std::vector<double> p(N / 2, 0.0);
    int frames = 0;
    for (size_t at = 0; at + N <= v.size() / 2; at += N / 2) {
        std::vector<std::complex<double>> a(N);
        for (size_t i = 0; i < N; i++) {
            const double w = 0.5 - 0.5 * std::cos(2 * M_PI * (double)i / (double)(N - 1));
            a[i] = v[2 * (at + i)] * w;
        }
        fft(a);
        for (size_t i = 0; i < N / 2; i++) p[i] += std::norm(a[i]);
        frames++;
    }
    for (auto& x : p) x /= frames ? frames : 1;
    return p;
}
static double band(const std::vector<double>& p, unsigned rate, double lo, double hi) {
    const double hz = (double)rate / (2.0 * (double)p.size());
    double a = 0;
    for (size_t i = (size_t)(lo / hz); i < p.size() && (double)i * hz < hi; i++) a += p[i];
    return a;
}
// dB per octave from a least-squares fit of octave-band energy per Hz, between lo and hi.
static double slope(const std::vector<double>& p, unsigned rate, double lo, double hi) {
    std::vector<double> xs, ys;
    for (double f = lo; f * 2 <= hi; f *= 2) {
        xs.push_back(std::log2(f));
        ys.push_back(10 * std::log10(band(p, rate, f, 2 * f) / f));
    }
    double mx = 0, my = 0;
    for (size_t i = 0; i < xs.size(); i++) { mx += xs[i]; my += ys[i]; }
    mx /= (double)xs.size();
    my /= (double)xs.size();
    double sxy = 0, sxx = 0;
    for (size_t i = 0; i < xs.size(); i++) { sxy += (xs[i] - mx) * (ys[i] - my); sxx += (xs[i] - mx) * (xs[i] - mx); }
    return sxy / sxx;
}
// A-weighted level (dB re full-scale sine RMS ~ 0.707 is not used: this is relative between sounds).
static double a_weighted(const std::vector<double>& p, unsigned rate) {
    const double hz = (double)rate / (2.0 * (double)p.size());
    double a = 0, total = 0;
    for (size_t i = 1; i < p.size(); i++) {
        const double f = (double)i * hz, f2 = f * f;
        const double ra = 12194.0 * 12194.0 * f2 * f2
                        / ((f2 + 20.6 * 20.6) * std::sqrt((f2 + 107.7 * 107.7) * (f2 + 737.9 * 737.9)) * (f2 + 12194.0 * 12194.0));
        a += p[i] * ra * ra * 1.2589;   // +2 dB normalises A(1 kHz) to 0 dB
        total += p[i];
    }
    return 10 * std::log10(a / total);   // dB relative to the unweighted level
}
// K-weighted level (ITU-R BS.1770's loudness curve: +4 dB shelf above ~1.7 kHz, high-pass at 38 Hz),
// relative to the unweighted level. A-weighting is for quiet sounds and buries a low roar under
// 20 dB it does not deserve; K is what loudness meters use for program material.
static double k_weighted(const std::vector<double>& p, unsigned rate) {
    const double hz = (double)rate / (2.0 * (double)p.size());
    double a = 0, total = 0;
    for (size_t i = 1; i < p.size(); i++) {
        const double f = (double)i * hz, x = f / 1681.0;
        const double shelf = (1.0 + x * x * 2.5119) / (1.0 + x * x);              // +4 dB
        const double f0 = 38.0, q = 0.5;
        const double hp = f * f * f * f / ((f * f - f0 * f0) * (f * f - f0 * f0) + (f * f0 / q) * (f * f0 / q));
        a += p[i] * shelf * hp;
        total += p[i];
    }
    return 10 * std::log10(a / total);
}
// RMS envelope in `win`-second windows.
static std::vector<double> envelope(const std::vector<float>& v, unsigned rate, double win) {
    std::vector<double> e;
    const size_t w = (size_t)(win * rate) * 2;
    for (size_t at = 0; at + w <= v.size(); at += w) e.push_back(rms(v, at, at + w));
    return e;
}
// A loop shows as a PEAK in the envelope's autocorrelation at the loop's length: higher than the
// lags around it. A slow random drift shows as a smooth hill from lag 0 instead. So what is measured
// is how far any lag stands above the median of its neighbours (±2 s).
static double loop_prominence(const std::vector<double>& e, double win, double from_s, double to_s, double* at_s);
// Normalised autocorrelation of a sequence at `lag`.
static double autocorr(const std::vector<double>& e, size_t lag) {
    double m = 0;
    for (double x : e) m += x;
    m /= (double)e.size();
    double num = 0, den = 0;
    for (size_t i = 0; i < e.size(); i++) den += (e[i] - m) * (e[i] - m);
    for (size_t i = 0; i + lag < e.size(); i++) num += (e[i] - m) * (e[i + lag] - m);
    return num / den;
}

static double loop_prominence(const std::vector<double>& e, double win, double from_s, double to_s, double* at_s) {
    const size_t lo = (size_t)(from_s / win), hi = (size_t)(to_s / win), nb = (size_t)(2.0 / win);
    std::vector<double> c(hi + nb + 1);
    for (size_t lag = 1; lag < c.size() && lag < e.size(); lag++) c[lag] = autocorr(e, lag);
    double worst = -1;
    for (size_t lag = lo; lag < hi; lag++) {
        std::vector<double> around;
        for (size_t j = lag - nb; j <= lag + nb; j++) if (j != lag) around.push_back(c[j]);
        std::sort(around.begin(), around.end());
        const double p = c[lag] - around[around.size() / 2];
        if (p > worst) { worst = p; *at_s = (double)lag * win; }
    }
    return worst;
}

// The best match of a stretch anywhere else in the signal, as a normalised correlation of
// the raw samples (FFT cross-correlation). A recording on a loop matches itself at ~1.0 one loop
// later; procedural sound matches nothing.
static double best_rematch(const std::vector<float>& v, size_t tpl_at, size_t tpl_len) {
    size_t n = 1;
    while (n < v.size() / 2 + tpl_len) n <<= 1;
    std::vector<std::complex<double>> a(n), b(n);
    for (size_t i = 0; i < v.size() / 2; i++) a[i] = v[2 * i];
    double tn = 0;
    for (size_t i = 0; i < tpl_len; i++) { b[i] = v[2 * (tpl_at + i)]; tn += (double)v[2 * (tpl_at + i)] * v[2 * (tpl_at + i)]; }
    fft(a);
    fft(b);
    for (size_t i = 0; i < n; i++) a[i] *= std::conj(b[i]);
    // inverse via the conjugate trick
    for (auto& x : a) x = std::conj(x);
    fft(a);
    std::vector<double> energy(v.size() / 2 + 1, 0.0);
    for (size_t i = 0; i < v.size() / 2; i++) energy[i + 1] = energy[i] + (double)v[2 * i] * v[2 * i];
    double best = 0;
    for (size_t lag = 0; lag + tpl_len <= v.size() / 2; lag++) {
        if (lag + tpl_len > tpl_at && lag < tpl_at + tpl_len) continue;   // itself and its overlaps
        const double e = energy[lag + tpl_len] - energy[lag];
        if (e <= 0) continue;
        const double c = std::conj(a[lag]).real() / (double)n / std::sqrt(e * tn);
        best = std::fmax(best, c);
    }
    return best;
}

static void write_wav(const std::string& path, const std::vector<float>& v, unsigned rate) {
    FILE* f = std::fopen(path.c_str(), "wb");
    if (!f) return;
    const uint32_t data = (uint32_t)(v.size() * 2), riff = 36 + data, fmt_len = 16, byte_rate = rate * 4;
    const uint16_t pcm = 1, ch = 2, align = 4, bits = 16;
    std::fwrite("RIFF", 1, 4, f); std::fwrite(&riff, 4, 1, f); std::fwrite("WAVEfmt ", 1, 8, f);
    std::fwrite(&fmt_len, 4, 1, f); std::fwrite(&pcm, 2, 1, f); std::fwrite(&ch, 2, 1, f);
    std::fwrite(&rate, 4, 1, f); std::fwrite(&byte_rate, 4, 1, f); std::fwrite(&align, 2, 1, f);
    std::fwrite(&bits, 2, 1, f); std::fwrite("data", 1, 4, f); std::fwrite(&data, 4, 1, f);
    for (float x : v) {
        const int16_t s = (int16_t)std::lrint(std::fmax(-1.0, std::fmin(1.0, (double)x)) * 32767.0);
        std::fwrite(&s, 2, 1, f);
    }
    std::fclose(f);
}

int main() {
    verbose = std::getenv("SS_VERBOSE") != nullptr;
    const char* wav_dir = std::getenv("SS_WAV_DIR");
    const unsigned R = 48000;

    // ── every sound: level, peak, nothing silent, nothing broken ────────────────────────────────
    std::printf("── level and peak of every sound (60 s at 48 kHz, level 100%%) ──\n");
    double aw[SS_COUNT] = { 0 };
    for (int snd = 1; snd < SS_COUNT; snd++) {
        const auto v = render(snd, R, 60.0, (uint32_t)snd * 7919u);
        bool finite = true;
        for (float x : v) finite = finite && std::isfinite(x);
        const double r = rms(v, (size_t)R * 2, v.size()), pk = peak(v);   // skip the 1 s fade-in
        const auto p = spectrum(std::vector<float>(v.begin() + (long)R * 2, v.end()));
        aw[snd] = db(r) + k_weighted(p, R);
        const auto e = envelope(v, R, 1.0);
        double quiet = 1e9;
        for (size_t i = 2; i < e.size(); i++) quiet = std::fmin(quiet, e[i]);
        if (verbose)
            std::printf("     %-6s rms %6.1f dB  K-weighted %6.1f dB  A-weighted %6.1f dB  peak %5.3f  quietest second %6.1f dB\n",
                        NAMES[snd], db(r), aw[snd], db(r) + a_weighted(p, R), pk, db(quiet));
        char m[160];
        std::snprintf(m, sizeof m, "%s: finite, peak %.2f < 1, rms %.1f dB in [-32, -12], never silent (quietest second %.1f dB)",
                      NAMES[snd], pk, db(r), db(quiet));
        check(finite && pk < 1.0 && db(r) > -32 && db(r) < -12 && db(quiet) > -55, m);
        if (wav_dir) write_wav(std::string(wav_dir) + "/" + NAMES[snd] + ".wav",
                               std::vector<float>(v.begin(), v.begin() + (long)R * 2 * 20), R);
    }
    // Loudness across sounds: switching from one to another at the same setting should not jump.
    {
        double lo = 1e9, hi = -1e9;
        for (int snd = 1; snd < SS_COUNT; snd++) { lo = std::fmin(lo, aw[snd]); hi = std::fmax(hi, aw[snd]); }
        checkf(hi - lo < 6.0, "K-weighted loudness of all ten within 6 dB of each other (spread %.1f dB)", hi - lo);
    }

    // ── the noises are the colours their names say ──────────────────────────────────────────────
    std::printf("── spectra ──\n");
    {
        const auto w = spectrum(render(SS_WHITE, R, 20.0));
        const auto p = spectrum(render(SS_PINK, R, 20.0));
        const auto d = spectrum(render(SS_DARK, R, 20.0));
        const double sw = slope(w, R, 100, 12800), sp = slope(p, R, 100, 12800), sd = slope(d, R, 100, 6400);
        checkf(std::fabs(sw - 0.0) < 0.6, "white is flat: %.2f dB/octave", sw);
        checkf(std::fabs(sp + 3.0) < 0.7, "pink falls 3 dB/octave: %.2f", sp);
        checkf(std::fabs(sd + 6.0) < 1.0, "dark falls 6 dB/octave: %.2f", sd);
    }
    {
        const auto rain = spectrum(render(SS_RAIN, R, 30.0));
        const double hi = band(rain, R, 1000, 12000), lo = band(rain, R, 20, 1000);
        checkf(hi > lo, "rain is bright: %.1f dB more energy above 1 kHz than below", 10 * std::log10(hi / lo));
        const auto wind = spectrum(render(SS_WIND, R, 30.0));
        const double whi = band(wind, R, 1500, 20000), wlo = band(wind, R, 20, 1500);
        checkf(wlo > 10 * whi, "wind is dark: %.1f dB more below 1.5 kHz", 10 * std::log10(wlo / whi));
        const auto night = spectrum(render(SS_NIGHT, R, 30.0));
        const double crick = band(night, R, 3500, 5600), mid = band(night, R, 1000, 3500) + band(night, R, 5600, 12000);
        checkf(crick > 3 * mid, "night: the crickets' band (3.5-5.6 kHz) stands out by %.1f dB", 10 * std::log10(crick / mid));
    }

    // ── a beach comes in waves, and not the same wave twice ─────────────────────────────────────
    std::printf("── structure ──\n");
    {
        const auto v = render(SS_BEACH, R, 180.0, 4242);
        const auto e = envelope(v, R, 0.25);
        double lo = 1e9, hi = 0;
        for (size_t i = 8; i < e.size(); i++) { lo = std::fmin(lo, e[i]); hi = std::fmax(hi, e[i]); }
        checkf(db(hi) - db(lo) > 9.0, "beach: waves rise and fall (%.1f dB between the loudest and quietest quarter-second)",
               db(hi) - db(lo));
        double at = 0;
        const double prom = loop_prominence(e, 0.25, 4.0, 60.0, &at);
        checkf(prom < 0.3, "beach: no lag from 4 to 60 s stands out as a period (most prominent %.2f at %.1f s)", prom, at);
    }
    for (int snd : { SS_RAIN, SS_STORM, SS_STREAM, SS_WIND, SS_FIRE, SS_NIGHT }) {
        const auto v = render(snd, R, 120.0, 99u + (uint32_t)snd);
        const auto e = envelope(v, R, 0.1);
        double at = 0;
        const double prom = loop_prominence(e, 0.1, 2.0, 60.0, &at);
        char m[160];
        std::snprintf(m, sizeof m, "%s: no lag from 2 to 60 s stands out as a period (most prominent %.2f at %.1f s)",
                      NAMES[snd], prom, at);
        check(prom < 0.3, m);
    }
    // Not one stretch of the raw audio comes back. At 16 kHz to keep the FFT small; the engine is
    // the same at every rate.
    for (int snd = 1; snd < SS_COUNT; snd++) {
        const unsigned r16 = 16000;
        const auto v = render(snd, r16, 65.0, 555u + (uint32_t)snd);
        // A full second: a low roar has few independent samples in less, and matches by chance.
        const double m = best_rematch(v, (size_t)(5.0 * r16), (size_t)(1.0 * r16));
        char msg[160];
        std::snprintf(msg, sizeof msg, "%s: the second at 5 s never comes back in a minute (best match %.2f; a loop is ~1)", NAMES[snd], m);
        check(m < 0.5, msg);
    }
    {
        // the fire crackles: sharp peaks well above its level
        const auto v = render(SS_FIRE, R, 30.0);
        const double crest = peak(std::vector<float>(v.begin() + (long)R * 2, v.end())) / rms(v, (size_t)R * 2, v.size());
        checkf(crest > 4.0, "fire: crackles stand out (crest factor %.1f)", crest);
        // thunder in a storm: some second within three minutes is far louder in the lows than the median
        const auto st = render(SS_STORM, R, 180.0, 31337);
        std::vector<double> lows;
        for (size_t at2 = 0; at2 + (size_t)R * 2 <= st.size(); at2 += (size_t)R * 2) {
            std::vector<float> sec(st.begin() + (long)at2, st.begin() + (long)at2 + (long)R * 2);
            lows.push_back(band(spectrum(sec), R, 20, 300));
        }
        std::vector<double> sorted = lows;
        std::sort(sorted.begin(), sorted.end());
        const double ratio = sorted.back() / sorted[sorted.size() / 2];
        checkf(ratio > 4.0, "storm: thunder within three minutes (loudest second %.1f dB over the median, below 300 Hz)",
               10 * std::log10(ratio));
    }
    {
        // different seeds, different sound; the same seed, the same sound (for these tests)
        const auto a = render(SS_RAIN, R, 2.0, 1), b = render(SS_RAIN, R, 2.0, 2), c = render(SS_RAIN, R, 2.0, 1);
        check(a != b, "two sessions (seeds) give two different rains");
        check(a == c, "one seed gives the same output (the tests are repeatable)");
    }

    // ── ramps: no clicks, ever ──────────────────────────────────────────────────────────────────
    std::printf("── ramps ──\n");
    {
        ss_state s;
        ss_init(&s, 5);
        ss_set(&s, SS_WHITE, 1.0f);
        std::vector<float> v(2 * R);
        ss_render(&s, v.data(), R, R);
        const double first10ms = rms(v, 0, (size_t)(0.01 * R) * 2);
        const double after = rms(v, (size_t)(0.5 * R) * 2, v.size());
        checkf(first10ms < after * 0.06, "a start fades in: the first 10 ms at %.1f dB under the level", db(after) - db(first10ms));
        // switch sound: the level dips to nothing between them
        ss_set(&s, SS_DARK, 1.0f);
        std::vector<float> w(2 * R);
        ss_render(&s, w.data(), R, R);
        const auto e = envelope(w, R, 0.01);
        double dip = 1e9;
        for (double x : e) dip = std::fmin(dip, x);
        checkf(dip < after * 0.05, "a new sound fades the old one out first (dip to %.1f dB under)", db(after) - db(dip));
        // off: fades out, then renders nothing, and a mix leaves the buffer bit-exact
        ss_set(&s, SS_OFF, 0.0f);
        std::vector<float> o(2 * R);
        ss_render(&s, o.data(), R, R);
        check(!ss_active(&s), "off: inactive once faded out");
        const double tail = rms(o, (size_t)(0.5 * R) * 2, o.size());
        check(tail == 0.0, "off: silent after the fade");
        std::vector<uint8_t> music(4 * 1000);
        for (size_t i = 0; i < music.size(); i++) music[i] = (uint8_t)(i * 37 + 11);
        const auto before = music;
        const unsigned long long fr = s.frames;
        ss_mix(&s, music.data(), 1000, SS_FMT_S16_LE, 2, R);
        check(music == before && s.frames == fr, "off: the music is bit-exact and nothing is computed");
    }
    {
        // a level change mid-sound is a ramp: its biggest step between frames is no bigger than the
        // sound's own at full level
        ss_state s;
        ss_init(&s, 6);
        ss_set(&s, SS_DARK, 0.1f);
        std::vector<float> v(2 * R);
        ss_render(&s, v.data(), R, R);
        ss_set(&s, SS_DARK, 1.0f);
        std::vector<float> w(2 * (R / 4));
        ss_render(&s, w.data(), R / 4, R);
        std::vector<float> steady(2 * R);
        ss_render(&s, steady.data(), R, R);
        double jump = 0, own = 0;
        for (size_t i = 2; i < w.size(); i += 2) jump = std::fmax(jump, std::fabs(w[i] - w[i - 2]));
        for (size_t i = 2; i < steady.size(); i += 2) own = std::fmax(own, std::fabs(steady[i] - steady[i - 2]));
        checkf(jump <= own * 1.1, "a level change ramps: its largest step %.4f is no bigger than the sound's own %.4f", jump, own);
    }

    // ── every format, saturating ────────────────────────────────────────────────────────────────
    std::printf("── formats ──\n");
    {
        auto run = [](int fmt, int sb, int ch, std::vector<uint8_t>& buf, size_t frames) {
            ss_state s;
            ss_init(&s, 9);
            ss_set(&s, SS_WHITE, 1.0f);
            std::vector<uint8_t> warm(frames * (size_t)ch * (size_t)sb);
            ss_mix(&s, warm.data(), frames, fmt, ch, 48000);   // through the fade-in
            for (int i = 0; i < 40; i++) ss_mix(&s, warm.data(), frames, fmt, ch, 48000);
            return ss_mix(&s, buf.data(), frames, fmt, ch, 48000);
        };
        const size_t F = 2048;
        // S16: silence picks the sound up; full scale saturates and never wraps
        std::vector<uint8_t> z(F * 4, 0);
        run(SS_FMT_S16_LE, 2, 2, z, F);
        int nz = 0;
        for (size_t i = 0; i < F * 2; i++) nz += (int16_t)(z[2 * i] | (z[2 * i + 1] << 8)) != 0;
        check(nz > (int)F, "S16: the soundscape reaches a silent buffer");
        std::vector<uint8_t> top(F * 4);
        for (size_t i = 0; i < F * 2; i++) { top[2 * i] = 0xFF; top[2 * i + 1] = 0x7F; }   // +32767
        run(SS_FMT_S16_LE, 2, 2, top, F);
        int wrapped = 0, held = 0;
        for (size_t i = 0; i < F * 2; i++) {
            const int16_t x = (int16_t)(top[2 * i] | (top[2 * i + 1] << 8));
            wrapped += x < 0;
            held += x == 32767;
        }
        check(wrapped == 0 && held > 0, "S16: full scale saturates, never wraps to negative");
        // S24_LE: the top byte stays the sign of the 24-bit value
        std::vector<uint8_t> s24(F * 8, 0);
        run(SS_FMT_S24_LE, 4, 2, s24, F);
        bool sign_ok = true;
        for (size_t i = 0; i < F * 2; i++) {
            const uint8_t* q = &s24[4 * i];
            sign_ok = sign_ok && q[3] == ((q[2] & 0x80) ? 0xFF : 0x00);
        }
        check(sign_ok, "S24 in 32: the padding byte carries the sign");
        // S24_3LE and S32: picks the sound up at the same level as S16
        std::vector<uint8_t> p3(F * 6, 0), p32(F * 8, 0);
        run(SS_FMT_S24_3LE, 3, 2, p3, F);
        run(SS_FMT_S32_LE, 4, 2, p32, F);
        double r16 = 0, r24 = 0, r32 = 0;
        for (size_t i = 0; i < F * 2; i++) {
            const double a = (int16_t)(z[2 * i] | (z[2 * i + 1] << 8)) / 32768.0;
            int32_t b = (int32_t)p3[3 * i] | ((int32_t)p3[3 * i + 1] << 8) | ((int32_t)p3[3 * i + 2] << 16);
            if (b & 0x800000) b -= 0x1000000;
            int32_t c;
            std::memcpy(&c, &p32[4 * i], 4);
            r16 += a * a;
            r24 += (b / 8388608.0) * (b / 8388608.0);
            r32 += (c / 2147483648.0) * (c / 2147483648.0);
        }
        checkf(std::fabs(db(std::sqrt(r16)) - db(std::sqrt(r24))) < 0.1 && std::fabs(db(std::sqrt(r16)) - db(std::sqrt(r32))) < 0.1,
               "S16, S24 and S32 carry it at one level (%.2f / %.2f dB)", db(std::sqrt(r24)) - db(std::sqrt(r16)),
               db(std::sqrt(r32)) - db(std::sqrt(r16)));
        // mono streams get both channels' worth
        std::vector<uint8_t> m(F * 2, 0);
        check(run(SS_FMT_S16_LE, 2, 1, m, F) == 1, "mono S16 is mixed too");
        // a format it does not know is left alone
        std::vector<uint8_t> f(F * 4, 0x55);
        const auto f0 = f;
        check(run(14 /* FLOAT_LE */, 4, 2, f, F) == 0 && f == f0, "an unknown format is left exactly as it came");
    }

    // ── every rate: the same level, and the engine never above 48 kHz ───────────────────────────
    std::printf("── rates ──\n");
    {
        const double ref = rms(render(SS_PINK, 48000, 6.0, 3), 2 * 48000, 0);
        // 8 and 22.05 kHz lose the top octaves pink noise has energy in, so their level is lower by
        // design; nothing on the player runs that slow, and they only have to work.
        for (unsigned rate : { 8000u, 22050u, 32000u, 44100u, 88200u, 96000u, 176400u, 192000u, 384000u }) {
            const auto v = render(SS_PINK, rate, 6.0, 3);
            ss_state s;
            ss_init(&s, 1);
            ss_prepare(&s, rate);
            char m[160];
            std::snprintf(m, sizeof m, "%u Hz: engine at %.0f Hz (x%d), level within tolerance of 48 kHz (%.2f dB)",
                          rate, (double)s.er, s.k, db(rms(v, 2 * rate, 0)) - db(ref));
            const double tol = rate < 32000 ? 6.0 : 1.5;
            check(s.er <= 48000.0f && std::fabs(db(rms(v, 2 * rate, 0)) - db(ref)) < tol, m);
        }
        // a rate change mid-stream re-prepares instead of playing at the wrong speed
        ss_state s;
        ss_init(&s, 4);
        ss_set(&s, SS_RAIN, 1.0f);
        std::vector<float> v(2 * 4800);
        ss_render(&s, v.data(), 4800, 44100);
        ss_render(&s, v.data(), 4800, 96000);
        check(s.rate == 96000 && s.k == 2, "a new rate re-prepares the engine");
    }

    // ── the control line ────────────────────────────────────────────────────────────────────────
    std::printf("── control line ──\n");
    {
        int snd, milli;
        check(ss_parse_control("4 250\n", 6, &snd, &milli) && snd == 4 && milli == 250, "\"4 250\" is rain at 25%");
        check(ss_parse_control("10 5000", 7, &snd, &milli) && snd == 10 && milli == 1000, "a level over 1000 is 1000");
        check(!ss_parse_control("99 500", 6, &snd, &milli) && snd == SS_OFF, "an unknown sound is off");
        check(!ss_parse_control("4", 1, &snd, &milli) && snd == SS_OFF, "a short line is off");
        check(!ss_parse_control("4 -5", 4, &snd, &milli) && snd == SS_OFF, "garbage is off");
        check(!ss_parse_control("", 0, &snd, &milli), "an empty file is off");
    }

    // ── cost ────────────────────────────────────────────────────────────────────────────────────
    std::printf("── cost (host) ──\n");
    {
        double worst = 0;
        int worst_snd = 0;
        for (int snd = 1; snd < SS_COUNT; snd++) {
            ss_state s;
            ss_init(&s, 77);
            ss_set(&s, snd, 1.0f);
            std::vector<uint8_t> buf(4 * 1024, 0);
            const auto t0 = std::chrono::steady_clock::now();
            const int secs = 20;
            for (int i = 0; i < secs * 48000 / 1024; i++) ss_mix(&s, buf.data(), 1024, SS_FMT_S16_LE, 2, 48000);
            const double ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - t0).count() / secs;
            if (verbose) std::printf("     %-6s %.2f ms per second of audio\n", NAMES[snd], ms);
            if (ms > worst) { worst = ms; worst_snd = snd; }
        }
        char m[160];
        std::snprintf(m, sizeof m, "every sound under 10 ms per second of 48 kHz audio on the host (worst: %s, %.2f ms)",
                      NAMES[worst_snd], worst);
        check(worst < 10.0, m);
        std::printf("     sizeof(ss_state) = %zu bytes\n", sizeof(ss_state));
        check(sizeof(ss_state) < 16384, "the state is under 16 KB (it lives in the shim's .bss twice)");
    }

    std::printf(fails ? "\n%d FAILED\n" : "\nall passed\n", fails);
    return fails ? 1 : 0;
}
