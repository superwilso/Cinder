// SPDX-License-Identifier: GPL-2.0
/*
 * hci_stp — the MT8590's Bluetooth core as an HCI device, straight over STP (Cinder, Phase 4c;
 * design: analysis/DESIGN_hci_stp.md). Replaces the btbridge + /dev/vhci path: no user process
 * between the core and the Bluetooth stack.
 *
 * STP's BlueZ mode hands every Bluetooth packet to the callback registered here instead of to the
 * ring /dev/stpbt reads; sending is one STP call per packet. The calls are the kernel image's
 * exported wrappers (common_detect/wmt_stp_exp.c), each with a trailing chip type, 0 = SOC.
 *
 *   insmod compat.ko; insmod bluetooth.ko; insmod hci_stp.ko [alt=1]
 *
 * Sony's Bluetooth must be off first (its mtkbt holds /dev/stpbt): bringing hci0 up takes the
 * core's packets, and bringing it down gives STP back as it was.
 */
#include <linux/err.h>
#include <linux/fs.h>
#include <linux/module.h>
#include <linux/skbuff.h>
#include <linux/spinlock.h>
#include <linux/uaccess.h>
#include <net/bluetooth/bluetooth.h>
#include <net/bluetooth/hci_core.h>

/* The kernel's exports (wmt_stp_exp.h, MTK_WCN_BOOL = int, CHIP_TYPE_SOC = 0). */
typedef void (*stp_if_rx_t)(unsigned char *const data, int size);
extern int mtk_wcn_stp_register_if_rx(stp_if_rx_t func, int chip_type);
extern int mtk_wcn_stp_send_data(unsigned char *const buf, const unsigned int len, const unsigned char type, int chip_type);
extern void mtk_wcn_stp_set_bluez(int flag, int chip_type);
extern int mtk_wcn_wmt_func_on(int drv_type, int chip_type);
extern int mtk_wcn_wmt_func_off(int drv_type, int chip_type);
enum { CHIP_SOC = 0, WMT_BT = 0, STP_BT = 0 };

static bool alt;
module_param(alt, bool, 0444);
MODULE_PARM_DESC(alt, "use a different address than Sony's stack (keeps the headphones' pairing with it)");

static const char kNvram[] = "/data/BT_Addr";   /* 64 bytes: address, voice, codec, radio, sleep, ... */

static struct hci_dev *g_hdev;
static DEFINE_SPINLOCK(g_rx_lock);
static unsigned char g_rx[2048];
static size_t g_have;

/* Length of the H4 packet at b (n bytes so far): 0 = not enough yet, -1 = not a packet type. */
static long h4_len(const unsigned char *b, size_t n)
{
	if (n < 1)
		return 0;
	switch (b[0]) {
	case HCI_EVENT_PKT:
		return n >= 3 ? 3 + b[2] : 0;
	case HCI_ACLDATA_PKT:
		return n >= 5 ? 5 + (b[3] | b[4] << 8) : 0;
	case HCI_SCODATA_PKT:
		return n >= 4 ? 4 + b[3] : 0;
	default:
		return -1;
	}
}

/* STP's receive path, with BlueZ mode on: one STP payload, normally one whole H4 packet. */
static void stp_rx(unsigned char *const data, int size)
{
	struct hci_dev *hdev = g_hdev;
	unsigned long flags;

	if (!hdev || size <= 0)
		return;
	spin_lock_irqsave(&g_rx_lock, flags);
	if (g_have + size > sizeof(g_rx)) {
		BT_ERR("hci_stp: receive buffer overflow, %zu + %d bytes dropped", g_have, size);
		g_have = 0;
		spin_unlock_irqrestore(&g_rx_lock, flags);
		return;
	}
	memcpy(g_rx + g_have, data, size);
	g_have += size;
	while (g_have) {
		long len = h4_len(g_rx, g_have);
		struct sk_buff *skb;

		if (len < 0) {   /* lost sync: drop a byte and look again, as btbridge does */
			memmove(g_rx, g_rx + 1, --g_have);
			continue;
		}
		if (len == 0 || (size_t)len > g_have)
			break;
		/* The core says it has feature page 2, then refuses to read it (status 0x30), and the
		 * stack's start-up fails on that. Its last page is 1 (btbridge, 2026-11-19). */
		if (g_rx[0] == HCI_EVENT_PKT && g_rx[1] == HCI_EV_CMD_COMPLETE && len >= 9 &&
		    (g_rx[4] | g_rx[5] << 8) == HCI_OP_READ_LOCAL_EXT_FEATURES && g_rx[8] > 1)
			g_rx[8] = 1;
		skb = bt_skb_alloc(len - 1, GFP_ATOMIC);
		if (skb) {
			memcpy(skb_put(skb, len - 1), g_rx + 1, len - 1);
			bt_cb(skb)->pkt_type = g_rx[0];
			hdev->stat.byte_rx += len - 1;
			hci_recv_frame(hdev, skb);
		} else {
			hdev->stat.err_rx++;
		}
		memmove(g_rx, g_rx + len, g_have - len);
		g_have -= len;
	}
	spin_unlock_irqrestore(&g_rx_lock, flags);
}

static int stp_open(struct hci_dev *hdev)
{
	if (test_bit(HCI_RUNNING, &hdev->flags))
		return 0;
	if (!mtk_wcn_wmt_func_on(WMT_BT, CHIP_SOC)) {
		BT_ERR("hci_stp: WMT would not power the Bluetooth core on");
		return -EIO;
	}
	g_have = 0;
	mtk_wcn_stp_set_bluez(1, CHIP_SOC);
	mtk_wcn_stp_register_if_rx(stp_rx, CHIP_SOC);
	set_bit(HCI_RUNNING, &hdev->flags);
	return 0;
}

static int stp_close(struct hci_dev *hdev)
{
	if (!test_and_clear_bit(HCI_RUNNING, &hdev->flags))
		return 0;
	mtk_wcn_stp_register_if_rx(NULL, CHIP_SOC);
	mtk_wcn_stp_set_bluez(0, CHIP_SOC);
	mtk_wcn_wmt_func_off(WMT_BT, CHIP_SOC);
	return 0;
}

static int stp_flush(struct hci_dev *hdev)
{
	unsigned long flags;

	spin_lock_irqsave(&g_rx_lock, flags);
	g_have = 0;
	spin_unlock_irqrestore(&g_rx_lock, flags);
	return 0;
}

static int stp_send(struct hci_dev *hdev, struct sk_buff *skb)
{
	int len, sent;

	if (!test_bit(HCI_RUNNING, &hdev->flags)) {
		kfree_skb(skb);
		return -EBUSY;
	}
	memcpy(skb_push(skb, 1), &bt_cb(skb)->pkt_type, 1);
	len = skb->len;
	sent = mtk_wcn_stp_send_data(skb->data, len, STP_BT, CHIP_SOC);
	kfree_skb(skb);
	if (sent != len) {
		hdev->stat.err_tx++;
		return -EIO;
	}
	hdev->stat.byte_tx += len;
	return 0;
}

/* The NVRAM record, read the way Sony's stp_chrdev_bt.c reads its NVRAM file. */
static int read_nvram(unsigned char *nv, size_t n)
{
	struct file *f = filp_open(kNvram, O_RDONLY, 0);
	mm_segment_t fs;
	loff_t pos = 0;
	ssize_t got;

	if (IS_ERR(f))
		return PTR_ERR(f);
	fs = get_fs();
	set_fs(KERNEL_DS);
	got = vfs_read(f, (char __user *)nv, n, &pos);
	set_fs(fs);
	filp_close(f, NULL);
	return got == (ssize_t)n ? 0 : -EIO;
}

static int vendor_cmd(struct hci_dev *hdev, u16 op, const void *p, u32 n)
{
	struct sk_buff *skb = __hci_cmd_sync(hdev, op, n, p, HCI_INIT_TIMEOUT);
	int st;

	if (IS_ERR(skb)) {
		BT_ERR("hci_stp: command 0x%04x: %ld", op, PTR_ERR(skb));
		return PTR_ERR(skb);
	}
	st = skb->len ? skb->data[0] : 0;
	kfree_skb(skb);
	if (st)
		BT_ERR("hci_stp: command 0x%04x status 0x%02x", op, st);
	return st ? -EIO : 0;
}

/* Power-on, before the stack's own start-up: btbridge's four commands (PLAN_hardware_first.md,
 * "The power-on commands, decoded 2026-11-18"), from the NVRAM record. */
static int stp_setup(struct hci_dev *hdev)
{
	unsigned char nv[64], addr[6];
	int i, err = read_nvram(nv, sizeof(nv));

	if (err) {
		BT_ERR("hci_stp: cannot read %s (%d)", kNvram, err);
		return err;
	}
	for (i = 0; i < 6; i++)
		addr[i] = nv[5 - i];
	if (alt)
		addr[0] ^= 0x5a;
	err = vendor_cmd(hdev, 0xfc1a, addr, 6);
	if (!err)
		err = vendor_cmd(hdev, 0xfc79, nv + 0x0c, 6);   /* radio */
	if (!err)
		err = vendor_cmd(hdev, 0xfc93, nv + 0x1b, 3);   /* TX power offset */
	if (!err)
		err = vendor_cmd(hdev, 0xfc7a, nv + 0x12, 7);   /* sleep */
	return err;
}

static int __init hci_stp_init(void)
{
	struct hci_dev *hdev = hci_alloc_dev();
	int err;

	if (!hdev)
		return -ENOMEM;
	hdev->bus = HCI_UART;   /* BTIF is the SoC's UART-like link to the core */
	hdev->dev_type = HCI_BREDR;
	hdev->open = stp_open;
	hdev->close = stp_close;
	hdev->flush = stp_flush;
	hdev->send = stp_send;
	hdev->setup = stp_setup;
	g_hdev = hdev;
	err = hci_register_dev(hdev);
	if (err < 0) {
		g_hdev = NULL;
		hci_free_dev(hdev);
		return err;
	}
	return 0;
}

static void __exit hci_stp_exit(void)
{
	struct hci_dev *hdev = g_hdev;

	hci_unregister_dev(hdev);   /* closes it first: STP back to Sony's ring, the core off */
	g_hdev = NULL;
	hci_free_dev(hdev);
}

module_init(hci_stp_init);
module_exit(hci_stp_exit);
MODULE_DESCRIPTION("MT8590 Bluetooth core over STP as an HCI device (Cinder)");
MODULE_LICENSE("GPL");
