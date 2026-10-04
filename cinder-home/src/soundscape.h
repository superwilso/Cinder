/* soundscape.h — procedural ambient sound: noise, rain, a beach, a stream, wind, a fire, a night.
 *
 * WHAT IT IS. One generator, used in three places that each hold their own state:
 *
 *   * libcinder_mono.so (src/cinder-mono.c), inside Sony's SoundServiceFw — mixed into the jack
 *     (snd_pcm_writei) and the Bluetooth transmitter socket while library music plays;
 *   * cinder-home's own pumps (src/main.cpp) — USB-DAC -> LDAC and FM -> Bluetooth carry it over
 *     their audio, and the soundscape thread plays it ON ITS OWN when nothing else is playing.
 *
 * NOTHING HERE LOOPS. There is no recording. Every sound is built from noise and short events
 * (drops, bubbles, crackles, chirps, waves, gusts, thunder) whose timing, pitch, loudness and place
 * come from a xorshift128 generator seeded per session (period 2^128 - 1), with slow random drifts
 * on top (how hard it rains, how big the next wave is, how strong the wind). Two minutes of rain
 * are never the same two minutes.
 *
 * WHY THIS SHAPE.
 *   * It runs on SoundServiceFw's audio thread, where a stall is a dropout and a crash powers the
 *     player off (RE_mono_audio.md §9). So: no allocation, no locks, no syscalls, no libm (the
 *     shim must not gain a DT_NEEDED it did not have), fixed-size state, bounded work per frame.
 *   * Cheap enough to leave on. The engine runs at no more than 48 kHz whatever the stream's rate
 *     (96 and 192 kHz streams get it upsampled by 2 or 4), every filter is one or two poles, and
 *     events are short voices from fixed pools. Off means off: with the gain at 0 the mix call
 *     returns before touching the buffer, so the music is bit-exact and nothing is computed.
 *   * Never clips the music on its own account: the soundscape is soft-limited below full scale
 *     before it is added, and the sum saturates rather than wraps.
 *
 * Host-tested by tools/soundscape_selftest.cpp (levels, spectra, non-repetition, ramps, every
 * format, every rate, cost). `SS_WAV_DIR=<dir>` makes the self-test write 20 s of each sound as
 * WAV files, for listening on a PC.
 *
 * Valid C99 and C++14: the shim is C, the shell is C++.
 */
#ifndef CINDER_SOUNDSCAPE_H
#define CINDER_SOUNDSCAPE_H

#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* The sounds. THESE NUMBERS ARE A FILE FORMAT: the UI saves them (`ambient=` in the settings file)
 * and the shell writes them into /tmp/cinder_ambient for the shim. Append; never renumber. */
enum {
    SS_OFF = 0,
    SS_WHITE = 1,   /* flat spectrum */
    SS_PINK = 2,    /* -3 dB per octave: equal energy per octave, the "balanced" one */
    SS_DARK = 3,    /* -6 dB per octave (brown/red noise): a low rumble */
    SS_RAIN = 4,
    SS_STORM = 5,   /* heavier rain, and distant thunder now and then */
    SS_BEACH = 6,   /* waves breaking on sand */
    SS_STREAM = 7,  /* a brook: bubbles over a soft rush */
    SS_WIND = 8,
    SS_FIRE = 9,    /* a roar, a hiss, crackles and the odd pop */
    SS_NIGHT = 10,  /* crickets over still air */
    SS_COUNT = 11
};

/* Sample formats (snd_pcm_format_t values), the ones ss_mix understands. Same numbers as
 * mono_sum.h's CM_FMT_*, repeated so this header stands alone. */
enum { SS_FMT_S16_LE = 2, SS_FMT_S24_LE = 6, SS_FMT_S32_LE = 10, SS_FMT_S24_3LE = 32 };

#define SS_CTL 32          /* engine frames per control tick: modulators and filters move here */
#define SS_SINE 512        /* sine table size (linear interpolation: error < 5e-6) */
#define SS_GRAINS 48       /* noise bursts: rain patter, fire crackles */
#define SS_RINGS 16        /* damped resonators: drops, pops */
#define SS_BUBBLES 40      /* the stream's rising-pitch bubbles */
#define SS_CRICKETS 5
#define SS_FADE_S 0.35f    /* level changes and sound switches ramp over this long */

/* ── maths without libm ───────────────────────────────────────────────────────────────────────── */

/* e^x for x in about [-80, 80]: 2^(x log2 e), the integer part in the exponent bits and the
 * fraction from a degree-5 polynomial (relative error < 2e-6). Used for coefficients and envelopes,
 * never per sample per voice. */
static inline float ss_exp(float x)
{
    if (x < -80.0f) return 0.0f;
    if (x > 80.0f) x = 80.0f;
    float t = x * 1.44269504f;
    int i = (int)t;
    if ((float)i > t) i--;                 /* floor */
    const float f = t - (float)i;          /* [0, 1) */
    const float p = 1.0f + f * (0.693147182f + f * (0.240226507f + f * (0.0555041087f
                  + f * (0.00961812911f + f * 0.00133335581f))));
    const int32_t bits = (int32_t)(i + 127) << 23;
    float scale;
    memcpy(&scale, &bits, sizeof scale);   /* not a union: this header is C++ too */
    return p * scale;
}

/* sin(2 pi t), t in turns, any value. Reduced to a quarter wave, then Taylor to x^9 (error < 4e-6). */
static inline float ss_sin2pi(float t)
{
    t -= (float)(int)t;                    /* (-1, 1) */
    if (t < 0.0f) t += 1.0f;               /* [0, 1) */
    float sign = 1.0f;
    if (t >= 0.5f) { t -= 0.5f; sign = -1.0f; }
    if (t > 0.25f) t = 0.5f - t;           /* [0, 0.25]: sin is symmetric about the quarter */
    const float x = t * 6.28318531f, x2 = x * x;
    return sign * x * (1.0f - x2 / 6.0f * (1.0f - x2 / 20.0f * (1.0f - x2 / 42.0f * (1.0f - x2 / 72.0f))));
}

/* 10^(x), for the level curve's callers that want it in C. */
static inline float ss_pow10(float x) { return ss_exp(x * 2.30258509f); }

/* ── building blocks ──────────────────────────────────────────────────────────────────────────── */

typedef struct { float z; } ss_lp;                 /* one-pole low-pass */
typedef struct { float x1, y1; } ss_hp;            /* one-pole high-pass */
typedef struct { float lp, bp; } ss_svf;           /* Chamberlin state-variable band-pass */

static inline float ss_lp_run(ss_lp* f, float a, float x) { f->z += a * (x - f->z); return f->z; }
static inline float ss_hp_run(ss_hp* f, float a, float x)
{
    const float y = a * (f->y1 + x - f->x1);
    f->x1 = x;
    f->y1 = y;
    return y;
}
static inline float ss_svf_bp(ss_svf* s, float f, float q, float x)
{
    const float hp = x - s->lp - q * s->bp;
    s->bp += f * hp;
    s->lp += f * s->bp;
    return s->bp;
}

/* A slowly wandering value: every so often a new target in [lo, hi], approached smoothly. The
 * "how hard is it raining", "how strong is the wind" of every sound. Ticks are control ticks. */
typedef struct { float v, target; int left; } ss_drift;

typedef struct { float env, decay, gl, gr; int delay; } ss_grain;
/* A damped sine from an impulse: y = c*y1 - r2*y2. `amp` tracks the envelope, to retire it. */
typedef struct { float y1, y2, c, r2, gl, gr, amp, r; int delay; } ss_ring;
typedef struct { float ph, inc, chirp, env, decay, gl, gr; int delay; } ss_bubble;
typedef struct {
    float ph, inc, am_ph, am_inc, amp, gl, gr, env, drift;
    int period, pos, pulses, plen, gap, rest;
} ss_cricket;
typedef struct {
    int on;
    float t, tb, ac, tw, h, pan, bright;
    float pk[2][3];                        /* its own pink noise: two waves are two sources */
    ss_lp a[2], b[2];
    ss_hp foam[2];
} ss_wave;
typedef struct {
    int on;
    float t, attack, tau, amp, pan;
    float ca;                              /* low-pass coefficient (distance) */
    ss_drift roll;
    ss_lp a[2], b[2];
    float brown[2];
} ss_thunder;

typedef struct {
    uint32_t r[4];
    int sound, want;
    float gain, target;
    unsigned rate;                         /* output rate the coefficients are for; 0 = none yet */
    int k;                                 /* output frames per engine frame (1, 2, 4, 8) */
    float er, tps;                         /* engine rate; control ticks per second */
    float fade_step;                       /* gain change per OUTPUT frame */
    /* the upsampler and the block of engine frames rendered ahead */
    int kpos;
    float prev[2], cur[2];
    float blk[2][SS_CTL];
    int blk_n, blk_pos;
    float sine[SS_SINE + 1];
    /* noise colours */
    float pk[2][3], br[2];
    float c_pk[3], g_pk, c_br, g_br;
    ss_hp dc[2];
    float c_dc;
    /* general-purpose filters and drifts, meaning per sound */
    ss_lp lp[6][2];
    ss_hp hp[6][2];
    ss_svf svf[3][2];
    ss_drift d[6];
    float g_last[6];                       /* control values at the previous tick, to interpolate */
    float spawn[4];                        /* fractional events carried between ticks */
    ss_grain grain[SS_GRAINS];
    ss_ring ring[SS_RINGS];
    ss_bubble bub[SS_BUBBLES];
    /* one past the highest live voice in each pool: the per-sample loops stop there, so a pool
     * with three live voices costs three iterations, not forty-eight */
    int hw_grain, hw_ring, hw_bub;
    ss_cricket cri[SS_CRICKETS];
    ss_wave wave[2];
    float next_wave;                       /* seconds until the next wave starts */
    ss_thunder thunder;
    float next_thunder;
    unsigned long long frames;             /* engine frames since the sound started */
} ss_state;

/* ── randomness ───────────────────────────────────────────────────────────────────────────────── */

static inline uint32_t ss_rand(ss_state* s)
{
    uint32_t t = s->r[3];
    const uint32_t x = s->r[0];
    s->r[3] = s->r[2];
    s->r[2] = s->r[1];
    s->r[1] = x;
    t ^= t << 11;
    t ^= t >> 8;
    s->r[0] = t ^ x ^ (x >> 19);
    return s->r[0];
}
/* [0, 1) */
static inline float ss_rand01(ss_state* s) { return (float)(ss_rand(s) >> 8) * (1.0f / 16777216.0f); }
/* [lo, hi) */
static inline float ss_uniform(ss_state* s, float lo, float hi) { return lo + (hi - lo) * ss_rand01(s); }
/* Two independent noise samples in [-1, 1) from one draw: 16 bits each is a -96 dB floor, far
 * below anything a soundscape is heard against, and it halves the generator's work. */
static inline void ss_noise2(ss_state* s, float* a, float* b)
{
    const uint32_t v = ss_rand(s);
    *a = (float)(int16_t)(v & 0xFFFF) * (1.0f / 32768.0f);
    *b = (float)(int16_t)(v >> 16) * (1.0f / 32768.0f);
}

static inline float ss_sine(const ss_state* s, float ph)
{
    const float x = ph * (float)SS_SINE;
    int i = (int)x;
    const float f = x - (float)i;
    i &= SS_SINE - 1;
    return s->sine[i] + f * (s->sine[i + 1] - s->sine[i]);
}

/* ── coefficients ─────────────────────────────────────────────────────────────────────────────── */

static inline float ss_lp_coef(const ss_state* s, float fc) { return 1.0f - ss_exp(-6.28318531f * fc / s->er); }
static inline float ss_hp_coef(const ss_state* s, float fc) { return ss_exp(-6.28318531f * fc / s->er); }
static inline float ss_svf_coef(const ss_state* s, float fc)
{
    float f = 2.0f * ss_sin2pi(0.5f * fc / s->er);
    return f > 1.2f ? 1.2f : f;
}
/* Per-sample multiplier that decays by 1/e in `secs`. */
static inline float ss_decay(const ss_state* s, float secs) { return ss_exp(-1.0f / (secs * s->er)); }
/* Per-tick smoothing coefficient for a time constant of `secs`. */
static inline float ss_tick_coef(const ss_state* s, float secs) { return 1.0f - ss_exp(-1.0f / (secs * s->tps)); }

static inline float ss_drift_tick(ss_state* s, ss_drift* d, float lo, float hi, float every_s, float tau_s)
{
    if (--d->left <= 0) {
        d->target = ss_uniform(s, lo, hi);
        d->left = 1 + (int)(every_s * s->tps * ss_uniform(s, 0.5f, 1.5f));
    }
    d->v += ss_tick_coef(s, tau_s) * (d->target - d->v);
    return d->v;
}
static inline void ss_drift_set(ss_drift* d, float v) { d->v = d->target = v; d->left = 0; }

/* Pan: equal-power-ish without a square root (a parabola through the three exact points). */
static inline void ss_pan(float p, float* gl, float* gr)
{
    if (p < 0.0f) p = 0.0f;
    if (p > 1.0f) p = 1.0f;
    *gl = 1.0f - p * p;
    *gr = 1.0f - (1.0f - p) * (1.0f - p);
}

/* Events that arrive at `per_s` on average, as a count for this tick (fractions carried). */
static inline int ss_events(ss_state* s, int slot, float per_s)
{
    /* Poisson-ish: the mean is exact, and the jitter in where each one lands (`delay` below) is
     * what keeps a dense patter from sitting on the tick grid. */
    s->spawn[slot] += per_s / s->tps * ss_uniform(s, 0.0f, 2.0f);
    int n = (int)s->spawn[slot];
    s->spawn[slot] -= (float)n;
    return n;
}

/* ── voices ───────────────────────────────────────────────────────────────────────────────────── */

static inline void ss_grain_spawn(ss_state* s, float amp, float secs, float pan, int delay)
{
    for (int i = 0; i < SS_GRAINS; i++) {
        ss_grain* g = &s->grain[i];
        if (g->env > 0.0f) continue;
        g->env = amp;
        g->decay = ss_decay(s, secs);
        ss_pan(pan, &g->gl, &g->gr);
        g->delay = delay;
        if (i >= s->hw_grain) s->hw_grain = i + 1;
        return;
    }
}

static inline void ss_ring_spawn(ss_state* s, float amp, float hz, float secs, float pan, int delay)
{
    if (hz > 0.45f * s->er) return;
    for (int i = 0; i < SS_RINGS; i++) {
        ss_ring* g = &s->ring[i];
        if (g->amp > 0.0f) continue;
        const float r = ss_decay(s, secs);
        const float w = hz / s->er;
        g->c = 2.0f * r * ss_sin2pi(w + 0.25f);
        g->r2 = r * r;
        g->r = r;
        /* y[n] = A r^n sin((n+1)w) / sin(w): start from (y1, y2) = (A sin w, 0) so the first
         * output is A r sin(2w)… close enough, and the peak is A. */
        g->y1 = amp * ss_sin2pi(w);
        g->y2 = 0.0f;
        g->amp = amp;
        ss_pan(pan, &g->gl, &g->gr);
        g->delay = delay;
        if (i >= s->hw_ring) s->hw_ring = i + 1;
        return;
    }
}

static inline void ss_bubble_spawn(ss_state* s, float amp, float hz, float rise, float life, float pan, int delay)
{
    if (hz * rise > 0.45f * s->er) return;
    for (int i = 0; i < SS_BUBBLES; i++) {
        ss_bubble* b = &s->bub[i];
        if (b->env > 0.0f) continue;
        b->ph = 0.0f;                      /* a sine from zero: no click, so no attack needed */
        b->inc = hz / s->er;
        /* the pitch rises by `rise` over `life`: e^(ln(rise)/n) per sample, ln(rise) ~ rise-1
         * for the small rises used here (1.1..1.8) — close enough for a bubble */
        b->chirp = ss_exp((rise - 1.0f) / (life * s->er));
        b->env = amp;
        b->decay = ss_decay(s, life * 0.35f);
        ss_pan(pan, &b->gl, &b->gr);
        b->delay = delay;
        if (i >= s->hw_bub) s->hw_bub = i + 1;
        return;
    }
}

/* Every live voice, summed into (l, r) for one frame; the noise bursts separately into (pl, pr),
 * because each sound filters those its own way. */
static inline void ss_voices(ss_state* s, float* l, float* r, float* patter_l, float* patter_r)
{
    float pl = 0.0f, pr = 0.0f;
    int live = 0;
    for (int i = 0; i < s->hw_grain; i++) {
        ss_grain* g = &s->grain[i];
        if (g->env <= 0.0f) continue;
        live = i + 1;
        if (g->delay > 0) { g->delay--; continue; }
        float a, b;
        ss_noise2(s, &a, &b);
        const float v = (a + b) * 0.5f * g->env;
        pl += v * g->gl;
        pr += v * g->gr;
        g->env *= g->decay;
        if (g->env < 1e-4f) g->env = 0.0f;
    }
    s->hw_grain = live;
    *patter_l = pl;
    *patter_r = pr;
    float ol = 0.0f, orr = 0.0f;
    live = 0;
    for (int i = 0; i < s->hw_ring; i++) {
        ss_ring* g = &s->ring[i];
        if (g->amp <= 0.0f) continue;
        live = i + 1;
        if (g->delay > 0) { g->delay--; continue; }
        const float y = g->c * g->y1 - g->r2 * g->y2;
        g->y2 = g->y1;
        g->y1 = y;
        ol += y * g->gl;
        orr += y * g->gr;
        g->amp *= g->r;
        if (g->amp < 1e-4f) { g->amp = 0.0f; g->y1 = g->y2 = 0.0f; }
    }
    s->hw_ring = live;
    live = 0;
    for (int i = 0; i < s->hw_bub; i++) {
        ss_bubble* b = &s->bub[i];
        if (b->env <= 0.0f) continue;
        live = i + 1;
        if (b->delay > 0) { b->delay--; continue; }
        const float v = ss_sine(s, b->ph) * b->env;
        ol += v * b->gl;
        orr += v * b->gr;
        b->ph += b->inc;
        if (b->ph >= 1.0f) b->ph -= 1.0f;
        b->inc *= b->chirp;
        if (b->inc > 0.45f) b->env = 0.0f;
        b->env *= b->decay;
        if (b->env < 1e-4f) b->env = 0.0f;
    }
    s->hw_bub = live;
    *l += ol;
    *r += orr;
}

/* A recursive filter fed by voices that fall silent decays toward zero and, on the way, through
 * denormal floats, which cost a hundred cycles each on x86 and are slow or flushed on ARM. Feeding
 * it a whisper of noise (-180 dB) keeps every state a normal number for the price of a multiply. */
#define SS_FLOOR 1e-9f

/* ── noise colours, one frame each ────────────────────────────────────────────────────────────── */

static inline void ss_white2(ss_state* s, float* l, float* r) { ss_noise2(s, l, r); }

/* Paul Kellet's three-pole pink filter, poles moved to the engine rate. `b` is one channel's state. */
static inline float ss_pink_on(const ss_state* s, float* b, float w)
{
    b[0] = s->c_pk[0] * b[0] + w * 0.0990460f;
    b[1] = s->c_pk[1] * b[1] + w * 0.2965164f;
    b[2] = s->c_pk[2] * b[2] + w * 1.0526913f;
    return (b[0] + b[1] + b[2] + w * 0.1848f) * s->g_pk;
}
static inline float ss_pink1(ss_state* s, int ch, float w) { return ss_pink_on(s, s->pk[ch], w); }

/* A leaky integrator: -6 dB per octave above ~20 Hz, flat below, then a DC block. */
static inline float ss_brown1(ss_state* s, int ch, float w)
{
    s->br[ch] = s->c_br * s->br[ch] + w;
    return ss_hp_run(&s->dc[ch], s->c_dc, s->br[ch] * s->g_br);
}

/* ── the sounds: each renders one control block of engine frames into blk ─────────────────────── */

/* Interpolate a control value across the block: from what it was at the last tick to `now`. */
static inline float ss_ctl_from(ss_state* s, int slot, float now, float* step, int n)
{
    const float from = s->g_last[slot];
    *step = (now - from) / (float)n;
    s->g_last[slot] = now;
    return from;
}

static void ss_block_noise(ss_state* s, int n)
{
    for (int i = 0; i < n; i++) {
        float a, b;
        ss_white2(s, &a, &b);
        float l, r;
        if (s->sound == SS_WHITE) { l = a * 0.30f; r = b * 0.30f; }
        else if (s->sound == SS_PINK) { l = ss_pink1(s, 0, a); r = ss_pink1(s, 1, b); }
        else { l = ss_brown1(s, 0, a); r = ss_brown1(s, 1, b); }
        s->blk[0][i] = l;
        s->blk[1][i] = r;
    }
}

/* Rain, and Storm (heavier, brighter, with thunder). */
static void ss_block_rain(ss_state* s, int n)
{
    const int storm = s->sound == SS_STORM;
    /* d[0]: intensity. Rain wanders between a shower and a downpour; a storm stays heavy. */
    const float inten = storm ? ss_drift_tick(s, &s->d[0], 0.85f, 1.25f, 9.0f, 4.0f)
                              : ss_drift_tick(s, &s->d[0], 0.45f, 1.0f, 12.0f, 6.0f);
    /* d[1]: a faster flutter on the bed, as gusts move the curtain of rain. */
    const float flut = ss_drift_tick(s, &s->d[1], 0.75f, 1.0f, 0.8f, 0.5f);

    /* Patter: a dense field of tiny impacts. Drops: fewer, bigger, with a pitch. */
    int np = ss_events(s, 0, (storm ? 520.0f : 300.0f) * inten);
    for (int k = 0; k < np; k++) {
        const float u = ss_rand01(s);
        ss_grain_spawn(s, 0.03f + 0.32f * u * u * u * u, ss_uniform(s, 0.0004f, 0.0022f),
                       ss_rand01(s), (int)(ss_rand01(s) * n));
    }
    int nd = ss_events(s, 1, (storm ? 16.0f : 11.0f) * inten);
    for (int k = 0; k < nd; k++) {
        const float u = ss_rand01(s);
        const float hz = 1700.0f * ss_exp(ss_rand01(s) * 1.25f);     /* 1.7 .. 6 kHz, log */
        ss_ring_spawn(s, 0.02f + 0.16f * u * u * u, hz, ss_uniform(s, 0.003f, 0.011f),
                      ss_rand01(s), (int)(ss_rand01(s) * n));
    }

    float st_bed;
    float bed = ss_ctl_from(s, 0, inten * flut * (storm ? 0.17f : 0.12f), &st_bed, n);
    float st_body;
    float body = ss_ctl_from(s, 1, inten * (storm ? 0.30f : 0.14f), &st_body, n);
    const float a_hp = ss_hp_coef(s, storm ? 300.0f : 450.0f), a_lp = ss_lp_coef(s, storm ? 9000.0f : 7000.0f);
    const float a_body = ss_lp_coef(s, 700.0f), a_pat = ss_hp_coef(s, 1100.0f);

    /* Thunder (storm only): a distant roll every 20-70 s. */
    ss_thunder* th = &s->thunder;
    if (storm) {
        s->next_thunder -= (float)n / s->er;
        if (!th->on && s->next_thunder <= 0.0f) {
            const float dist = ss_rand01(s);
            th->on = 1;
            th->t = 0.0f;
            th->attack = 0.12f + 0.45f * dist;
            th->tau = 1.6f + 2.8f * ss_rand01(s);
            th->amp = 0.95f - 0.5f * dist;
            th->ca = ss_lp_coef(s, 650.0f - 420.0f * dist);
            th->pan = ss_uniform(s, 0.25f, 0.75f);
            ss_drift_set(&th->roll, 0.6f);
            s->next_thunder = ss_uniform(s, 20.0f, 70.0f);
        }
    }
    float th_env = 0.0f, th_step = 0.0f, tl = 0.0f, tr = 0.0f;
    if (th->on) {
        const float t0 = th->t;
        th->t += (float)n / s->er;
        const float roll = ss_drift_tick(s, &th->roll, 0.25f, 1.0f, 0.12f, 0.05f);
        float e0 = t0 < th->attack ? t0 / th->attack : ss_exp(-(t0 - th->attack) / th->tau);
        float e1 = th->t < th->attack ? th->t / th->attack : ss_exp(-(th->t - th->attack) / th->tau);
        e0 *= th->amp * roll;
        e1 *= th->amp * roll;
        th_env = e0;
        th_step = (e1 - e0) / (float)n;
        if (th->t > th->attack + 6.0f * th->tau) th->on = 0;
        ss_pan(th->pan, &tl, &tr);
    }

    for (int i = 0; i < n; i++) {
        float a, b;
        ss_white2(s, &a, &b);
        float l = ss_lp_run(&s->lp[0][0], a_lp, ss_hp_run(&s->hp[0][0], a_hp, a)) * bed;
        float r = ss_lp_run(&s->lp[0][1], a_lp, ss_hp_run(&s->hp[0][1], a_hp, b)) * bed;
        float c, d;
        ss_white2(s, &c, &d);
        l += ss_lp_run(&s->lp[1][0], a_body, ss_brown1(s, 0, c)) * body;
        r += ss_lp_run(&s->lp[1][1], a_body, ss_brown1(s, 1, d)) * body;
        float pl, pr;
        ss_voices(s, &l, &r, &pl, &pr);
        l += ss_hp_run(&s->hp[1][0], a_pat, pl + a * SS_FLOOR);
        r += ss_hp_run(&s->hp[1][1], a_pat, pr + b * SS_FLOOR);
        if (th->on) {
            float e, f;
            ss_white2(s, &e, &f);
            th->brown[0] = 0.9985f * th->brown[0] + e * 0.05f;
            th->brown[1] = 0.9985f * th->brown[1] + f * 0.05f;
            const float x = ss_lp_run(&th->b[0], th->ca, ss_lp_run(&th->a[0], th->ca, th->brown[0]));
            const float y = ss_lp_run(&th->b[1], th->ca, ss_lp_run(&th->a[1], th->ca, th->brown[1]));
            l += x * th_env * tl * 2.2f;
            r += y * th_env * tr * 2.2f;
            th_env += th_step;
        }
        s->blk[0][i] = l;
        s->blk[1][i] = r;
        bed += st_bed;
        body += st_body;
    }
}

static void ss_wave_start(ss_state* s, ss_wave* w)
{
    w->on = 1;
    w->t = 0.0f;
    w->tb = ss_uniform(s, 2.6f, 5.0f);         /* the swell, until it breaks */
    w->ac = ss_uniform(s, 0.25f, 0.6f);        /* the crash */
    w->tw = ss_uniform(s, 1.1f, 2.4f);         /* the wash up the sand */
    w->h = ss_uniform(s, 0.5f, 1.0f);
    w->pan = ss_uniform(s, 0.3f, 0.7f);
    w->bright = ss_uniform(s, 0.6f, 1.0f);
}

/* A beach: waves that swell, break and wash up the sand, over the low roar of the rest of the sea.
 * Every wave has its own size, pace, brightness and place; the next one starts while the last is
 * still washing in, at a random interval. */
static void ss_block_beach(ss_state* s, int n)
{
    const float dt = (float)n / s->er;
    s->next_wave -= dt;
    if (s->next_wave <= 0.0f) {
        ss_wave* w = !s->wave[0].on ? &s->wave[0] : !s->wave[1].on ? &s->wave[1]
                   : (s->wave[0].t > s->wave[1].t ? &s->wave[0] : &s->wave[1]);
        ss_wave_start(s, w);
        s->next_wave = w->tb + w->ac + ss_uniform(s, 1.0f, 4.5f);
    }
    float env0[2] = { 0, 0 }, env1[2] = { 0, 0 }, foam0[2] = { 0, 0 }, foam1[2] = { 0, 0 }, cut[2] = { 0, 0 };
    for (int j = 0; j < 2; j++) {
        ss_wave* w = &s->wave[j];
        if (!w->on) continue;
        for (int e = 0; e < 2; e++) {
            const float t = w->t + (e ? dt : 0.0f);
            float env, c, foam = 0.0f;
            if (t < w->tb) {
                const float x = t / w->tb;
                env = 0.4f * x * x;
                c = 250.0f + 900.0f * x * x;
            } else if (t < w->tb + w->ac) {
                const float x = (t - w->tb) / w->ac;
                env = 0.4f + 0.6f * x;
                c = 1150.0f + 2200.0f * w->bright * x;
                foam = x;
            } else {
                const float x = t - w->tb - w->ac;
                env = ss_exp(-x / w->tw) + 0.18f * ss_exp(-x / 3.5f);
                c = 600.0f + (550.0f + 2200.0f * w->bright) * ss_exp(-x / (0.8f * w->tw));
                foam = ss_exp(-x / (0.7f * w->tw)) + 0.3f * ss_exp(-x / 3.0f);
            }
            env *= w->h;
            foam *= w->h;
            if (e) { env1[j] = env; foam1[j] = foam; } else { env0[j] = env; foam0[j] = foam; cut[j] = c; }
        }
        w->t += dt;
        if (w->t > w->tb + w->ac + 7.0f * w->tw + 6.0f) w->on = 0;
    }
    const float a_roar = ss_lp_coef(s, 380.0f), a_foam = ss_hp_coef(s, 1800.0f);
    float ac[2], ge[2], gs[2], fe[2], fs[2], pl[2], pr[2];
    for (int j = 0; j < 2; j++) {
        ac[j] = ss_lp_coef(s, cut[j]);
        ge[j] = env0[j];
        gs[j] = (env1[j] - env0[j]) / (float)n;
        fe[j] = foam0[j];
        fs[j] = (foam1[j] - foam0[j]) / (float)n;
        ss_pan(s->wave[j].pan, &pl[j], &pr[j]);
    }
    /* the sea beyond the waves: never silent between them */
    const float sea = 0.85f * ss_drift_tick(s, &s->d[0], 0.6f, 1.0f, 6.0f, 3.0f);
    for (int i = 0; i < n; i++) {
        float a, b;
        ss_white2(s, &a, &b);
        float l = ss_lp_run(&s->lp[0][0], a_roar, ss_brown1(s, 0, a)) * sea * 0.25f;
        float r = ss_lp_run(&s->lp[0][1], a_roar, ss_brown1(s, 1, b)) * sea * 0.25f;
        for (int j = 0; j < 2; j++) {
            ss_wave* w = &s->wave[j];
            if (ge[j] <= 0.0f && fe[j] <= 0.0f && gs[j] <= 0.0f) continue;
            float c, d;
            ss_white2(s, &c, &d);
            const float sl = ss_lp_run(&w->b[0], ac[j], ss_lp_run(&w->a[0], ac[j], ss_pink_on(s, w->pk[0], c)));
            const float sr = ss_lp_run(&w->b[1], ac[j], ss_lp_run(&w->a[1], ac[j], ss_pink_on(s, w->pk[1], d)));
            const float fl = ss_hp_run(&w->foam[0], a_foam, c) * fe[j] * 0.16f;
            const float fr = ss_hp_run(&w->foam[1], a_foam, d) * fe[j] * 0.16f;
            l += (sl * ge[j] * 0.55f + fl) * (0.6f + 0.4f * pl[j]);
            r += (sr * ge[j] * 0.55f + fr) * (0.6f + 0.4f * pr[j]);
            ge[j] += gs[j];
            fe[j] += fs[j];
        }
        s->blk[0][i] = l;
        s->blk[1][i] = r;
    }
}

/* A brook: a field of tiny bubbles, each a sine whose pitch rises as it shrinks (Minnaert's
 * resonance), over the band-limited rush of the water. */
static void ss_block_stream(ss_state* s, int n)
{
    const float flow = ss_drift_tick(s, &s->d[0], 0.6f, 1.0f, 5.0f, 2.5f);
    const int nb = ss_events(s, 0, 95.0f * flow);
    for (int k = 0; k < nb; k++) {
        const float u = ss_rand01(s);
        const float hz = 520.0f * ss_exp(ss_rand01(s) * 1.55f);       /* 520 Hz .. 2.5 kHz, log */
        /* small bubbles (high) are short and quiet; big ones (low) ring a little longer */
        const float life = 0.006f + 0.030f * (2500.0f - hz) / 2000.0f * ss_uniform(s, 0.5f, 1.0f);
        ss_bubble_spawn(s, 0.03f + 0.17f * u * u * u, hz, ss_uniform(s, 1.15f, 1.7f), life,
                        ss_uniform(s, 0.1f, 0.9f), (int)(ss_rand01(s) * n));
    }
    float st;
    float rush = ss_ctl_from(s, 0, 0.085f * (0.5f + 0.5f * flow), &st, n);
    const float a_hp = ss_hp_coef(s, 480.0f), a_lp = ss_lp_coef(s, 3800.0f), a_low = ss_lp_coef(s, 300.0f);
    for (int i = 0; i < n; i++) {
        float a, b;
        ss_white2(s, &a, &b);
        float l = ss_lp_run(&s->lp[0][0], a_lp, ss_hp_run(&s->hp[0][0], a_hp, a)) * rush;
        float r = ss_lp_run(&s->lp[0][1], a_lp, ss_hp_run(&s->hp[0][1], a_hp, b)) * rush;
        l += ss_lp_run(&s->lp[1][0], a_low, ss_brown1(s, 0, b)) * rush * 0.9f;
        r += ss_lp_run(&s->lp[1][1], a_low, ss_brown1(s, 1, a)) * rush * 0.9f;
        float pl, pr;
        ss_voices(s, &l, &r, &pl, &pr);
        s->blk[0][i] = l;
        s->blk[1][i] = r;
        rush += st;
    }
}

/* Wind: noise through a band-pass whose centre and level follow the gusts — higher and louder as
 * the wind picks up — with a faint whistle on the strongest gusts. Each ear's band wanders on its
 * own, which is what makes it sound like air moving past rather than a filter sweeping. */
static void ss_block_wind(ss_state* s, int n)
{
    /* A new gust target is chosen on the tick `left` runs out; a quarter of them are a big one. */
    const int fresh = s->d[0].left <= 1;
    const float gust = ss_drift_tick(s, &s->d[0], 0.0f, 1.0f, 5.0f, 2.2f);
    if (fresh && ss_rand01(s) < 0.25f) s->d[0].target = 1.0f;
    const float fl = ss_drift_tick(s, &s->d[1], 170.0f, 520.0f, 4.0f, 2.5f) * (0.8f + 0.7f * gust);
    const float fr = ss_drift_tick(s, &s->d[2], 170.0f, 520.0f, 4.0f, 2.5f) * (0.8f + 0.7f * gust);
    const float wl = ss_drift_tick(s, &s->d[3], 900.0f, 1700.0f, 3.0f, 1.5f);
    float st, sw;
    float g = ss_ctl_from(s, 0, 0.30f + 0.55f * gust, &st, n);
    float whistle = ss_ctl_from(s, 1, 0.010f * gust * gust * gust, &sw, n);
    const float cl = ss_svf_coef(s, fl), cr = ss_svf_coef(s, fr), cw = ss_svf_coef(s, wl);
    const float a_low = ss_lp_coef(s, 110.0f), a_air = ss_lp_coef(s, 1300.0f);
    for (int i = 0; i < n; i++) {
        float a, b;
        ss_white2(s, &a, &b);
        float l = ss_svf_bp(&s->svf[0][0], cl, 0.9f, a) * g;
        float r = ss_svf_bp(&s->svf[0][1], cr, 0.9f, b) * g;
        l += ss_svf_bp(&s->svf[1][0], cw, 0.12f, b) * whistle;
        r += ss_svf_bp(&s->svf[1][1], cw * 1.03f, 0.12f, a) * whistle;
        l += ss_lp_run(&s->lp[0][0], a_low, ss_brown1(s, 0, a)) * g * 0.5f;
        r += ss_lp_run(&s->lp[0][1], a_low, ss_brown1(s, 1, b)) * g * 0.5f;
        /* air, not hiss: what is left above ~1 kHz is the whistle's job */
        s->blk[0][i] = ss_lp_run(&s->lp[1][0], a_air, l);
        s->blk[1][i] = ss_lp_run(&s->lp[1][1], a_air, r);
        g += st;
        whistle += sw;
    }
}

/* A fire: a low roar that flickers, a soft hiss, crackles that come in bursts, and the odd pop. */
static void ss_block_fire(ss_state* s, int n)
{
    const float flick = ss_drift_tick(s, &s->d[0], 0.45f, 1.0f, 0.5f, 0.25f);
    const float heat = ss_drift_tick(s, &s->d[1], 0.5f, 1.0f, 8.0f, 4.0f);
    /* crackle bursts: each one a handful of snaps a few ms apart */
    const int nc = ss_events(s, 0, 7.0f * heat);
    for (int k = 0; k < nc; k++) {
        const int snaps = 1 + (int)(ss_rand01(s) * 5.0f);
        const float pan = ss_rand01(s);
        int at = (int)(ss_rand01(s) * n);
        for (int j = 0; j < snaps; j++) {
            const float u = ss_rand01(s);
            ss_grain_spawn(s, 0.08f + 0.75f * u * u * u, ss_uniform(s, 0.0001f, 0.0007f),
                           pan + ss_uniform(s, -0.08f, 0.08f), at);
            at += (int)(ss_uniform(s, 0.001f, 0.014f) * s->er);
        }
    }
    const int npop = ss_events(s, 1, 0.5f * heat);
    for (int k = 0; k < npop; k++)
        ss_ring_spawn(s, ss_uniform(s, 0.12f, 0.35f), ss_uniform(s, 700.0f, 2400.0f),
                      ss_uniform(s, 0.008f, 0.022f), ss_rand01(s), (int)(ss_rand01(s) * n));
    float st, sh;
    float roar = ss_ctl_from(s, 0, 0.55f * flick * heat, &st, n);
    float hiss = ss_ctl_from(s, 1, 0.022f * (0.4f + 0.6f * flick), &sh, n);
    const float a_roar = ss_lp_coef(s, 260.0f), a_hiss = ss_hp_coef(s, 3000.0f), a_snap = ss_hp_coef(s, 1400.0f);
    for (int i = 0; i < n; i++) {
        float a, b;
        ss_white2(s, &a, &b);
        float l = ss_lp_run(&s->lp[0][0], a_roar, ss_brown1(s, 0, a)) * roar;
        float r = ss_lp_run(&s->lp[0][1], a_roar, ss_brown1(s, 1, b)) * roar;
        l += ss_hp_run(&s->hp[0][0], a_hiss, b) * hiss;
        r += ss_hp_run(&s->hp[0][1], a_hiss, a) * hiss;
        float pl, pr;
        ss_voices(s, &l, &r, &pl, &pr);
        l += ss_hp_run(&s->hp[1][0], a_snap, pl + a * SS_FLOOR);
        r += ss_hp_run(&s->hp[1][1], a_snap, pr + b * SS_FLOOR);
        s->blk[0][i] = l;
        s->blk[1][i] = r;
        roar += st;
        hiss += sh;
    }
}

static void ss_cricket_start(ss_state* s, ss_cricket* c)
{
    const float hz = ss_uniform(s, 3900.0f, 5100.0f);
    c->inc = hz / s->er;
    c->am_inc = ss_uniform(s, 140.0f, 260.0f) / s->er;
    c->amp = ss_uniform(s, 0.025f, 0.09f);
    ss_pan(ss_rand01(s), &c->gl, &c->gr);
    c->period = (int)(ss_uniform(s, 0.45f, 1.1f) * s->er);
    c->pulses = 2 + (int)(ss_rand01(s) * 3.0f);
    c->plen = (int)(ss_uniform(s, 0.014f, 0.022f) * s->er);
    c->gap = (int)(ss_uniform(s, 0.010f, 0.018f) * s->er);
    c->pos = (int)(ss_rand01(s) * (float)c->period);   /* out of step with the others */
    c->rest = 0;
    c->drift = 1.0f;
}

/* A night: a few crickets, each with its own pitch, pace and distance, chirping out of step and
 * now and then falling silent for a while — over the faintest movement of air. */
static void ss_block_night(ss_state* s, int n)
{
    const float air = ss_drift_tick(s, &s->d[0], 0.5f, 1.0f, 7.0f, 3.5f);
    for (int j = 0; j < SS_CRICKETS; j++) {
        ss_cricket* c = &s->cri[j];
        if (c->period <= 0) ss_cricket_start(s, c);
        if (c->rest > 0) {
            c->rest -= n;
            if (c->rest <= 0) { c->rest = 0; c->pos = 0; }
        } else if (ss_rand01(s) < 0.04f / s->tps) {
            c->rest = (int)(ss_uniform(s, 3.0f, 15.0f) * s->er);      /* a pause */
        }
        if (ss_rand01(s) < 0.02f / s->tps) ss_cricket_start(s, c);   /* a different cricket */
    }
    float st;
    float g = ss_ctl_from(s, 0, 0.11f * air, &st, n);
    const float a_low = ss_lp_coef(s, 220.0f), a_hp = ss_hp_coef(s, 500.0f), a_lp = ss_lp_coef(s, 2500.0f);
    const float atk = 1.0f / (0.003f * s->er), rel = 1.0f / (0.004f * s->er);
    for (int i = 0; i < n; i++) {
        float a, b;
        ss_white2(s, &a, &b);
        float l = ss_lp_run(&s->lp[0][0], a_low, ss_brown1(s, 0, a)) * g;
        float r = ss_lp_run(&s->lp[0][1], a_low, ss_brown1(s, 1, b)) * g;
        l += ss_lp_run(&s->lp[1][0], a_lp, ss_hp_run(&s->hp[0][0], a_hp, b)) * g * 0.08f;
        r += ss_lp_run(&s->lp[1][1], a_lp, ss_hp_run(&s->hp[0][1], a_hp, a)) * g * 0.08f;
        for (int j = 0; j < SS_CRICKETS; j++) {
            ss_cricket* c = &s->cri[j];
            int on = 0;
            if (c->rest == 0) {
                const int slot = c->plen + c->gap;
                const int k = c->pos / slot;
                on = k < c->pulses && c->pos % slot < c->plen;
                if (++c->pos >= c->period) c->pos = 0;
            }
            c->env += on ? atk : -rel;
            if (c->env > 1.0f) c->env = 1.0f;
            if (c->env <= 0.0f) { c->env = 0.0f; continue; }
            const float am = 0.55f + 0.45f * ss_sine(s, c->am_ph);
            const float v = ss_sine(s, c->ph) * c->env * am * c->amp;
            l += v * c->gl;
            r += v * c->gr;
            c->ph += c->inc;
            if (c->ph >= 1.0f) c->ph -= 1.0f;
            c->am_ph += c->am_inc;
            if (c->am_ph >= 1.0f) c->am_ph -= 1.0f;
        }
        s->blk[0][i] = l;
        s->blk[1][i] = r;
        g += st;
    }
}

static void ss_engine_block(ss_state* s)
{
    const int n = SS_CTL;
    switch (s->sound) {
    case SS_WHITE: case SS_PINK: case SS_DARK: ss_block_noise(s, n); break;
    case SS_RAIN: case SS_STORM: ss_block_rain(s, n); break;
    case SS_BEACH: ss_block_beach(s, n); break;
    case SS_STREAM: ss_block_stream(s, n); break;
    case SS_WIND: ss_block_wind(s, n); break;
    case SS_FIRE: ss_block_fire(s, n); break;
    case SS_NIGHT: ss_block_night(s, n); break;
    default: memset(s->blk, 0, sizeof s->blk); break;
    }
    s->blk_n = n;
    s->blk_pos = 0;
    s->frames += (unsigned long long)n;
}

/* ── set-up ───────────────────────────────────────────────────────────────────────────────────── */

/* Forget every voice and filter: a new sound starts from silence. */
static inline void ss_reset_voices(ss_state* s)
{
    memset(s->pk, 0, sizeof s->pk);
    memset(s->br, 0, sizeof s->br);
    memset(s->dc, 0, sizeof s->dc);
    memset(s->lp, 0, sizeof s->lp);
    memset(s->hp, 0, sizeof s->hp);
    memset(s->svf, 0, sizeof s->svf);
    memset(s->d, 0, sizeof s->d);
    memset(s->g_last, 0, sizeof s->g_last);
    memset(s->spawn, 0, sizeof s->spawn);
    memset(s->grain, 0, sizeof s->grain);
    memset(s->ring, 0, sizeof s->ring);
    memset(s->bub, 0, sizeof s->bub);
    s->hw_grain = s->hw_ring = s->hw_bub = 0;
    memset(s->cri, 0, sizeof s->cri);
    memset(s->wave, 0, sizeof s->wave);
    memset(&s->thunder, 0, sizeof s->thunder);
    s->next_wave = 0.5f;
    s->next_thunder = 8.0f + 20.0f * ss_rand01(s);
    /* drifts start mid-range rather than at zero, so a sound does not fade up from nothing over its
     * first drift time on top of the level ramp */
    for (int i = 0; i < 6; i++) s->d[i].v = s->d[i].target = 0.0f;
    s->blk_n = s->blk_pos = 0;
    s->kpos = 0;
    s->prev[0] = s->prev[1] = s->cur[0] = s->cur[1] = 0.0f;
    s->frames = 0;
}

static inline void ss_seed_drifts(ss_state* s)
{
    switch (s->sound) {
    case SS_RAIN: ss_drift_set(&s->d[0], 0.75f); ss_drift_set(&s->d[1], 0.9f); break;
    case SS_STORM: ss_drift_set(&s->d[0], 1.05f); ss_drift_set(&s->d[1], 0.9f); break;
    case SS_BEACH: ss_drift_set(&s->d[0], 0.8f); break;
    case SS_STREAM: ss_drift_set(&s->d[0], 0.8f); break;
    case SS_WIND: ss_drift_set(&s->d[0], 0.4f); ss_drift_set(&s->d[1], 320.0f); ss_drift_set(&s->d[2], 320.0f);
                  ss_drift_set(&s->d[3], 1200.0f); break;
    case SS_FIRE: ss_drift_set(&s->d[0], 0.75f); ss_drift_set(&s->d[1], 0.75f); break;
    case SS_NIGHT: ss_drift_set(&s->d[0], 0.75f); break;
    default: break;
    }
}

/* The coefficients that depend on the rate. Called when the rate changes. */
static inline void ss_prepare(ss_state* s, unsigned rate)
{
    if (rate < 8000) rate = 8000;
    s->rate = rate;
    s->k = 1;
    while (rate / (unsigned)s->k > 48000u && s->k < 8) s->k *= 2;
    s->er = (float)rate / (float)s->k;
    s->tps = s->er / (float)SS_CTL;
    s->fade_step = 1.0f / (SS_FADE_S * (float)rate);
    /* Kellet's poles at 44.1 kHz are 16.5 Hz, 265 Hz and 3.95 kHz; keep the frequencies. */
    s->c_pk[0] = ss_exp(-6.28318531f * 16.5f / s->er);
    s->c_pk[1] = ss_exp(-6.28318531f * 264.6f / s->er);
    s->c_pk[2] = ss_exp(-6.28318531f * 3945.0f / s->er);
    s->g_pk = 0.105f;
    /* Brown: b = p b + w has variance var(w) / (1 - p^2); scale it back to the same level as the
     * others whatever the rate. var(uniform[-1,1)) = 1/3. */
    s->c_br = ss_exp(-6.28318531f * 18.0f / s->er);
    {
        const float v = 1.0f - s->c_br * s->c_br;       /* small: ~0.005 at 44.1 kHz */
        /* sqrt(v) by two Newton steps from a good start — v is in a known narrow range */
        float q = 0.07f;
        for (int i = 0; i < 4; i++) q = 0.5f * (q + v / q);
        s->g_br = q * 0.30f * 1.7320508f;
    }
    s->c_dc = ss_hp_coef(s, 14.0f);
    ss_reset_voices(s);
    ss_seed_drifts(s);
}

static inline void ss_init(ss_state* s, uint32_t seed)
{
    memset(s, 0, sizeof *s);
    /* splitmix32 over the seed so near seeds diverge at once; xorshift must not start at zero */
    uint32_t z = seed ? seed : 0x9E3779B9u;
    for (int i = 0; i < 4; i++) {
        z += 0x9E3779B9u;
        uint32_t x = z;
        x = (x ^ (x >> 16)) * 0x85EBCA6Bu;
        x = (x ^ (x >> 13)) * 0xC2B2AE35u;
        x ^= x >> 16;
        s->r[i] = x ? x : 0x6C078965u;
    }
    for (int i = 0; i <= SS_SINE; i++) s->sine[i] = ss_sin2pi((float)i / (float)SS_SINE);
}

/* Ask for a sound and a level (0..1, already through the level curve). Takes effect gradually: the
 * level ramps, and a different sound fades the old one out before the new one fades in. */
static inline void ss_set(ss_state* s, int sound, float gain)
{
    if (sound < 0 || sound >= SS_COUNT) sound = SS_OFF;
    if (gain < 0.0f || sound == SS_OFF) gain = 0.0f;
    if (gain > 1.0f) gain = 1.0f;
    s->want = sound;
    s->target = gain;
}

/* Is there anything to add? False only when it is off AND has finished fading: then a caller may
 * skip the call, and the buffer it would have mixed into stays bit-exact. */
static inline int ss_active(const ss_state* s)
{
    return s->gain > 0.0f || (s->target > 0.0f && s->want != SS_OFF);
}

/* Each sound's output gain, matched by K-weighted loudness (ITU-R BS.1770, what loudness meters
 * use) so that switching from one to another at the same setting does not jump: all at about -21 dB
 * relative to full scale, the night 4 dB quieter because it IS the quiet one. Measured and asserted
 * by tools/soundscape_selftest.cpp. */
static const float ss_makeup[SS_COUNT] = {
    0.0f, 0.37f, 0.49f, 0.69f, 1.74f, 0.93f, 2.07f, 2.43f, 1.33f, 2.37f, 3.9f
};

/* A soft limit well below full scale: tanh's Pade form, flat for the levels sounds sit at. */
static inline float ss_soft(float x)
{
    if (x > 3.0f) x = 3.0f;
    if (x < -3.0f) x = -3.0f;
    const float x2 = x * x;
    return x * (27.0f + x2) / (27.0f + 9.0f * x2) * 0.92f;
}

/* Render `frames` stereo frames at `rate` into out[2*frames] (interleaved, -1..1), level ramps
 * applied. Returns 0 and leaves `out` untouched when there is nothing to render. */
static inline int ss_render(ss_state* s, float* out, size_t frames, unsigned rate)
{
    if (!ss_active(s)) return 0;
    if (rate != s->rate) ss_prepare(s, rate);
    for (size_t i = 0; i < frames; i++) {
        /* the level, and the switch between sounds once the old one has faded out */
        const float goal = s->want == s->sound ? s->target : 0.0f;
        if (s->gain < goal) { s->gain += s->fade_step; if (s->gain > goal) s->gain = goal; }
        else if (s->gain > goal) { s->gain -= s->fade_step; if (s->gain < goal) s->gain = goal; }
        if (s->gain <= 0.0f && s->want != s->sound) {
            s->sound = s->want;
            ss_reset_voices(s);
            ss_seed_drifts(s);
        }
        if (s->gain <= 0.0f && s->target <= 0.0f) {
            memset(out + 2 * i, 0, (frames - i) * 2 * sizeof(float));
            return 1;
        }
        /* the next output frame: an engine frame, or a step between two of them */
        if (s->kpos == 0) {
            if (s->blk_pos >= s->blk_n) ss_engine_block(s);
            s->prev[0] = s->cur[0];
            s->prev[1] = s->cur[1];
            s->cur[0] = s->blk[0][s->blk_pos];
            s->cur[1] = s->blk[1][s->blk_pos];
            s->blk_pos++;
        }
        s->kpos++;
        const float f = (float)s->kpos / (float)s->k;
        if (s->kpos >= s->k) s->kpos = 0;
        const float mk = ss_makeup[s->sound];
        const float l = (s->prev[0] + (s->cur[0] - s->prev[0]) * f) * mk;
        const float r = (s->prev[1] + (s->cur[1] - s->prev[1]) * f) * mk;
        out[2 * i] = ss_soft(l) * s->gain;
        out[2 * i + 1] = ss_soft(r) * s->gain;
    }
    return 1;
}

static inline int32_t ss_clamp(int64_t v, int32_t lo, int32_t hi)
{
    return v < lo ? lo : v > hi ? hi : (int32_t)v;
}

/* Mix into interleaved PCM in place: `channels` 1 or 2, `fmt` one of SS_FMT_*. Anything else is
 * left alone (returns 0), as is everything while the soundscape is off. The caller checks for DSD
 * over PCM first (mono_sum.h's cm_looks_like_dop): adding to DoP markers is noise at full scale. */
static inline int ss_mix(ss_state* s, uint8_t* p, size_t frames, int fmt, int channels, unsigned rate)
{
    if (!ss_active(s) || !p || frames == 0 || (channels != 1 && channels != 2)) return 0;
    int sb;
    switch (fmt) {
    case SS_FMT_S16_LE: sb = 2; break;
    case SS_FMT_S24_3LE: sb = 3; break;
    case SS_FMT_S24_LE: case SS_FMT_S32_LE: sb = 4; break;
    default: return 0;
    }
    float tmp[2 * 128];
    size_t done = 0;
    while (done < frames) {
        const size_t n = frames - done < 128 ? frames - done : 128;
        if (!ss_render(s, tmp, n, rate)) return done > 0;
        for (size_t i = 0; i < n; i++) {
            for (int c = 0; c < channels; c++) {
                const float a = channels == 2 ? tmp[2 * i + c] : 0.5f * (tmp[2 * i] + tmp[2 * i + 1]);
                uint8_t* q = p + ((done + i) * (size_t)channels + (size_t)c) * (size_t)sb;
                switch (fmt) {
                case SS_FMT_S16_LE: {
                    const int64_t v = (int16_t)(q[0] | (q[1] << 8)) + (int64_t)(a * 32767.0f);
                    const int32_t m = ss_clamp(v, -32768, 32767);
                    q[0] = (uint8_t)(m & 0xFF);
                    q[1] = (uint8_t)((m >> 8) & 0xFF);
                    break;
                }
                case SS_FMT_S24_3LE: case SS_FMT_S24_LE: {
                    int32_t x = (int32_t)q[0] | ((int32_t)q[1] << 8) | ((int32_t)q[2] << 16);
                    if (x & 0x800000) x -= 0x1000000;
                    const int32_t m = ss_clamp((int64_t)x + (int64_t)(a * 8388607.0f), -8388608, 8388607);
                    q[0] = (uint8_t)(m & 0xFF);
                    q[1] = (uint8_t)((m >> 8) & 0xFF);
                    q[2] = (uint8_t)((m >> 16) & 0xFF);
                    if (fmt == SS_FMT_S24_LE) q[3] = (uint8_t)(m < 0 ? 0xFF : 0x00);
                    break;
                }
                case SS_FMT_S32_LE: {
                    int32_t x;
                    memcpy(&x, q, 4);
                    const int32_t m = ss_clamp((int64_t)x + (int64_t)(a * 2147483647.0f), INT32_MIN, INT32_MAX);
                    memcpy(q, &m, 4);
                    break;
                }
                }
            }
        }
        done += n;
    }
    return 1;
}

/* The control line the shell writes to /tmp/cinder_ambient and the shim reads: "<sound> <milli>",
 * milli = the level as 0..1000. Returns 1 when it parsed; anything malformed reads as off. */
static inline int ss_parse_control(const char* b, size_t n, int* sound, int* milli)
{
    int v[2] = { 0, 0 }, k = 0, digits = 0;
    *sound = SS_OFF;
    *milli = 0;
    for (size_t i = 0; i < n && b[i] && k < 2; i++) {
        const char c = b[i];
        if (c >= '0' && c <= '9') {
            if (v[k] < 100000) v[k] = v[k] * 10 + (c - '0');
            digits++;
        } else if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
            if (digits) { k++; digits = 0; }
        } else {
            return 0;
        }
    }
    if (digits) k++;
    if (k < 2 || v[0] >= SS_COUNT) return 0;
    *sound = v[0];
    *milli = v[1] > 1000 ? 1000 : v[1];
    return 1;
}

#endif /* CINDER_SOUNDSCAPE_H */
