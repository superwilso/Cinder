/* cinder_power.h — C ABI over Sony's PowerMgrServiceClient (libPowerMgrServiceClient.so) for the
 * battery-care ("Itawari" considerate charging, caps at ~90%) toggle on Cinder's Settings screen.
 *
 * SAFETY: PowerMgrServiceClient is constructed lazily (the ctor connects to the power service and
 * can crash/hang if it's down), so every entry point must be called from behind cinder-home's
 * run_guarded crash+hang guard. Object size is RE-confirmed 8 bytes and reserved 0x10
 * (power_abi.hpp), so `new PowerMgrServiceClient` can't overflow the heap. */
#ifndef CINDER_POWER_H
#define CINDER_POWER_H
#ifdef __cplusplus
extern "C" {
#endif

/* Read whether battery care (Itawari charging) is currently enabled on the device.
 * Returns 1 = on, 0 = off, -1 = unavailable (service not reachable / not constructed). */
int cinder_power_get_battery_care(void);

/* Enable (on != 0) or disable battery care on the device. Returns 0 = applied, -1 = unavailable. */
int cinder_power_set_battery_care(int on);

/* Restart the device (PowerMgrServiceClient::Reboot). 0 = requested, -1 = service unavailable.
 * Does not return on success in practice — the device goes down under it. */
int cinder_power_reboot(void);

/* Power the device off (PowerMgrServiceClient::SetStatus(PowerOff)). 0 = requested, -1 =
 * unavailable. The PowerStatus enum value is INFERRED from the service's own name table and is
 * device-unverified; a wrong value yields a sleep state, recoverable with the power button. */
int cinder_power_off(void);

#ifdef __cplusplus
}
#endif
#endif /* CINDER_POWER_H */
