/* cinder_storage.h — C ABI over Sony's StorageMgrServiceFwClient, for keeping the microSD card
 * mounted when a USB cable is plugged in.
 *
 * THE PROBLEM THIS EXISTS FOR. Connecting USB makes Sony's storage manager unmount the card and
 * hand the raw block device to the mass-storage gadget. A media scan that is in flight at that
 * moment loses everything it had indexed — measured 2026-09-03: 637 of the card's 1130 tracks
 * gone, and the whole card missing from the library. Every reconnect restarts the scan from zero,
 * so a card that is new or has just gained music can stay unindexed indefinitely. Turning the
 * auto-export setting off stops the handover; see storage_abi.hpp for the RE behind it.
 *
 * SAFETY. Every entry point here is an asynchronous pst binder call and MUST be made
 *   (a) after the easel app lifecycle has started pst::core::Framework, with the pump running —
 *       an unpumped client returns uninitialised stack, not an error, and
 *   (b) from behind cinder-home's run_guarded crash+hang guard.
 * libStorageMgrServiceFw.so is dlopen'd on first use rather than linked, so a missing or
 * unloadable library degrades to "unavailable" instead of stopping cinder-home from starting —
 * this runs in the Home app, and a Home app that will not start is a device the user has to
 * recover by hand. */
#ifndef CINDER_STORAGE_H
#define CINDER_STORAGE_H
#ifdef __cplusplus
extern "C" {
#endif

/* Storage ids, matching Sony's enum (storage_abi.hpp). External0 is the microSD slot. */
#define CINDER_STORAGE_INTERNAL  0
#define CINDER_STORAGE_EXTERNAL0 1
#define CINDER_STORAGE_EXTERNAL1 2

/* Is "hand storage to the PC automatically when a cable appears" currently on?
 * Returns 1 = on (stock default), 0 = off, -1 = unavailable (service not reachable). */
int cinder_storage_get_auto_export(void);

/* Turn that automatic handover on (on != 0) or off. Returns 0 = applied, -1 = failed.
 * The service persists this to NVP (DmpConfig FNC_MSC_AUTOEXPORT), so it survives a reboot;
 * Cinder re-applies it at startup anyway, because a factory reset restores the stock value. */
int cinder_storage_set_auto_export(int on);

/* Mount one storage now — used to bring the card back after it has been released by the gadget,
 * without waiting for a cable event. Returns 0 = mounted (or already mounted), -1 = failed.
 * Mounting is what makes Sony's scanner index the card; mounting it behind StorageMgr's back
 * (say from a setuid helper) would leave the tracks out of MTPDB.dat and therefore out of
 * Cinder, which reads that DB rather than walking the filesystem. */
int cinder_storage_mount(int storage);

/* Explicitly export everything to the PC as mass storage (on != 0), or take it back (on == 0).
 * This is the deliberate transfer path, and is unaffected by the auto-export setting.
 * Returns 0 = applied, -1 = failed. */
int cinder_storage_export_as_msc(int on);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_STORAGE_H */
