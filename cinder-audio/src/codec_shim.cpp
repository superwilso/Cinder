// codec_shim.cpp — implements cinder_codec.h by driving the CXD3778GF's `standby` ALSA control
// directly over the kernel control device.
//
// NO libasound. cinder-home does not link ALSA today and this is not a good enough reason to make
// it: the whole job is two ioctls on a character device, and adding a shared-library dependency to
// the Home app buys a way for the device to boot with no launcher. cinder-probe does link libasound
// (for the FM capture work) but shares this file, so the flat ioctl keeps them identical.
//
// BY NAME, NOT BY numid. On the reference firmware `standby` happens to be numid 35, but numid is
// just the order the driver registered its controls in — a firmware that registers one more control
// ahead of it silently renumbers everything after. The kernel's control lookup matches on
// (iface, name, index, device, subdevice) when numid is 0, so addressing by name costs nothing and
// cannot land on the wrong control.
#include "cinder_codec.h"

#include <fcntl.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <cstring>
#include <sound/asound.h>

namespace {

const char kControlDev[] = "/dev/snd/controlC0";
const char kStandbyCtl[] = "standby";

// The SE gain control, spelled out. There are three similarly named controls and Sony's driver
// mis-wires one of them: `headphone smaster gain mode` and `headphone smaster btl gain mode` both
// resolve to the BTL handler in `cxd3778gf_snd_controls`. BTL is the balanced output, which this
// model does not have. This is the one that reaches the 3.5 mm jack.
const char kSeGainCtl[]  = "headphone smaster se gain mode";
const char kLatencyCtl[] = "playback latency";
const char kJackSeCtl[]  = "jack status se";
const char kMasterVolCtl[] = "master volume";

// Fill in an element id that addresses a mixer control by name. numid stays 0 so the kernel
// resolves by name rather than by number.
void fill_id(snd_ctl_elem_id& id, const char* name) {
    std::memset(&id, 0, sizeof(id));
    id.iface = SNDRV_CTL_ELEM_IFACE_MIXER;
    std::strncpy(reinterpret_cast<char*>(id.name), name, sizeof(id.name) - 1);
}

// One ioctl against the control device. Returns 0 on success. The fd is opened per call rather
// than cached: these run at most once per screen blank, an open+close on a character device is
// microseconds, and holding an audio fd open across the life of the Home app is exactly the sort
// of thing that turns into "every PCM open returns -EBUSY for the rest of the boot" (which this
// device has done before, from AudioInPlayerService).
int ctl_ioctl(unsigned long req, snd_ctl_elem_value* v) {
    int fd = open(kControlDev, O_RDWR | O_CLOEXEC);
    if (fd < 0) return -1;
    int rc = ioctl(fd, req, v);
    close(fd);
    return rc < 0 ? -1 : 0;
}

// ENUMERATED controls carry their value in `value.enumerated.item[]`, not
// `value.integer.value[]`. On this 32-bit ARM build the two union members happen to overlap, so
// using the wrong one would work by accident here and break on anything else. Use the right one.
int get_enum(const char* name) {
    snd_ctl_elem_value v;
    std::memset(&v, 0, sizeof(v));
    fill_id(v.id, name);
    if (ctl_ioctl(SNDRV_CTL_IOCTL_ELEM_READ, &v) != 0) return -1;
    return static_cast<int>(v.value.enumerated.item[0]);
}

int set_enum(const char* name, int item) {
    snd_ctl_elem_value v;
    std::memset(&v, 0, sizeof(v));
    fill_id(v.id, name);
    v.value.enumerated.item[0] = static_cast<unsigned int>(item);
    return ctl_ioctl(SNDRV_CTL_IOCTL_ELEM_WRITE, &v);
}

// INTEGER controls, as opposed to the ENUMERATED ones above. `value.integer.value[]` is an array
// of `long`; the ALSA ABI is explicit about that, so do not narrow it on the way in.
int get_int(const char* name) {
    snd_ctl_elem_value v;
    std::memset(&v, 0, sizeof(v));
    fill_id(v.id, name);
    if (ctl_ioctl(SNDRV_CTL_IOCTL_ELEM_READ, &v) != 0) return -1;
    return static_cast<int>(v.value.integer.value[0]);
}

int set_int(const char* name, int val) {
    snd_ctl_elem_value v;
    std::memset(&v, 0, sizeof(v));
    fill_id(v.id, name);
    v.value.integer.value[0] = val;
    return ctl_ioctl(SNDRV_CTL_IOCTL_ELEM_WRITE, &v);
}

} // namespace

extern "C" {

int cinder_codec_get_standby(void) {
    snd_ctl_elem_value v;
    std::memset(&v, 0, sizeof(v));
    fill_id(v.id, kStandbyCtl);
    if (ctl_ioctl(SNDRV_CTL_IOCTL_ELEM_READ, &v) != 0) return -1;
    return v.value.integer.value[0] ? 1 : 0;
}

int cinder_codec_set_standby(int on) {
    snd_ctl_elem_value v;
    std::memset(&v, 0, sizeof(v));
    fill_id(v.id, kStandbyCtl);
    v.value.integer.value[0] = (on != 0) ? 1 : 0;
    return ctl_ioctl(SNDRV_CTL_IOCTL_ELEM_WRITE, &v);
}

int cinder_codec_get_gain_mode(void)            { return get_enum(kSeGainCtl); }
int cinder_codec_set_gain_mode(int high)        { return set_enum(kSeGainCtl, high != 0 ? 1 : 0); }
int cinder_codec_get_playback_latency(void)     { return get_enum(kLatencyCtl); }
int cinder_codec_set_playback_latency(int low)  { return set_enum(kLatencyCtl, low != 0 ? 1 : 0); }
int cinder_codec_get_jack_se(void)              { return get_enum(kJackSeCtl); }
int cinder_codec_get_master_volume(void)        { return get_int(kMasterVolCtl); }
int cinder_codec_set_master_volume(int v)       { return set_int(kMasterVolCtl, v); }

} // extern "C"
