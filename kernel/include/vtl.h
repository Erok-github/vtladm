#ifndef _VTL_H
#define _VTL_H

#include <linux/module.h>
#include <linux/kernel.h>
#include <linux/init.h>
#include <linux/slab.h>
#include <linux/spinlock.h>
#include <linux/mutex.h>
#include <linux/rwsem.h>
#include <linux/kref.h>
#include <linux/list.h>
#include <linux/workqueue.h>
#include <linux/delay.h>
#include <linux/blkdev.h>
#include <linux/blk-mq.h>
#include <linux/version.h>

#if LINUX_VERSION_CODE < KERNEL_VERSION(4, 18, 0)
#error "VTL requires Linux kernel 4.18 or later (this tree is too old)"
#endif

#include <scsi/scsi.h>
#include <scsi/scsi_host.h>
#include <scsi/scsi_cmnd.h>
#include <scsi/scsi_device.h>
#include <scsi/scsi_tcq.h>

/*
 * Linux 4.18–6.4: scsi_cmnd has ->scsi_done callback.
 * Linux 6.5+ removes that member; LLDDs must call scsi_done(cmd).
 */
#if LINUX_VERSION_CODE >= KERNEL_VERSION(6, 5, 0)
#define vtl_scsi_done(cmd) scsi_done(cmd)
#else
#define vtl_scsi_done(cmd) ((cmd)->scsi_done(cmd))
#endif

/*
 * Big-endian helpers — avoid <linux/unaligned.h>: some vendor kernel-devel
 * packages (e.g. certain Kylin trees) omit that header while still on 4.19.
 */
static inline u32 vtl_get_be32(const u8 *p)
{
    return ((u32)p[0] << 24) | ((u32)p[1] << 16) | ((u32)p[2] << 8) | (u32)p[3];
}

static inline u64 vtl_get_be64(const u8 *p)
{
    return ((u64)p[0] << 56) | ((u64)p[1] << 48) | ((u64)p[2] << 40) | ((u64)p[3] << 32) |
           ((u64)p[4] << 24) | ((u64)p[5] << 16) | ((u64)p[6] << 8)  | (u64)p[7];
}

static inline void vtl_put_be32(u32 v, u8 *p)
{
    p[0] = (u8)(v >> 24);
    p[1] = (u8)(v >> 16);
    p[2] = (u8)(v >> 8);
    p[3] = (u8)v;
}

static inline void vtl_put_be16(u16 v, u8 *p)
{
    p[0] = (u8)(v >> 8);
    p[1] = (u8)v;
}

static inline void vtl_put_be64(u64 v, u8 *p)
{
    p[0] = (u8)(v >> 56);
    p[1] = (u8)(v >> 48);
    p[2] = (u8)(v >> 40);
    p[3] = (u8)(v >> 32);
    p[4] = (u8)(v >> 24);
    p[5] = (u8)(v >> 16);
    p[6] = (u8)(v >> 8);
    p[7] = (u8)v;
}

#define VTL_VERSION "1.0.0"
#define VTL_NAME "vtl"

#define VTL_MAX_DRIVES 8
#define VTL_MAX_SLOTS 256
#define VTL_MAX_MAILSLOTS 4
/** Max medium-changer + tape host groups (one platform device each). */
#define VTL_MAX_SCSI_INSTANCES 8
/*
 * Product caps (Plan B / vtladm): up to 8 libraries (SCSI hosts), each with
 * one medium changer (LUN 0), up to 8 tape drives and 256 data slots.
 */

#define VTL_DEFAULT_DRIVES 1
#define VTL_DEFAULT_SLOTS 10

/** Max bytes for `vtl_instances` spec passed via `VTL_IOCTL_SET_INSTANCES` (incl. NUL). */
#define VTL_INST_SPEC_MAX 384

#define VTL_MIN_BLOCK_SIZE 512
#define VTL_MAX_BLOCK_SIZE (1024 * 1024)
#define VTL_DEFAULT_BLOCK_SIZE 0 /* variable — mhVTL hybrid: SCSI var, I/O fixed */
#define VTL_DEFAULT_DENSITY 0x40 /* Default LTO (Ultrium) */

/* Record-block format (mhVTL-aligned): multiple SCSI writes packed into 64KB I/O blocks.
 * Block layout: [magic:u32 "VLBK"][num_recs:u16][flags:u16][packed records...]
 * Each record:   [size:u32][data:size]  */
#define VTL_REC_BLOCK_MAGIC 0x564C424B /* "VLBK" */
#define VTL_REC_BUF_SIZE    65536      /* 64 KB I/O block */
#define VTL_REC_HEADER_SIZE 8          /* magic(4)+num(2)+flags(2) */

/* T10 SCSI density codes for sequential-access devices (SSC-5) */
#define VTL_DENSITY_LTO5     0x4A
#define VTL_DENSITY_LTO6     0x4C
#define VTL_DENSITY_LTO7     0x4E
#define VTL_DENSITY_LTO8     0x50
#define VTL_DENSITY_LTO9     0x52
#define VTL_DENSITY_LTO10    0x58

/* Compression algorithm identifiers */
#define VTL_COMP_NONE   0
#define VTL_COMP_ZLIB   1
#define VTL_COMP_LZO    2

/* VTLMETA flags byte bit definitions (byte 7 of sidecar) */
#define VTL_META_FLAG_COMPRESSED  0x01
#define VTL_META_FLAG_ALGO_MASK   0x06
#define VTL_META_FLAG_ALGO_SHIFT  1

/* Block header magic "VTLB" and size */
#define VTL_BLOCK_MAGIC      0x564C5442
#define VTL_BLOCK_HEADER_SIZE 16

struct vtl_block_header {
	__be32 magic;
	__be32 uncompressed_size;
	__be32 compressed_size;
	u8     algorithm;
	u8     reserved[3];
} __packed;

/*
 * Filemark sidecar (.vtlfm): stores filemark offsets for SPACE / READ POSITION.
 * Separate from the tape-data format.
 */
#define VTL_FM_MAGIC   0x564C5446  /* "VTLF" */
#define VTL_FM_VERSION 1
struct vtl_fm_header {
	__be32 magic;
	__be32 version;
	__be32 num_filemarks;
	__be32 reserved;
	/* Followed by: __be64 filemark_offsets[num_filemarks] */
} __packed;

#define VTL_MAX_FILEMARKS (4 * 1024 * 1024)

#define VTL_MIN_TAPE_SIZE (10 * 1024 * 1024)
#define VTL_MAX_TAPE_SIZE (10ULL * 1024 * 1024 * 1024 * 1024)

#define VTL_VENDOR_ID "VTL     "
/* 16-byte inquiry product: keep changer vs tape distinct for upper layers / devinfo */
#define VTL_PRODUCT_CHANGER "VTL CHANGER     "
#define VTL_PRODUCT_TAPE    "VTL TAPE DRV    "
#define VTL_REVISION "1.00"

struct vtl_changer; /* forward ref for element-address helpers below */

/** SMC-3 element type codes in READ ELEMENT STATUS pages (not the same as SCSI device types). */
#define VTL_SMC_ELEM_ST 0x02 /* storage */
#define VTL_SMC_ELEM_IE 0x03 /* import/export */
#define VTL_SMC_ELEM_DT 0x04 /* data transfer (tape drives) */

/* PVolTag format selection for READ ELEMENT STATUS */
#define VTL_CDB_PV_BIT       0x10 /* Primary Volume Tag bit in CDB byte 1 */
#define VTL_PVOLTAG_AUTO     0  /* Heuristic: mtx detection + Mars/Veritas workaround */
#define VTL_PVOLTAG_STANDARD 1  /* SMC-3 standard: 4-byte PVolTag header at bytes 12-15 */
#define VTL_PVOLTAG_MTX      2  /* mtx-compatible: barcode directly at byte 12 */
extern int vtl_pvoltag_format;

struct vtl_sense_data {
    u8 key;
    u8 asc;
    u8 ascq;
};


struct vtl_tape_metadata {
    char serial[32];
    char barcode[16];
    u64 capacity;
    u64 used;
    u32 block_size;
    time64_t created;
    time64_t accessed;
    u32 num_snapshots;
    /* Host-side usage counters (SCSI LOG SENSE / health); not persisted to image header today */
    u64 log_bytes_read;
    u64 log_bytes_written;
    u32 mount_count;
    u8 density;       /* T10 density code from sidecar metadata */
    u8 meta_flags;    /* flags byte from sidecar (compression, algorithm) */
};

struct vtl_tape {
    char name[64];
    char path[256];
    struct vtl_tape_metadata meta;
    struct file *file;
    loff_t position;
    bool loaded;
    bool write_protected;

    /* Filemark offset persistence (.vtlfm sidecar) */
    u64  *filemark_offsets;   /* byte offsets, dynamically allocated */
    u32   num_filemarks;
    u32   filemark_capacity;
    bool  meta_dirty;

    struct mutex lock;
    struct kref ref;
};

struct vtl_drive {
    int id;
    char name[32];
    struct vtl_tape *loaded_tape;
    int source_slot; /* SCSI address of the slot the tape was loaded from, -1 if none */
    struct scsi_device *sdev;
    struct request_queue *queue;
    /* Reserved for future blk layer integration; not used by current LLD. */
    struct blk_mq_tag_set tag_set;
    u32 block_size;
    u8 density;
    bool at_filemark;
    bool at_end;
    bool at_bot;
    bool compression_enabled;  /* runtime compression on/off (from VTLMETA / MODE SELECT) */
    u8   compression_algorithm; /* VTL_COMP_ZLIB or VTL_COMP_LZO */
    u64  comp_bytes_written;   /* uncompressed bytes written (LOG SENSE stats) */
    u64  comp_bytes_read;      /* uncompressed bytes read    (LOG SENSE stats) */
    /* Unit Attention: set on media-change / reset, cleared by REQUEST SENSE */
    bool ua_pending;
    u8   ua_asc;
    u8   ua_ascq;

    /* mhVTL-style record buffer: each SCSI WRITE becomes a record,
     * packed into 64KB blocks for efficient disk I/O. */
    u8  *rec_buf;       /* 64KB record accumulator */
    u32  rec_buf_used;  /* bytes used in buffer */
    u16  rec_count;     /* records in current block (write side) */
    /* Read side: track position within a multi-record block */
    u16  rec_read_idx;   /* next record index to read (0-based) */
    u16  rec_read_total; /* total records in current block */
    loff_t rec_block_start; /* file offset where current block data begins */

    struct vtl_sense_data sense;
    struct mutex lock;
    struct work_struct work;
};

#define VTL_EARLY_WARN_MARGIN (512ULL * 1024 * 1024)  /* 512 MiB */

struct vtl_slot {
    int id;
    bool occupied;
    struct vtl_tape *tape;
};

struct vtl_changer {
    int id;
    char name[32];
    struct scsi_device *sdev;
    int num_drives;
    int num_slots;
    int num_mailslots;
    struct vtl_drive drives[VTL_MAX_DRIVES];
    struct vtl_slot slots[VTL_MAX_SLOTS];
    struct vtl_slot mailslots[VTL_MAX_MAILSLOTS];
    struct vtl_sense_data sense;
    struct mutex lock;
};

/** SCSI element addresses: sequential SMC-3 conventional layout.
 *  Storage: 1..num_slots
 *  Data transfer (drives): num_slots+1 .. num_slots+num_drives
 *  Import/export: num_slots+num_drives+1 .. num_slots+num_drives+num_mailslots
 */
static inline int vtl_elem_drive_base(const struct vtl_changer *ch)
{
    return ch->num_slots + 1;
}

static inline int vtl_elem_ie_base(const struct vtl_changer *ch)
{
    return ch->num_slots + ch->num_drives + 1;
}

static inline int vtl_elem_is_storage(const struct vtl_changer *ch, int addr)
{
    /* Accept 0 as alias for address 1 (some backup software uses 0-based
     * type-relative addressing). */
    if (addr == 0 && ch->num_slots > 0)
        return 1;
    return addr >= 1 && addr <= ch->num_slots;
}

static inline int vtl_elem_is_drive(const struct vtl_changer *ch, int addr)
{
    int base = vtl_elem_drive_base(ch);
    int end  = base + ch->num_drives;
    if (addr == 0 && ch->num_drives > 0)
        return 1;
    return addr >= base && addr < end;
}

static inline int vtl_elem_is_ie(const struct vtl_changer *ch, int addr)
{
    int base = vtl_elem_ie_base(ch);
    int end  = base + ch->num_mailslots;
    if (addr == 0 && ch->num_mailslots > 0)
        return 1;
    return addr >= base && addr < end;
}

/* 0-based slot index from storage element address (1-based). */
static inline int vtl_elem_to_slot(const struct vtl_changer *ch, int addr)
{
    (void)ch;
    return addr - 1;
}

struct vtl_host {
    struct Scsi_Host *shost;
    struct vtl_changer *changer;
    /*
     * Protects changer and its embedded drives while queuecommand executes.
     * Hot reconfiguration/removal takes the write side before publishing NULL
     * and freeing the old changer.
     */
    struct rw_semaphore io_sem;
    struct list_head list;
    /** Back-pointer for deferred scsi_add_host (outside probe). */
    struct platform_device *pdev;
    /** True after deferred scsi_add_host succeeds. */
    bool scsi_registered;
    /** Deferred scsi_add_host on system_long_wq (probe must not call add_host). */
    struct delayed_work scan_work;
    /** After scsi_add_host: optional extra wait before scsi_scan_host (Kylin). */
    struct delayed_work post_add_scan_work;
    /** Set after scsi_scan_host (+ quiesce) completes or bringup aborted. */
    bool scan_done;
    /** Periodic restore of devices that st driver offlines during background probing. */
    struct delayed_work offline_guard_work;
};

int vtl_eh_abort(struct scsi_cmnd *cmd);
int vtl_eh_device_reset(struct scsi_cmnd *cmd);
int vtl_eh_host_reset(struct scsi_cmnd *cmd);

int vtl_scsi_queuecommand(struct Scsi_Host *shost, struct scsi_cmnd *cmd);
int vtl_slave_alloc(struct scsi_device *sdev);
void vtl_slave_destroy(struct scsi_device *sdev);
int vtl_slave_configure(struct scsi_device *sdev);
int vtl_change_queue_depth(struct scsi_device *sdev, int depth);

int vtl_tape_create(const char *name, u64 size, u8 density, u8 flags);

/* Sidecar metadata (VTLMETA) I/O */
int vtl_meta_write(const char *tape_path, u8 density, u8 flags, u64 used);
int vtl_meta_read(const char *tape_path, u8 *density_out, u8 *flags_out, u64 *used_out);

/* Compression engine (compression.c) */
void vtl_block_header_fill(struct vtl_block_header *hdr, u32 uncomp_sz, u32 comp_sz, u8 algo);
int vtl_compress_block(const u8 *in, u32 in_len, u8 *out, u32 *out_len, u8 algo);
int vtl_decompress_block(const u8 *in, u32 in_len, u8 *out, u32 *out_len);
void vtl_format_meta_path(char *buf, size_t len, const char *tape_path);
struct vtl_tape *vtl_tape_find_by_name(const char *name);
struct vtl_tape *vtl_tape_open_existing(const char *name);
void vtl_tape_set_barcode(struct vtl_tape *tape, const char *barcode);
int vtl_changer_slot_place(struct vtl_changer *ch, int slot, struct vtl_tape *tape);
int vtl_changer_load_slot_to_drive(struct vtl_changer *ch, int slot, int drive,
				   const char *tape_name, const char *barcode);
int vtl_changer_unload_drive_to_slot(struct vtl_changer *ch, int drive, int slot);
struct vtl_changer *vtl_changer_get_instance(int instance);
int vtl_tape_load(struct vtl_drive *drive, struct vtl_tape *tape);
int vtl_tape_unload(struct vtl_drive *drive);
int vtl_tape_read(struct vtl_drive *drive, u8 *buffer, u32 len, u32 *actual);
int vtl_tape_write(struct vtl_drive *drive, const u8 *buffer, u32 len, u32 *actual);
int vtl_tape_space(struct vtl_drive *drive, int code, int count);
int vtl_tape_write_filemarks(struct vtl_drive *drive, int count);
int vtl_tape_rewind(struct vtl_drive *drive);
int vtl_rec_flush(struct vtl_drive *drv, struct vtl_tape *tape);

void vtl_changer_clear_media(struct vtl_changer *ch);
void vtl_tapes_release_all(void);
/** Drop one reference; frees tape when last ref (module unload / table remove). */
void vtl_tape_put(struct vtl_tape *tape);

/* Filemark offset persistence (.vtlfm sidecar) */
int  vtl_tape_load_metadata(struct vtl_tape *tape, bool *was_loaded);
int  vtl_tape_save_metadata(struct vtl_tape *tape);
void vtl_tape_free_metadata(struct vtl_tape *tape);

int vtl_changer_move_medium(struct vtl_changer *ch, int src, int dst);
/** Remove medium from changer element without placing elsewhere (shelf / off-line). */
int vtl_changer_remove_medium(struct vtl_changer *ch, int elem);
int vtl_changer_exchange_medium(struct vtl_changer *ch, int src1, int src2, int dst);
int vtl_changer_read_element_status(struct vtl_changer *ch, u8 *buffer, u32 len,
				    bool voltag, bool voltag_std, u8 elem_type,
				    int start_elem, int num_elems);
int vtl_changer_collect_inventory(struct vtl_changer *ch, int *num_drives,
				  int *num_slots, int *num_mailslots,
				  int *count, int *truncated, int elements[],
				  char names[][64], int max_items);

void vtl_set_sense(struct vtl_sense_data *sense, u8 key, u8 asc, u8 ascq);
void vtl_build_sense_buffer(struct scsi_cmnd *cmd, struct vtl_sense_data *sense);

int vtl_sysfs_init(void);
void vtl_sysfs_exit(void);

int vtl_misc_init(void);
void vtl_misc_exit(void);

/**
 * Rebuild SCSI hosts from a `vtl_instances`-format string (e.g. "2x32,1x10") without
 * unloading the module. Requires CAP_SYS_ADMIN and module_param allow_hot_geom=Y (default N).
 * When allow_hot_geom=N returns -EBUSY; prefer insmod vtl_instances= or full module reload.
 */
int vtl_apply_instances_spec(const char *spec);

/**
 * Plan B: adjust per-host drives/slots without platform_device teardown.
 * @spec must have the same segment count as live vtl_ninstances; returns -EINVAL if not.
 * Refuses shrink while tapes remain in removed drives/slots (-EBUSY).
 */
int vtl_apply_geom_resize_only(const char *spec);

/** True during SET_INSTANCES teardown or module unload (SCSI queuecommand should fail fast). */
bool vtl_reconfig_in_progress(void);

/** True while `rmmod` is tearing the module down (deferred bringup/scan must not run). */
bool vtl_module_is_unloading(void);

/* Optional delay after each successful medium move (ms); module_param in vtl_main.c */
extern int vtl_move_delay_ms;

/** Directory for `name.vtltape` files; module_param `tape_dir` in vtl_main.c */
extern char *vtl_tape_dir;

/** Restore changer occupancy from state files written by vtladm (mhVTL-style, init only). */
void vtl_restore_all_changer_states(void);

#endif
