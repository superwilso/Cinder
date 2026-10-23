/* fmrec.h — FM recording: the WAV writer both radio readers share (the `fmrec` component).
 *
 * Two processes read the radio's PCM from hw:0,1, never both at once: Sony's AudioInPlayerService
 * when the radio plays on the headphone jack (src/cinder-fmrec.c is preloaded into it), and
 * cinder-home's own Bluetooth bridge when BT OUT is on (fmbt_thread in main.cpp). Each feeds every
 * block it reads to fmrec_feed(), and that is the whole recorder.
 *
 * CONTROL IS ONE FILE. cinder-home writes the output path into FMREC_CTL to start and deletes it to
 * stop; the reader polls it about four times a second. While writing, the reader keeps FMREC_STATE
 * as "1 <path>"; it writes "0 <why>" when it stops, so the shell can say what happened.
 *
 * THE FILE IS PLAYABLE AT EVERY MOMENT. The WAV header's two sizes are rewritten every five seconds,
 * so a recording cut short by a crash, a full drive or a pulled battery loses at most those five
 * seconds rather than becoming a file every player rejects. It stops on its own short of 4 GB,
 * where the header's sizes run out (about six and a half hours at 44.1 kHz stereo).
 *
 * Header-only and plain C so the preload (C, glibc only) and main.cpp (C++) share one copy. No
 * allocation; stdio only. Every failure stops the recording and never the audio it is fed from.
 */
#ifndef CINDER_FMREC_H
#define CINDER_FMREC_H

#include <stdio.h>
#include <string.h>
#include <sys/stat.h>

#ifndef FMREC_CTL
#define FMREC_CTL   "/tmp/cinder_fmrec"
#endif
#ifndef FMREC_STATE
#define FMREC_STATE "/tmp/cinder_fmrec.state"
#endif
#define FMREC_LIMIT 0xFFF00000UL   /* stop short of the 32-bit RIFF size */

struct fmrec {
    FILE* f;
    unsigned long bytes;        /* PCM bytes written */
    unsigned long since_poll;   /* frames since FMREC_CTL was last looked at */
    unsigned long since_fix;    /* frames since the header was last rewritten */
    unsigned rate, ch, bps;     /* bps = bytes per sample */
    int refused;                /* the last start failed: wait until FMREC_CTL goes away */
};

static void fmrec_state(const char* s)
{
    FILE* f = fopen(FMREC_STATE, "w");
    if (f) { fputs(s, f); fputc('\n', f); fclose(f); }
}

static void fmrec_le(unsigned char* p, unsigned long v, int n)
{
    for (int i = 0; i < n; ++i) p[i] = (unsigned char)(v >> (8 * i));
}

/* Writes (or rewrites) the 44-byte header at the start of the file. Returns 0 on success. */
static int fmrec_header(struct fmrec* r)
{
    unsigned char h[44];
    memcpy(h, "RIFF", 4);  fmrec_le(h + 4, 36 + r->bytes, 4);
    memcpy(h + 8, "WAVEfmt ", 8);
    fmrec_le(h + 16, 16, 4);  fmrec_le(h + 20, 1, 2);  fmrec_le(h + 22, r->ch, 2);
    fmrec_le(h + 24, r->rate, 4);  fmrec_le(h + 28, (unsigned long)r->rate * r->ch * r->bps, 4);
    fmrec_le(h + 32, r->ch * r->bps, 2);  fmrec_le(h + 34, 8 * r->bps, 2);
    memcpy(h + 36, "data", 4);  fmrec_le(h + 40, r->bytes, 4);
    if (fflush(r->f) != 0 || fseek(r->f, 0, SEEK_SET) != 0) return -1;
    if (fwrite(h, 1, sizeof h, r->f) != sizeof h) return -1;
    if (fseek(r->f, 0, SEEK_END) != 0 || fflush(r->f) != 0) return -1;
    return 0;
}

/* Closes the file with its final sizes. `why` goes to FMREC_STATE. Safe to call when idle. */
static void fmrec_stop(struct fmrec* r, const char* why)
{
    if (!r->f) return;
    fmrec_header(r);
    fclose(r->f);
    r->f = NULL;
    char s[96];
    snprintf(s, sizeof s, "0 %s", why);
    fmrec_state(s);
}

/* Feed one block the reader has just read. `bps` is bytes per sample: 2 for S16_LE, 4 for S32_LE.
 * Anything else (or an unknown rate) is not recorded. */
static void fmrec_feed(struct fmrec* r, const void* buf, unsigned long frames,
                       unsigned rate, unsigned ch, unsigned bps)
{
    if (!buf || frames == 0 || rate == 0 || ch == 0 || (bps != 2 && bps != 4)) return;
    r->since_poll += frames;
    if (r->since_poll >= rate / 4) {
        r->since_poll = 0;
        struct stat st;
        const int want = stat(FMREC_CTL, &st) == 0;
        if (!want) {
            r->refused = 0;
            if (r->f) fmrec_stop(r, "stopped");
        } else if (!r->f && !r->refused) {
            char path[256] = {0};
            FILE* c = fopen(FMREC_CTL, "r");
            if (c) {
                if (!fgets(path, sizeof path, c)) path[0] = 0;
                fclose(c);
            }
            path[strcspn(path, "\r\n")] = 0;
            /* A reader that cannot open the file says so once and does not retry every poll: the
             * shell sees the state and gives up, which deletes FMREC_CTL and clears `refused`. */
            if (path[0] && (r->f = fopen(path, "wb")) != NULL) {
                r->bytes = 0;
                r->since_fix = 0;
                r->rate = rate; r->ch = ch; r->bps = bps;
                if (fmrec_header(r) == 0) {
                    char s[300];
                    snprintf(s, sizeof s, "1 %s", path);
                    fmrec_state(s);
                } else {
                    fclose(r->f);
                    r->f = NULL;
                    r->refused = 1;
                    fmrec_state("0 could not write the file");
                }
            } else {
                r->refused = 1;
                fmrec_state("0 could not create the file");
            }
        }
    }
    if (!r->f) return;
    const unsigned long n = frames * ch * bps;
    if (r->bytes + n > FMREC_LIMIT) { fmrec_stop(r, "reached the 4 GB limit"); return; }
    if (fwrite(buf, 1, n, r->f) != n) { fmrec_stop(r, "the drive is full or went away"); return; }
    r->bytes += n;
    r->since_fix += frames;
    if (r->since_fix >= 5UL * r->rate) {
        r->since_fix = 0;
        if (fmrec_header(r) != 0) fmrec_stop(r, "the drive is full or went away");
    }
}

#endif
