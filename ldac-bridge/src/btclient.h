// btclient.h — control-plane interface to Sony's BtTransmitterServiceClient.
#ifndef LDAC_BRIDGE_BTCLIENT_H
#define LDAC_BRIDGE_BTCLIENT_H
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

typedef struct bt_client bt_client_t;

// BtLdacSoundQuality enum (from libBtTransmitterService strings: Auto/990/660/330).
// Exact integer values TODO — confirm from GetCapabilities / SetLdacSoundQuality RE.
typedef enum { BT_LDAC_AUTO = 0, BT_LDAC_990 = 1, BT_LDAC_660 = 2, BT_LDAC_330 = 3 } bt_ldac_quality_t;

bt_client_t *btclient_create(void);                          // factory CreateInstance + Connect
void         btclient_destroy(bt_client_t *c);
void         btclient_set_ldac(bt_client_t *c, bool on);              // SetLdac(const bool&)
void         btclient_set_ldac_quality(bt_client_t *c, bt_ldac_quality_t q); // SetLdacSoundQuality
void         btclient_notify_open_audio(bt_client_t *c);             // NotifyOpenAudio()
void         btclient_notify_close_audio(bt_client_t *c);            // NotifyCloseAudio()
uint16_t     btclient_pcm_preferred_size(bt_client_t *c);            // NotifyPcmPreferredSize/negotiated
// GetSocketName() returns a libc++ std::string; copy the C string into out.
void         btclient_get_socket_name(bt_client_t *c, char *out, size_t outlen);

#endif
