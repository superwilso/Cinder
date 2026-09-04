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

} // extern "C"
