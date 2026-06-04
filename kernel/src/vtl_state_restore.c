// SPDX-License-Identifier: GPL-2.0-only
/*
 * vtl_state_restore.c — mhVTL-style changer state persistence.
 *
 * Kernel READS state files that vtladm writes after every successful ioctl.
 * Write pattern: atomic tmp→rename in userspace; kernel only reads at init.
 *
 * State file format (<state_dir>/changer-<instance>.state):
 *   # <type> <addr> <tape_name> <barcode>
 *   S 0 TAPE001 VTL000001
 *   D 0 TAPE002 VTL000002
 *   I 0 TAPE003 VTL000003
 *
 * Types: S = Storage slot, D = Drive, I = Import/Export mailslot
 * addr  = relative index (0..num_slots-1, 0..num_drives-1, etc.)
 */

#include "vtl.h"
#include <linux/fs.h>
#include <linux/string.h>
#include <linux/slab.h>
#include <linux/mm.h>

static char *vtl_state_dir = "/opt/vtladm/var";
module_param_named(state_dir, vtl_state_dir, charp, 0644);
MODULE_PARM_DESC(state_dir, "Directory for changer state files (mhVTL-style persistence)");

static bool vtl_state_restore = true;
module_param_named(state_restore, vtl_state_restore, bool, 0444);
MODULE_PARM_DESC(state_restore, "Restore changer state from files on module load (default Y)");

#define VTL_STATE_BUF_MAX  65536

/*
 * Find first unoccupied data slot.
 * SAFETY: only called from vtl_restore_one_changer_state() during module_init,
 * before SCSI scan is queued. No concurrent changer access possible at this point.
 */
static int find_first_empty_slot(struct vtl_changer *ch)
{
	int i;
	for (i = 0; i < ch->num_slots; i++) {
		if (!ch->slots[i].occupied)
			return i;
	}
	return -1;
}

/* Parse and apply one state-file line. Returns 1 on success, 0 on skip/error. */
static int apply_one_state_line(struct vtl_changer *ch, char *line)
{
	char type;
	int addr, empty_slot, ret;
	char name[64];
	char barcode[16];
	struct vtl_tape *tape;

	/* Skip comments and blank lines */
	while (*line == ' ' || *line == '\t')
		line++;
	if (*line == '#' || *line == '\n' || *line == '\r' || *line == '\0')
		return 0;

	memset(name, 0, sizeof(name));
	memset(barcode, 0, sizeof(barcode));

	if (sscanf(line, " %c %d %63s %15s", &type, &addr, name, barcode) < 3)
		return 0;

	if (addr < 0)
		return 0;

	tape = vtl_tape_open_existing(name);
	if (IS_ERR_OR_NULL(tape)) {
		pr_warn("VTL: state restore: cannot open tape '%s' (err=%ld), skipping\n",
			name, tape ? PTR_ERR(tape) : -ENOMEM);
		return 0;
	}

	if (barcode[0])
		vtl_tape_set_barcode(tape, barcode);

	switch (type) {
	case 'S':
		ret = vtl_changer_slot_place(ch, addr, tape);
		if (ret < 0) {
			pr_warn("VTL: state restore: slot_place(%d) '%s': %d (tape kept in global table)\n",
				addr, name, ret);
		}
		break;

	case 'D':
		empty_slot = find_first_empty_slot(ch);
		if (empty_slot < 0) {
			pr_warn("VTL: state restore: no empty slot to stage drive load '%s' (tape kept in global table)\n",
				name);
			break;
		}
		ret = vtl_changer_slot_place(ch, empty_slot, tape);
		if (ret < 0) {
			pr_warn("VTL: state restore: preload slot_place(%d) '%s': %d (tape kept in global table)\n",
				empty_slot, name, ret);
			break;
		}
		ret = vtl_changer_move_medium(ch, empty_slot, vtl_elem_drive_base(ch) + addr);
		if (ret < 0) {
			pr_warn("VTL: state restore: move to drive %d '%s': %d (tape left in slot %d)\n",
				addr, name, ret, empty_slot);
		}
		break;

	case 'I':
		empty_slot = find_first_empty_slot(ch);
		if (empty_slot < 0) {
			pr_warn("VTL: state restore: no empty slot to stage mailslot load '%s' (tape kept in global table)\n",
				name);
			break;
		}
		ret = vtl_changer_slot_place(ch, empty_slot, tape);
		if (ret < 0) {
			pr_warn("VTL: state restore: preload slot_place(%d) '%s': %d (tape kept in global table)\n",
				empty_slot, name, ret);
			break;
		}
		ret = vtl_changer_move_medium(ch, empty_slot, vtl_elem_ie_base(ch) + addr);
		if (ret < 0) {
			pr_warn("VTL: state restore: move to mailslot %d '%s': %d (tape left in slot %d)\n",
				addr, name, ret, empty_slot);
		}
		break;

	default:
		pr_warn("VTL: state restore: unknown element type '%c' for '%s' (tape kept in global table)\n",
			type, name);
		break;
	}

	vtl_tape_put(tape);
	return 1;
}

/* Read and apply state file for a single changer instance. */
static void vtl_restore_one_changer_state(struct vtl_changer *ch, int instance)
{
	char path[256];
	struct file *filp;
	char *buf;
	loff_t pos = 0;
	ssize_t n;
	char *line, *next;
	int count = 0;

	snprintf(path, sizeof(path), "%s/changer-%d.state", vtl_state_dir, instance);

	filp = filp_open(path, O_RDONLY, 0);
	if (IS_ERR(filp)) {
		if (PTR_ERR(filp) != -ENOENT)
			pr_warn("VTL: state restore: open '%s' failed: %ld\n", path, PTR_ERR(filp));
		return;
	}

	buf = kvzalloc(VTL_STATE_BUF_MAX, GFP_KERNEL);
	if (!buf) {
		pr_warn("VTL: state restore: OOM allocating read buffer for '%s'\n", path);
		filp_close(filp, NULL);
		return;
	}

	n = kernel_read(filp, buf, VTL_STATE_BUF_MAX - 1, &pos);
	filp_close(filp, NULL);

	if (n < 0) {
		pr_warn("VTL: state restore: read '%s' failed: %zd\n", path, n);
		kvfree(buf);
		return;
	}
	buf[n] = '\0';

	/* Parse line by line */
	line = buf;
	while (line && *line) {
		next = strchr(line, '\n');
		if (next) {
			*next = '\0';
			/* Strip trailing CR (Windows/dos2unix safety) */
			if (next > line && *(next - 1) == '\r')
				*(next - 1) = '\0';
			next++;
		}
		if (apply_one_state_line(ch, line) > 0)
			count++;
		line = next;
	}

	kvfree(buf);

	if (count > 0)
		pr_info("VTL: state restore: instance %d — %d tape(s) loaded from %s\n",
			instance, count, path);
}

/*
 * Restore changer occupancy from state files written by vtladm.
 * Best-effort — failures never block module load.
 * Called from vtl_init() after changers exist but before SCSI scan.
 */
void vtl_restore_all_changer_states(void)
{
	int instance;
	struct vtl_changer *ch;
	int restored = 0;

	if (!vtl_state_restore) {
		pr_info("VTL: state restore disabled (state_restore=N)\n");
		return;
	}

	/*
	 * Changers for all instances exist at this point (vtl_register_all_pdevs
	 * has completed). vtl_changer_get_instance walks vtl_host_list under
	 * vtl_list_lock, which is safe because we're in module_init serial context.
	 */
	for (instance = 0; ; instance++) {
		ch = vtl_changer_get_instance(instance);
		if (!ch)
			break;
		vtl_restore_one_changer_state(ch, instance);
		restored++;
	}

	if (restored > 0)
		pr_info("VTL: state restore finished — checked %d instance(s)\n", restored);
}
