/* btsniff — READ-ONLY observer for the MTK Bluetooth stack's UNIX sockets (Route A of
 * docs/PLAN_bluetooth_stack.md).
 *
 * It connects to /tmp/bt.app.gap (and any other socket named on the command line) and dumps
 * every byte that arrives, with timestamps and read boundaries. It NEVER writes to the socket:
 * there is no send()/write() call on a socket fd anywhere in this file, by design. Sony's stack
 * owns the transport, and the whole point of Route A is to learn the framing without the stack
 * being able to notice us.
 *
 * Built static-musl (no libc version dependency), run as root from adb:
 *     arm-linux-musleabihf-gcc -static -Os -Wall -o btsniff btsniff.c
 *     adb push btsniff /data/btsniff && adb shell '/data/btsniff -t 30'
 *
 * usage: btsniff [-t SECS] [-m MARKFILE] [PATH ...]
 *   -t SECS     stop after SECS (default 20; 0 = until killed)
 *   -m FILE     poll FILE once a second; whenever its contents change, print it as a LABEL line.
 *               That is how a capture gets labelled from a second shell while it runs:
 *                   echo "connect wh1000xm3" > /data/btmark
 *   PATH ...    sockets to watch (default: /tmp/bt.app.gap)
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/un.h>
#include <time.h>
#include <unistd.h>

#define MAXSOCK 8

static double now_s(void) {
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return (double)tv.tv_sec + tv.tv_usec / 1e6;
}

static double t0;

static void stamp(void) { printf("[%8.3f] ", now_s() - t0); }

static void hexdump(const unsigned char *p, int n, const char *tag) {
    int i, j;
    for (i = 0; i < n; i += 16) {
        printf("           %s %04x  ", tag, i);
        for (j = 0; j < 16; j++) {
            if (i + j < n) printf("%02x ", p[i + j]);
            else           printf("   ");
            if (j == 7) putchar(' ');
        }
        printf(" |");
        for (j = 0; j < 16 && i + j < n; j++) {
            unsigned char c = p[i + j];
            putchar(c >= 0x20 && c < 0x7f ? c : '.');
        }
        printf("|\n");
    }
}

/* Connect one socket. Tries SOCK_STREAM then SOCK_SEQPACKET then SOCK_DGRAM: the node type is
 * 's' for all of them, and getting it wrong returns EPROTOTYPE, which is information too. */
static int connect_sock(const char *path, int *type_out) {
    static const int types[3] = { SOCK_STREAM, SOCK_SEQPACKET, SOCK_DGRAM };
    static const char *names[3] = { "STREAM", "SEQPACKET", "DGRAM" };
    int i;
    for (i = 0; i < 3; i++) {
        struct sockaddr_un sa;
        int fd = socket(AF_UNIX, types[i], 0);
        if (fd < 0) continue;
        memset(&sa, 0, sizeof sa);
        sa.sun_family = AF_UNIX;
        strncpy(sa.sun_path, path, sizeof sa.sun_path - 1);
        if (connect(fd, (struct sockaddr *)&sa, sizeof sa) == 0) {
            stamp();
            printf("OPEN  %s  as SOCK_%s  fd=%d\n", path, names[i], fd);
            *type_out = types[i];
            return fd;
        }
        stamp();
        printf("open  %s  SOCK_%-9s -> %s\n", path, names[i], strerror(errno));
        close(fd);
    }
    return -1;
}

int main(int argc, char **argv) {
    const char *paths[MAXSOCK];
    const char *markfile = NULL;
    int npath = 0, secs = 20, i;
    int fds[MAXSOCK];
    long long total[MAXSOCK];
    char lastmark[256];
    double next_mark_poll;

    t0 = now_s();
    setvbuf(stdout, NULL, _IOLBF, 0);

    for (i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "-t") && i + 1 < argc)      secs = atoi(argv[++i]);
        else if (!strcmp(argv[i], "-m") && i + 1 < argc) markfile = argv[++i];
        else if (npath < MAXSOCK)                        paths[npath++] = argv[i];
    }
    if (npath == 0) paths[npath++] = "/tmp/bt.app.gap";

    {
        time_t w = time(NULL);
        struct tm *tm = localtime(&w);
        char buf[64];
        strftime(buf, sizeof buf, "%Y-%m-%d %H:%M:%S", tm);
        printf("# btsniff  %s  read-only, %d socket(s), %ds\n", buf, npath, secs);
    }

    for (i = 0; i < npath; i++) {
        int type = 0;
        fds[i] = connect_sock(paths[i], &type);
        total[i] = 0;
        if (fds[i] < 0) {
            stamp();
            printf("FAIL  %s  — no socket type connected\n", paths[i]);
        }
    }

    lastmark[0] = 0;
    next_mark_poll = now_s();

    for (;;) {
        struct pollfd pfd[MAXSOCK];
        int np = 0, map[MAXSOCK], r;
        double t = now_s();

        if (secs > 0 && t - t0 > secs) break;

        if (markfile && t >= next_mark_poll) {
            int mf = open(markfile, O_RDONLY);
            next_mark_poll = t + 1.0;
            if (mf >= 0) {
                char buf[256];
                int n = read(mf, buf, sizeof buf - 1);
                close(mf);
                if (n > 0) {
                    buf[n] = 0;
                    while (n > 0 && (buf[n - 1] == '\n' || buf[n - 1] == '\r')) buf[--n] = 0;
                    if (strcmp(buf, lastmark)) {
                        strncpy(lastmark, buf, sizeof lastmark - 1);
                        stamp();
                        printf("LABEL ---- %s ----\n", lastmark);
                    }
                }
            }
        }

        for (i = 0; i < npath; i++) {
            if (fds[i] < 0) continue;
            pfd[np].fd = fds[i];
            pfd[np].events = POLLIN;
            pfd[np].revents = 0;
            map[np] = i;
            np++;
        }
        if (np == 0) { stamp(); printf("no live sockets, exiting\n"); break; }

        r = poll(pfd, np, 500);
        if (r < 0) {
            if (errno == EINTR) continue;
            stamp(); printf("poll: %s\n", strerror(errno));
            break;
        }
        for (i = 0; i < np; i++) {
            int k = map[i];
            if (pfd[i].revents & (POLLIN | POLLHUP | POLLERR)) {
                unsigned char buf[4096];
                int n = (int)recv(pfd[i].fd, buf, sizeof buf, MSG_DONTWAIT);
                if (n > 0) {
                    total[k] += n;
                    stamp();
                    printf("RX %-20s %4d bytes (total %lld)\n", paths[k], n, total[k]);
                    hexdump(buf, n, "");
                } else if (n == 0) {
                    stamp();
                    printf("EOF   %s  — peer closed (total %lld)\n", paths[k], total[k]);
                    close(fds[k]);
                    fds[k] = -1;
                } else if (errno != EAGAIN && errno != EWOULDBLOCK) {
                    stamp();
                    printf("ERR   %s  recv: %s\n", paths[k], strerror(errno));
                    close(fds[k]);
                    fds[k] = -1;
                }
            }
        }
    }

    for (i = 0; i < npath; i++) {
        stamp();
        printf("DONE  %-20s %lld bytes\n", paths[i], total[i]);
        if (fds[i] >= 0) close(fds[i]);
    }
    return 0;
}
