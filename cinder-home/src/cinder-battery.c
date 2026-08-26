/* cinder-battery — read the bq24262 charger's registers. Setuid root, READ-ONLY.
 *
 * WHY THIS EXISTS: everything the device will tell you about its own battery from ordinary files
 * is four numbers. /sys/class/power_supply/battery/ has capacity, status, health and voltage_now
 * and nothing else — no current, no temperature, no cycle count. There is no fuel gauge exposing
 * coulomb counting on this platform, and /sys/devices/platform/mt-auxadc publishes only per-channel
 * offset/slope calibration constants, not live channel readings. So a battery screen built on sysfs
 * alone can show a percentage and a voltage, and must guess at everything else.
 *
 * The charger IC knows more. Sony's driver registers it with the kernel's generic register monitor:
 *
 *     /proc/regmon/bq24262/target   write = select a register, read = the register-name table
 *     /proc/regmon/bq24262/value    read THAT register, over I2C, live
 *
 * Seven registers, and between them they carry the charge state machine, the fault code, the input
 * current limit, the charge current and termination settings, and the battery regulation voltage —
 * the last of which is how you find out what this device actually charges the cell TO. Measured on
 * this unit 2026-08-26: BATTERY_VOLTAGE reads 0x78, which is not the 4.2 V a stock lithium cell
 * usually gets.
 *
 * WHY A HELPER: both nodes are `-rw------- root root`, and cinder-home runs as uid 100 with an
 * empty capability set.
 *
 * WHY THIS ONE PRINTS INSTEAD OF WIDENING PERMISSIONS — the important difference from cinder-fm.
 * cinder-fm chmods its two regmon nodes 0666 and lets the shell talk to the tuner directly. That
 * is a defensible trade for a radio receiver: the worst a bad write does is detune it until the
 * driver's next Open(). It is NOT a defensible trade here. Writing bq24262 `value` reprograms a
 * lithium battery charger — the regulation voltage, the current limit, the safety timer — and the
 * failure mode is a damaged or dangerous cell, not a mistuned radio. There is no reason any part
 * of Cinder needs to WRITE these registers, so this helper never makes it possible: it reads them
 * itself, prints them, and exits. `value` is opened O_RDONLY and nothing else in this program ever
 * opens it any other way.
 *
 * The one write it does make is to `target`, which is the register SELECTOR, not the register. The
 * seven values it can ever write are the fixed list below. Nothing comes from the caller: no argv,
 * no environment, no paths. The i2c bus itself (/dev/i2c-2) stays untouched.
 *
 * OUTPUT, one line per register, on stdout:
 *
 *     reg0 0x00000010 STATUS
 *     reg1 0x000000AC CONTROL
 *     ...
 *
 * A register that cannot be read is simply omitted, so a partial read still yields usable lines
 * and the caller can tell what it got by what is there. Decoding lives in the caller — this stays
 * a dumb, auditable pipe.
 *
 * Exit: 0 = every register read, 3 = not setuid root, 4 = the regmon nodes are not there,
 * 5 = some registers could not be read (count is not encoded; check the output).
 */
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define TARGET "/proc/regmon/bq24262/target"
#define VALUE  "/proc/regmon/bq24262/value"

/* The bq24262's whole register map, as the kernel's own target table names it. Fixed at compile
 * time precisely so no caller can ever ask for an address that is not one of these. */
static const struct { int idx; const char *name; } REGS[] = {
    { 0, "STATUS" },
    { 1, "CONTROL" },
    { 2, "BATTERY_VOLTAGE" },
    { 3, "VENDOR" },
    { 4, "BATTERY_CURRENT" },
    { 5, "VIN_MINSYS" },
    { 6, "SAFETY" },
};
#define NREGS ((int)(sizeof REGS / sizeof REGS[0]))

/* Select a register by writing its index to the selector node. Returns 0 on success.
 * The value written is always REGS[i].idx — never anything derived from input. */
static int select_reg(int idx)
{
    char buf[16];
    int fd, n;

    fd = open(TARGET, O_WRONLY | O_NOFOLLOW);
    if (fd < 0)
        return -1;
    n = snprintf(buf, sizeof buf, "%d\n", idx);
    if (write(fd, buf, (size_t)n) != n) {
        close(fd);
        return -1;
    }
    close(fd);
    return 0;
}

/* Read the currently selected register. O_RDONLY, always — see the header comment. */
static int read_value(char *out, size_t outsz)
{
    int fd;
    ssize_t n;

    fd = open(VALUE, O_RDONLY | O_NOFOLLOW);
    if (fd < 0)
        return -1;
    n = read(fd, out, outsz - 1);
    close(fd);
    if (n <= 0)
        return -1;
    out[n] = '\0';
    /* The node returns a trailing newline; trim it so the caller gets one clean token. */
    while (n > 0 && (out[n - 1] == '\n' || out[n - 1] == '\r' || out[n - 1] == ' '))
        out[--n] = '\0';
    return 0;
}

int main(void)
{
    struct stat st;
    char val[64];
    int i, failed = 0;

    /* Refuse to run un-privileged rather than printing a confusing string of read failures.
     * setuid(0) first: a setuid binary starts with euid 0 but ruid 100, and some kernels apply
     * the stricter of the two to procfs opens. Same treatment cinder-clock gives it. */
    if (geteuid() != 0)
        return 3;
    if (setuid(0) != 0)
        return 3;

    if (stat(TARGET, &st) != 0 || stat(VALUE, &st) != 0)
        return 4;

    for (i = 0; i < NREGS; i++) {
        if (select_reg(REGS[i].idx) != 0) {
            failed++;
            continue;
        }
        if (read_value(val, sizeof val) != 0) {
            failed++;
            continue;
        }
        printf("reg%d %s %s\n", REGS[i].idx, val, REGS[i].name);
    }
    fflush(stdout);

    return failed ? 5 : 0;
}
