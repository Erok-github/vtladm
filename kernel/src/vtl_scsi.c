#include "../include/vtl.h"
#include "../include/vtl_personality.h"
#include <linux/mm.h>
#include <linux/vmalloc.h>

/* SCSI opcodes (avoid header drift across 4.18–6.x) */
#ifndef LOG_SENSE
#define LOG_SENSE 0x4d
#endif
#ifndef READ_POSITION
#define READ_POSITION 0x34
#endif
#ifndef PREVENT_ALLOW_MEDIUM_REMOVAL
#define PREVENT_ALLOW_MEDIUM_REMOVAL 0x1e
#endif
#ifndef READ_12
#define READ_12 0xa8
#endif
#ifndef WRITE_12
#define WRITE_12 0xaa
#endif
/* SSC tape opcodes (not always in scsi.h for out-of-tree / older trees) */
#ifndef REWIND
#define REWIND 0x01
#endif
#ifndef LOAD_UNLOAD
#define LOAD_UNLOAD 0x1b
#endif
#ifndef REPORT_LUNS
#define REPORT_LUNS 0xa0
#endif
#ifndef INITIALIZE_ELEMENT_STATUS
#define INITIALIZE_ELEMENT_STATUS 0x07
#endif
#ifndef REPORT_DENSITY_SUPPORT
#define REPORT_DENSITY_SUPPORT 0x44
#endif
#ifndef DRIVER_SENSE
#define DRIVER_SENSE 0x08
#endif
#ifndef SERVICE_ACTION_IN
#define SERVICE_ACTION_IN 0x9e
#endif
#ifndef READ_CAPACITY
#define READ_CAPACITY 0x25
#endif
#ifndef SERVICE_ACTION_READ_CAPACITY_16
#define SERVICE_ACTION_READ_CAPACITY_16 0x10
#endif
#ifndef SYNCHRONIZE_CACHE
#define SYNCHRONIZE_CACHE 0x35
#endif
#ifndef ERASE
#define ERASE 0x19
#endif
#ifndef ALLOW_OVERWRITE
#define ALLOW_OVERWRITE 0x1c
#endif
#ifndef VERIFY_6
#define VERIFY_6 0x13
#endif
#ifndef POSITION_TO_ELEMENT
#define POSITION_TO_ELEMENT 0x2b
#endif

/*
 * Compose cmd->result for SG_IO / sg3_utils: status in bits 0..7 is SAM status << 1;
 * CHECK CONDITION also needs DRIVER_SENSE in bits 8..15. Raw SAM_STAT_* alone breaks
 * sg_turs ("bad pass-through setup") even when the command logic is correct.
 */
static void vtl_set_cmd_result(struct scsi_cmnd *cmd, int sam_status)
{
    cmd->result = DID_OK << 16;
    if (sam_status == SAM_STAT_CHECK_CONDITION)
        cmd->result |= (DRIVER_SENSE << 8) | (SAM_STAT_CHECK_CONDITION << 1);
    else if (sam_status != SAM_STAT_GOOD)
        cmd->result |= (sam_status << 1);
}

/* Max single READ/WRITE buffer (matches VTL_SCSI_RW_CAP_BYTES below) */
#define VTL_XFER_BUF_MAX (64U * 1024U * 1024U)
#define VTL_XFER_KMALLOC_MAX PAGE_SIZE
#define VTL_ELEMENT_STATUS_BUFLEN 8192U

static void *vtl_xfer_buf_alloc(unsigned int len)
{
    if (len == 0 || len > VTL_XFER_BUF_MAX)
        return NULL;
    if (len <= VTL_XFER_KMALLOC_MAX)
        return kmalloc(len, GFP_KERNEL | __GFP_NOWARN);
    return vmalloc(len);
}

static void vtl_xfer_buf_free(void *p)
{
    if (!p)
        return;
    if (is_vmalloc_addr(p))
        vfree(p);
    else
        kfree(p);
}

void vtl_set_sense(struct vtl_sense_data *sense, u8 key, u8 asc, u8 ascq)
{
    sense->key = key;
    sense->asc = asc;
    sense->ascq = ascq;
}

void vtl_build_sense_buffer(struct scsi_cmnd *cmd, struct vtl_sense_data *sense)
{
    u8 *sb = cmd->sense_buffer;

    memset(sb, 0, SCSI_SENSE_BUFFERSIZE);
    sb[0] = 0x70;
    sb[2] = sense->key;
    sb[7] = 10;
    sb[12] = sense->asc;
    sb[13] = sense->ascq;
}

#ifndef ABORTED_COMMAND
#define ABORTED_COMMAND 0x0b
#endif
#ifndef HARDWARE_ERROR
#define HARDWARE_ERROR 0x04
#endif
#ifndef VOLUME_OVERFLOW
#define VOLUME_OVERFLOW 0x0d
#endif

static bool vtl_drive_has_tape(struct vtl_drive *drv)
{
    bool loaded;

    mutex_lock(&drv->lock);
    loaded = drv->loaded_tape != NULL;
    mutex_unlock(&drv->lock);
    return loaded;
}

static bool vtl_drive_write_protected(struct vtl_drive *drv)
{
    bool wp;

    mutex_lock(&drv->lock);
    wp = drv->loaded_tape && drv->loaded_tape->write_protected;
    mutex_unlock(&drv->lock);
    return wp;
}

/* Linear staging buffer → initiator scatter-gather (returns 0 or -EIO). */
static int vtl_scsi_copy_to_sg(struct scsi_cmnd *cmd, void *buf, unsigned int len,
                               struct vtl_sense_data *sense)
{
    int copied;

    if (len == 0)
        return 0;

    copied = scsi_sg_copy_from_buffer(cmd, buf, (int)len);
    if (unlikely(copied < 0)) {
        vtl_set_sense(sense, ABORTED_COMMAND, 0x00, 0x00);
        vtl_build_sense_buffer(cmd, sense);
        return -EIO;
    }
    if (unlikely((unsigned int)copied < len))
        scsi_set_resid(cmd, len - (unsigned int)copied);
    return 0;
}

/* Initiator scatter-gather → linear staging buffer (returns 0 or -EIO). */
static int vtl_scsi_copy_from_sg(struct scsi_cmnd *cmd, void *buf, unsigned int len,
                                 struct vtl_sense_data *sense)
{
    int copied;

    if (len == 0)
        return 0;

    copied = scsi_sg_copy_to_buffer(cmd, buf, (int)len);
    if (unlikely(copied < 0)) {
        vtl_set_sense(sense, ABORTED_COMMAND, 0x00, 0x00);
        vtl_build_sense_buffer(cmd, sense);
        return -EIO;
    }
    if (unlikely((unsigned int)copied < len)) {
        vtl_set_sense(sense, ABORTED_COMMAND, 0x00, 0x00);
        vtl_build_sense_buffer(cmd, sense);
        return -EIO;
    }
    return 0;
}

static void vtl_scsi_staging_oom(struct scsi_cmnd *cmd, struct vtl_sense_data *sense)
{
    vtl_set_sense(sense, HARDWARE_ERROR, 0x00, 0x00);
    vtl_build_sense_buffer(cmd, sense);
}

static int vtl_cmd_illegal(struct scsi_cmnd *cmd, struct vtl_sense_data *sense)
{
    vtl_set_sense(sense, ILLEGAL_REQUEST, 0x20, 0);
    vtl_build_sense_buffer(cmd, sense);
    return SAM_STAT_CHECK_CONDITION;
}

/* ILLEGAL REQUEST / ASC 0x25 ASCQ 0x00 — logical unit not supported */
static int vtl_cmd_lun_not_supported(struct scsi_cmnd *cmd, struct vtl_changer *ch)
{
    vtl_set_sense(&ch->sense, ILLEGAL_REQUEST, 0x25, 0);
    vtl_build_sense_buffer(cmd, &ch->sense);
    return SAM_STAT_CHECK_CONDITION;
}

static struct vtl_sense_data *vtl_sense_ptr(struct scsi_cmnd *cmd, struct vtl_host *vhost)
{
    unsigned int lun = cmd->device->lun;
    struct vtl_changer *ch = vhost->changer;

    if (lun == 0)
        return &ch->sense;
    if (lun >= 1 && lun <= (unsigned int)ch->num_drives)
        return &ch->drives[lun - 1].sense;
    return &ch->sense;
}

static unsigned int vtl_inquiry_alloc_len(struct scsi_cmnd *cmd)
{
    const u8 *cdb = cmd->cmnd;

    if (cmd->cmd_len >= 10)
        return ((unsigned int)cdb[3] << 8) | cdb[4];
    return (unsigned int)cdb[4];
}

static int vtl_handle_inquiry_evpd(struct scsi_cmnd *cmd, struct vtl_host *vhost, u8 page)
{
    u8 *buf;
    unsigned int lun = cmd->device->lun;
    unsigned int alloc = vtl_inquiry_alloc_len(cmd);
    unsigned int out_len;
    unsigned int buflen = 512;
    u8 ptype = (lun == 0) ? 0x08 : 0x01;
    char id8[9];

    buf = vtl_xfer_buf_alloc(buflen);
    if (!buf) {
        vtl_scsi_staging_oom(cmd, vtl_sense_ptr(cmd, vhost));
        return SAM_STAT_CHECK_CONDITION;
    }

    memset(buf, 0, buflen);
    buf[0] = ptype;
    buf[1] = page;

    switch (page) {
    case 0x00:
        buf[3] = 3;
        buf[4] = 0x00;
        buf[5] = 0x80;
        buf[6] = 0x83;
        out_len = 7;
        break;
    case 0x80: {
        /*
         * Unit Serial Number.  For tape drives, report the loaded tape's
         * barcode so backup software sees a unique serial per tape.
         * For the changer or empty drives, use host+LUN as fallback.
         */
        struct Scsi_Host *shost = vhost->shost;
        char serial[32];

        if (lun >= 1 && lun <= (unsigned int)vhost->changer->num_drives) {
            struct vtl_drive *drv = &vhost->changer->drives[lun - 1];
            mutex_lock(&drv->lock);
            if (drv->loaded_tape && drv->loaded_tape->meta.barcode[0])
                snprintf(serial, sizeof(serial), "%.16s",
                         drv->loaded_tape->meta.barcode);
            else
                snprintf(serial, sizeof(serial), "VTL%05uL%02u",
                         shost ? (unsigned int)shost->host_no : 0U, lun);
            mutex_unlock(&drv->lock);
        } else {
            snprintf(serial, sizeof(serial), "VTL%05uCHG",
                     shost ? (unsigned int)shost->host_no : 0U);
        }
        out_len = 4 + (unsigned int)strnlen(serial, 20U);
        if (out_len > buflen)
            out_len = buflen;
        buf[3] = (u8)(out_len - 4);
        memcpy(&buf[4], serial, out_len - 4);
        break;
    }
    case 0x83: {
        /*
         * Device Identification VPD: one vendor-specific descriptor (SPC).
         * 8-byte identifier encodes LUN so multipath tools can distinguish LUNs.
         */
        buf[3] = 12;
        buf[4] = 0x01; /* PI=0, code set = binary */
        buf[5] = 0x00; /* designator type 0 = vendor-specific */
        buf[6] = 0x00;
        buf[7] = 8;
        snprintf(id8, sizeof(id8), "VTL%05u", lun);
        memcpy(&buf[8], id8, 8);
        out_len = 4 + 12;
        break;
    }
    default:
        vtl_xfer_buf_free(buf);
        return vtl_cmd_illegal(cmd, vtl_sense_ptr(cmd, vhost));
    }

    alloc = min_t(unsigned int, alloc, buflen);
    if (vtl_scsi_copy_to_sg(cmd, buf, min_t(unsigned int, alloc, out_len), vtl_sense_ptr(cmd, vhost))) {
        vtl_xfer_buf_free(buf);
        return SAM_STAT_CHECK_CONDITION;
    }
    vtl_xfer_buf_free(buf);
    return SAM_STAT_GOOD;
}

static int vtl_handle_inquiry(struct scsi_cmnd *cmd, struct vtl_host *vhost)
{
    u8 *cdb = cmd->cmnd;
    u8 *buffer;
    unsigned int len;
    unsigned int alloc;
    unsigned int lun = cmd->device->lun;

    if (cdb[1] & 0x02)
        return vtl_cmd_illegal(cmd, vtl_sense_ptr(cmd, vhost));
    if (cdb[1] & 0x01)
        return vtl_handle_inquiry_evpd(cmd, vhost, cdb[2]);

    buffer = vtl_xfer_buf_alloc(252);
    if (!buffer) {
        vtl_scsi_staging_oom(cmd, vtl_sense_ptr(cmd, vhost));
        return SAM_STAT_CHECK_CONDITION;
    }

    memset(buffer, 0, 252);
    /* Peripheral qualifier 000b, device type */
    if (lun == 0)
        buffer[0] = 0x08; /* Medium changer */
    else
        buffer[0] = 0x01; /* Sequential-access */
    buffer[1] = 0x80; /* Removable */
    buffer[2] = 0x06; /* SPC-4 version (backup apps expect >= SPC-3) */
    buffer[3] = 0x02; /* Response Data Format = 2 */
    buffer[4] = 0x1f;
    buffer[5] = 0;
    buffer[6] = 0;
    buffer[7] = 0;
    {
        const struct vtl_personality_desc *pers =
            vtl_personality_lookup(vtl_personality_active_id());

        memcpy(&buffer[8], pers->vendor, 8);
        if (lun == 0)
            memcpy(&buffer[16], pers->product_changer, 16);
        else
            memcpy(&buffer[16], pers->product_tape, 16);
        memcpy(&buffer[32], pers->revision, 4);
    }

    alloc = vtl_inquiry_alloc_len(cmd);
    len = min_t(unsigned int, alloc, 252U);
    if (vtl_scsi_copy_to_sg(cmd, buffer, len, vtl_sense_ptr(cmd, vhost))) {
        vtl_xfer_buf_free(buffer);
        return SAM_STAT_CHECK_CONDITION;
    }
    vtl_xfer_buf_free(buffer);

    return SAM_STAT_GOOD;
}

static int vtl_handle_test_unit_ready(struct scsi_cmnd *cmd, struct vtl_host *vhost)
{
    unsigned int lun = cmd->device->lun;
    struct vtl_changer *ch = vhost->changer;

    if (lun == 0)
        return SAM_STAT_GOOD;

    if (lun < 1 || lun > (unsigned int)ch->num_drives)
        return vtl_cmd_lun_not_supported(cmd, ch);

    /*
     * Always report ready for virtual tape drives.
     * Returning NOT_READY when no tape is loaded is correct per SSC,
     * but Kylin 4.19's st driver offlines the device after repeated
     * NOT_READY responses ("Device offlined - not ready after error
     * recovery"), permanently blocking all SG_IO passthrough (ENXIO).
     *
     * READ/WRITE handlers still enforce loaded_tape checks, so backup
     * apps get proper errors when accessing an empty drive.
     */
    return SAM_STAT_GOOD;
}

static int vtl_handle_request_sense(struct scsi_cmnd *cmd, struct vtl_host *vhost)
{
	u8 *cdb = cmd->cmnd;
	u8 *buffer;
	int len;
	struct vtl_sense_data *src;

	/* SPC-5: zero allocation length means no data transferred */
	if (cdb[4] == 0)
		return SAM_STAT_GOOD;

	buffer = vtl_xfer_buf_alloc(252);
	if (!buffer) {
		vtl_scsi_staging_oom(cmd, vtl_sense_ptr(cmd, vhost));
		return SAM_STAT_CHECK_CONDITION;
	}

	src = vtl_sense_ptr(cmd, vhost);
	vtl_build_sense_buffer(cmd, src);
	len = min_t(int, cdb[4], 252);
	len = min_t(int, len, 96);
	memcpy(buffer, cmd->sense_buffer, len);

	/* Clear persistent sense after reporting (SPC-5 section 5.9) */
	memset(src, 0, sizeof(*src));

	if (vtl_scsi_copy_to_sg(cmd, buffer, (unsigned int)len, vtl_sense_ptr(cmd, vhost))) {
		vtl_xfer_buf_free(buffer);
		return SAM_STAT_CHECK_CONDITION;
	}
	vtl_xfer_buf_free(buffer);

	return SAM_STAT_GOOD;
}

static int vtl_handle_read_block_limits(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    u8 *buffer;

    buffer = vtl_xfer_buf_alloc(6);
    if (!buffer) {
        vtl_scsi_staging_oom(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    memset(buffer, 0, 6);
    buffer[1] = (VTL_MAX_BLOCK_SIZE >> 16) & 0xff;
    buffer[2] = (VTL_MAX_BLOCK_SIZE >> 8) & 0xff;
    buffer[3] = VTL_MAX_BLOCK_SIZE & 0xff;
    buffer[4] = (VTL_MIN_BLOCK_SIZE >> 8) & 0xff;
    buffer[5] = (VTL_MIN_BLOCK_SIZE >> 0) & 0xff;

    if (vtl_scsi_copy_to_sg(cmd, buffer, 6, &drv->sense)) {
        vtl_xfer_buf_free(buffer);
        return SAM_STAT_CHECK_CONDITION;
    }
    vtl_xfer_buf_free(buffer);

    return SAM_STAT_GOOD;
}

/*
 * Medium-changer MODE SENSE pages (mtx / backup apps probe 0x1D / 0x1E).
 * MODE SENSE(6): 4-byte header + page; MODE SENSE(10): 8-byte header + page.
 */
static unsigned int vtl_changer_mode_sense_fill(u8 *buffer, unsigned int buf_max,
						struct vtl_changer *ch, u8 page,
						bool sense10)
{
    unsigned int hdr = sense10 ? 8U : 4U;
    unsigned int off = hdr;
    u8 *pg;

    if (buf_max < hdr + 4)
        return hdr;

    memset(buffer, 0, buf_max);

    if (page == 0x00) {
        /*
         * Medium-changer supported mode pages (SMC-3): 0x00, 0x1D, 0x1E, 0x1F.
         * Page 0x01 (Read-Write Error Recovery) is a tape/SSC page and must NOT
         * appear in a medium changer's supported-pages list.
         */
        if (off + 6 > buf_max)
            return hdr;
        pg = &buffer[off];
        pg[0] = 0x00;
        pg[1] = 4;
        pg[2] = 0x00;
        pg[3] = 0x1d;
        pg[4] = 0x1e;
        pg[5] = 0x1f;
        off += 6;
    } else if (page == 0x1d) {
        /*
         * SMC-3 Element Address Assignment (page 0x1D), 16-byte parameter list:
         *  2-3  first medium transport, 4-5  #MT
         *  6-7  first storage,         8-9  #storage
         * 10-11 first I/E,            12-13 #I/E
         * 14-15 first data transfer,  16-17 #drives
         * (Legacy code wrongly put #slots at 4-5 and drive base at 6-7, so
         * initiators saw only two storage elements at 1000-1001.)
         */
        if (off + 18 > buf_max)
            return hdr;
        pg = &buffer[off];
        pg[0] = 0x1d;
        pg[1] = 16;
        vtl_put_be16(0, &pg[2]);
        vtl_put_be16(0, &pg[4]);
        vtl_put_be16(1, &pg[6]);
        vtl_put_be16(ch->num_slots, &pg[8]);
        vtl_put_be16(vtl_elem_ie_base(ch), &pg[10]);
        vtl_put_be16(ch->num_mailslots, &pg[12]);
        vtl_put_be16(vtl_elem_drive_base(ch), &pg[14]);
        vtl_put_be16(ch->num_drives, &pg[16]);
        off += 18;
    } else if (page == 0x1e) {
        if (off + 8 > buf_max)
            return hdr;
        pg = &buffer[off];
        pg[0] = 0x1e;
        pg[1] = 6;
        pg[2] = 1;
        pg[3] = 0;
        pg[4] = 0;
        pg[5] = 0;
        pg[6] = 0;
        pg[7] = 0;
        off += 8;
    } else if (page == 0x1f) {
        /*
         * SMC-3 Device Capabilities (page 0x1F), 10-byte parameter list:
         * tells initiators which element types exist (STOR, DT, IE).
         */
        if (off + 12 > buf_max)
            return hdr;
        pg = &buffer[off];
        pg[0] = 0x1f;
        pg[1] = 10;
        pg[2] = 0;
        pg[3] = (ch->num_drives > 0) ? 0x80 : 0;   /* DTDE */
        pg[4] = 0;
        pg[5] = 0x00;  /* MTDE = 0 (no robotic picker) */
        pg[6] = 0;
        pg[7] = 0;
        pg[8] = 0;
        pg[9] = (ch->num_mailslots > 0) ? 0x80 : 0;  /* IEDE */
        pg[10] = 0;
        pg[11] = 0;
        off += 12;
    } else if (page == 0x3f) {
        /*
         * All mode pages: backup apps (Mars/TSM) often use 0x3F instead of
         * separate 0x1D probes; returning header-only broke inventory while mtx
         * still worked (mtx requests page 0x1D explicitly).
         * Returns pages 0x1D, 0x1E, 0x1F.
         */
        if (off + 38 > buf_max)
            return hdr;
        pg = &buffer[off];
        pg[0] = 0x1d;
        pg[1] = 16;
        vtl_put_be16(0, &pg[2]);
        vtl_put_be16(0, &pg[4]);
        vtl_put_be16(1, &pg[6]);
        vtl_put_be16(ch->num_slots, &pg[8]);
        vtl_put_be16(vtl_elem_ie_base(ch), &pg[10]);
        vtl_put_be16(ch->num_mailslots, &pg[12]);
        vtl_put_be16(vtl_elem_drive_base(ch), &pg[14]);
        vtl_put_be16(ch->num_drives, &pg[16]);
        off += 18;
        if (off + 8 <= buf_max) {
            pg = &buffer[off];
            pg[0] = 0x1e;
            pg[1] = 6;
            pg[2] = 1;
            pg[3] = 0;
            pg[4] = 0;
            pg[5] = 0;
            pg[6] = 0;
            pg[7] = 0;
            off += 8;
        }
        if (off + 12 <= buf_max) {
            pg = &buffer[off];
            pg[0] = 0x1f;
            pg[1] = 10;
            pg[2] = 0;
            pg[3] = (ch->num_drives > 0) ? 0x80 : 0;
            pg[4] = 0;
            pg[5] = 0;
            pg[6] = 0;
            pg[7] = 0;
            pg[8] = 0;
            pg[9] = (ch->num_mailslots > 0) ? 0x80 : 0;
            pg[10] = 0;
            pg[11] = 0;
            off += 12;
        }
    }

    if (sense10) {
        /*
         * MODE SENSE(10): bytes 0-1 = length of bytes 2..(n-1) per SAM-5.
         * (Legacy used plen+2 with plen=off-8, short by 4 bytes — initiators
         * truncated page 0x1D before drive base @1000 / #drives; backup inventory failed.)
         */
        unsigned int md_len = off - 2;

        buffer[0] = (md_len >> 8) & 0xff;
        buffer[1] = md_len & 0xff;
        buffer[2] = 0;
        buffer[3] = 0;
        return off;
    }

    buffer[0] = (off - 1) & 0xff;
    buffer[1] = 0;
    buffer[2] = 0;
    buffer[3] = 0;
    return off;
}

/*
 * Write MODE SENSE parameter header + optional block descriptor for tape LUN.
 * Returns number of header bytes written (hdr + bd).
 * On return buffer[0..ret-1] contains the header; pages go at buffer[ret].
 */
static int vtl_mode_tape_write_header(u8 *buf, bool sense10, bool dbd,
                                      u8 wp, u8 density, u32 block_len)
{
    if (sense10) {
        buf[2] = 0;
        buf[3] = wp;
        if (!dbd) {
            buf[7] = 8;
            buf[8] = density;
            buf[9] = 0; buf[10] = 0; buf[11] = 0;
            buf[12] = 0;
            buf[13] = (block_len >> 16) & 0xff;
            buf[14] = (block_len >> 8) & 0xff;
            buf[15] = (block_len >> 0) & 0xff;
            return 16;
        }
        buf[7] = 0;
        return 8;
    }
    /* MODE SENSE(6) */
    buf[2] = wp;
    if (!dbd) {
        buf[3] = 8;
        buf[4] = density;
        buf[5] = 0; buf[6] = 0; buf[7] = 0;
        buf[8] = 0;
        buf[9] = (block_len >> 16) & 0xff;
        buf[10] = (block_len >> 8) & 0xff;
        buf[11] = (block_len >> 0) & 0xff;
        return 12;
    }
    buf[3] = 0;
    return 4;
}

/* Mode page fillers: write page at buf; return bytes written (incl 2-byte header). */
static int vtl_mode_page_01(u8 *buf)
{
    buf[0] = 0x01; buf[1] = 0x0a;  /* Read-Write Error Recovery, 10 bytes */
    buf[2] = 0x80;  /* AWRE=1 */
    return 12;
}

static int vtl_mode_page_0f(u8 *buf, bool compression_enabled, u8 algorithm)
{
    buf[0] = 0x0f; buf[1] = 0x0e;  /* Data Compression, 14 bytes */
    buf[2] = (compression_enabled ? 0x02 : 0x00) | 0x01; /* DCE=1 if on, DCC=1 (capable) */
    buf[4] = algorithm; /* Data Compression Protocol: 1=zlib, 2=lzo */
    return 16;
}

static int vtl_mode_page_10(u8 *buf, u8 density)
{
    buf[0] = 0x10; buf[1] = 0x0e;  /* Device Configuration, 14 bytes */
    /* Buffered mode 1: drive has a buffer; prevents st direct I/O bypass */
    buf[2] = 0x01;
    buf[9] = density;
    buf[14] = 0x02;  /* CAP = supports CAP */
    return 16;
}

static int vtl_mode_page_0a(u8 *buf)
{
    buf[0] = 0x0a; buf[1] = 0x0a;  /* Control, 10 bytes */
    /* RAC=0, SWP=0, everything else reserved; backup apps probe this page */
    return 12;
}

static int vtl_mode_page_1c(u8 *buf)
{
    buf[0] = 0x1c; buf[1] = 0x0a;  /* Informational Exceptions Control, 10 bytes */
    buf[3] = 0x03;  /* MRIE = report on REQUEST SENSE */
    return 12;
}

/*
 * Page 0x3F (all pages) inner writer: appends all supported pages except 0x00.
 */
static int vtl_mode_page_3f(u8 *buf, u8 density, bool compression_enabled,
                            u8 algorithm)
{
    int off = 0;

    /* Supported pages list */
    buf[off++] = 0x00;
    buf[off++] = 6;
    buf[off++] = 0x00; buf[off++] = 0x01; buf[off++] = 0x0a;
    buf[off++] = 0x0f; buf[off++] = 0x10; buf[off++] = 0x1c;
    off += vtl_mode_page_01(buf + off);
    off += vtl_mode_page_0a(buf + off);
    off += vtl_mode_page_0f(buf + off, compression_enabled, algorithm);
    off += vtl_mode_page_10(buf + off, density);
    off += vtl_mode_page_1c(buf + off);
    return off;
}

static int vtl_handle_mode_sense(struct scsi_cmnd *cmd, struct vtl_host *vhost)
{
    u8 *cdb = cmd->cmnd;
    u8 *buffer;
    unsigned int alloc_len;
    unsigned int lun = cmd->device->lun;
    struct vtl_changer *ch = vhost->changer;
    struct vtl_drive *drv = NULL;
    u32 block_len;
    u8 wp;
    u8 density;
    unsigned int out_len;
    u8 page;

    if (lun >= 1 && lun <= (unsigned int)ch->num_drives)
        drv = &ch->drives[lun - 1];

    if (cdb[0] == MODE_SENSE)
        alloc_len = cdb[4];
    else
        alloc_len = (cdb[7] << 8) | cdb[8];

    buffer = vtl_xfer_buf_alloc(255);
    if (!buffer) {
        vtl_scsi_staging_oom(cmd, vtl_sense_ptr(cmd, vhost));
        return SAM_STAT_CHECK_CONDITION;
    }

    memset(buffer, 0, 255);

    if (lun == 0) {
        page = cdb[2] & 0x3f;
        out_len = vtl_changer_mode_sense_fill(buffer, 255, ch, page,
                        cdb[0] == MODE_SENSE_10);
    } else {
        bool sense10, dbd;
        int hdr, pg;

        density = drv ? drv->density : VTL_DEFAULT_DENSITY;
        block_len = drv ? drv->block_size : VTL_DEFAULT_BLOCK_SIZE;
        wp = (drv && vtl_drive_write_protected(drv)) ? 0x80 : 0;
        sense10 = (cdb[0] == MODE_SENSE_10);
        dbd = (cdb[1] & 0x08) != 0;
        page = cdb[2] & 0x3f;

        hdr = vtl_mode_tape_write_header(buffer, sense10, dbd, wp,
                          density, block_len);
        pg = 0;

        if (page == 0x00) {
            /* Supported pages: 0x00, 0x01, 0x0A, 0x0F, 0x10, 0x1C */
            buffer[hdr]     = 0x00; buffer[hdr + 1] = 6;
            buffer[hdr + 2] = 0x00; buffer[hdr + 3] = 0x01;
            buffer[hdr + 4] = 0x0a; buffer[hdr + 5] = 0x0f;
            buffer[hdr + 6] = 0x10; buffer[hdr + 7] = 0x1c;
            pg = 8;
        } else if (page == 0x01) {
            pg = vtl_mode_page_01(buffer + hdr);
        } else if (page == 0x0a) {
            pg = vtl_mode_page_0a(buffer + hdr);
        } else if (page == 0x0f) {
            bool ce = drv ? drv->compression_enabled : false;
            u8 algo = drv ? drv->compression_algorithm : VTL_COMP_NONE;
            pg = vtl_mode_page_0f(buffer + hdr, ce, algo);
        } else if (page == 0x10) {
            pg = vtl_mode_page_10(buffer + hdr, density);
        } else if (page == 0x1c) {
            pg = vtl_mode_page_1c(buffer + hdr);
        } else if (page == 0x3f) {
            bool ce = drv ? drv->compression_enabled : false;
            u8 algo = drv ? drv->compression_algorithm : VTL_COMP_NONE;
            pg = vtl_mode_page_3f(buffer + hdr, density, ce, algo);
        }
        /* Unknown pages: return just header + block descriptor (pg stays 0). */

        out_len = hdr + pg;
        if (sense10)
            vtl_put_be16((u16)(out_len - 2), buffer);
        else
            buffer[0] = (u8)(out_len - 1);
    }

    alloc_len = min_t(unsigned int, alloc_len, 255U);
    if (vtl_scsi_copy_to_sg(cmd, buffer, min_t(unsigned int, alloc_len, out_len), vtl_sense_ptr(cmd, vhost))) {
        vtl_xfer_buf_free(buffer);
        return SAM_STAT_CHECK_CONDITION;
    }
    vtl_xfer_buf_free(buffer);

    return SAM_STAT_GOOD;
}


static u32 vtl_get_u24(const u8 *p);

static int vtl_handle_mode_select(struct scsi_cmnd *cmd, struct vtl_host *vhost)
{
    u8 *cdb = cmd->cmnd;
    u8 *pbuf;
    unsigned int plen;
    unsigned int lun = cmd->device->lun;
    struct vtl_changer *ch = vhost->changer;
    struct vtl_drive *drv = NULL;
    u8 bd_len;
    u8 new_density;
    u32 new_block_size;

    if (cdb[0] == MODE_SELECT)
        plen = cdb[4];
    else
        plen = ((unsigned int)cdb[7] << 8) | cdb[8];

    if (lun >= 1 && lun <= (unsigned int)ch->num_drives)
        drv = &ch->drives[lun - 1];

    if (!drv)
        return SAM_STAT_GOOD;

    if (plen == 0) {
        vtl_set_sense(&drv->sense, ILLEGAL_REQUEST, 0x1a, 0);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    pbuf = vtl_xfer_buf_alloc(plen);
    if (!pbuf) {
        vtl_scsi_staging_oom(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    if (vtl_scsi_copy_from_sg(cmd, pbuf, plen, &drv->sense)) {
        vtl_xfer_buf_free(pbuf);
        return SAM_STAT_CHECK_CONDITION;
    }

    if (cdb[0] == MODE_SELECT_10) {
        if (plen >= 8)
            bd_len = pbuf[6] << 8 | pbuf[7];
        else
            bd_len = 0;
    } else {
        if (plen >= 4)
            bd_len = pbuf[3];
        else
            bd_len = 0;
    }

    if (bd_len >= 8) {
        if (cdb[0] == MODE_SELECT_10) {
            if (plen < 16)
                goto mode_pages;
            new_density = pbuf[8];
            new_block_size = vtl_get_u24(&pbuf[13]);
        } else {
            if (plen < 12)
                goto mode_pages;
            new_density = pbuf[4];
            new_block_size = vtl_get_u24(&pbuf[9]);
        }

        mutex_lock(&drv->lock);
        drv->density = new_density;
        if (new_block_size >= VTL_MIN_BLOCK_SIZE &&
            new_block_size <= VTL_MAX_BLOCK_SIZE)
            drv->block_size = new_block_size;
        else if (new_block_size == 0)
            drv->block_size = 0;
        else
            pr_info("VTL: MODE SELECT block_size %u out of range, ignored\n",
                new_block_size);
        mutex_unlock(&drv->lock);

        pr_info("VTL: MODE SELECT drive %d density=0x%02x block_size=%u\n",
            drv->id, new_density, new_block_size);
    }

mode_pages:

    /* Parse mode page parameters after the block descriptor */
    {
        u32 parsed;
        if (cdb[0] == MODE_SELECT_10)
            parsed = 8 + bd_len;
        else
            parsed = 4 + bd_len;

        while (parsed + 2 <= plen) {
            u8 pg_code = pbuf[parsed];
            u8 pg_len = pbuf[parsed + 1];

            if (parsed + 2 + pg_len > plen)
                break;

            if (pg_code == 0x0f && pg_len >= 1) {
                /* Data Compression page: byte 2 bit 1 = DCE */
                bool new_dce = (pbuf[parsed + 2] & 0x02) != 0;
                pr_info("VTL: MODE SELECT page 0x0F DCE=%d drive %d\n",
                    new_dce, drv->id);
                mutex_lock(&drv->lock);
                drv->compression_enabled = new_dce;
                mutex_unlock(&drv->lock);
            }

            parsed += 2 + pg_len;
        }
    }

    vtl_xfer_buf_free(pbuf);
    return SAM_STAT_GOOD;
}

static void vtl_parse_rw_blocks(const u8 *cdb, u8 op, u32 *blocks, u32 *block_len,
                                struct vtl_drive *drv)
{
    bool fixed;
    u32 transfer_len;

    *block_len = drv->block_size;
    *blocks = 0;

    switch (op) {
    case READ_6:
    case WRITE_6:
        fixed = (cdb[1] & 0x01) != 0;
        transfer_len = vtl_get_u24(&cdb[2]);
        /* variable block mode: ignore FIXED, use CDB length as block size */
        if (drv->block_size == 0)
            fixed = false;
        if (fixed)
            *blocks = transfer_len;
        else {
            *block_len = transfer_len;
            *blocks = 1;
        }
        break;
    case READ_10:
    case WRITE_10:
        fixed = (cdb[1] & 0x02) != 0;
        transfer_len = vtl_get_u24(&cdb[6]);
        if (drv->block_size == 0)
            fixed = false;
        if (fixed)
            *blocks = transfer_len;
        else {
            *block_len = transfer_len;
            *blocks = 1;
        }
        break;
    case READ_12:
    case WRITE_12:
        fixed = (cdb[1] & 0x02) != 0;
        transfer_len = vtl_get_be32(&cdb[6]);
        if (drv->block_size == 0)
            fixed = false;
        if (fixed)
            *blocks = transfer_len;
        else {
            *block_len = transfer_len;
            *blocks = 1;
        }
        break;
    default:
        break;
    }
}

static int vtl_get_s24(const u8 *p)
{
    u32 v = ((u32)p[0] << 16) | ((u32)p[1] << 8) | (u32)p[2];

    if (v & 0x00800000)
        v |= 0xff000000;
    return (s32)v;
}

static u32 vtl_get_u24(const u8 *p)
{
    return ((u32)p[0] << 16) | ((u32)p[1] << 8) | (u32)p[2];
}

/* Single-command READ/WRITE byte ceiling (avoids overflow and abusive CDB values) */
#define VTL_SCSI_RW_CAP_BYTES VTL_XFER_BUF_MAX

static int vtl_rw_prepare_xfer(struct scsi_cmnd *cmd, u32 *blocks, u32 *block_len,
                               struct vtl_sense_data *sense)
{
    u64 bytes;

    if (*blocks == 0 || *block_len == 0) {
        vtl_set_sense(sense, ILLEGAL_REQUEST, 0x24, 0);
        vtl_build_sense_buffer(cmd, sense);
        return -EINVAL;
    }
    if (*block_len > VTL_MAX_BLOCK_SIZE || *block_len < VTL_MIN_BLOCK_SIZE) {
        vtl_set_sense(sense, ILLEGAL_REQUEST, 0x24, 0);
        vtl_build_sense_buffer(cmd, sense);
        return -EINVAL;
    }

    bytes = (u64)(*blocks) * (u64)(*block_len);
    if (bytes > (u64)VTL_SCSI_RW_CAP_BYTES) {
        vtl_set_sense(sense, ILLEGAL_REQUEST, 0x24, 0);
        vtl_build_sense_buffer(cmd, sense);
        return -EINVAL;
    }
    if (bytes > 0xffffffffULL) {
        vtl_set_sense(sense, ILLEGAL_REQUEST, 0x24, 0);
        vtl_build_sense_buffer(cmd, sense);
        return -EINVAL;
    }

    return 0;
}

static int vtl_handle_read(struct scsi_cmnd *cmd, struct vtl_drive *drv, u8 op)
{
    u8 *cdb = cmd->cmnd;
    u8 *buffer;
    u32 block_len;
    u32 blocks;
    u32 actual;
    int ret;

    vtl_parse_rw_blocks(cdb, op, &blocks, &block_len, drv);
    if (vtl_rw_prepare_xfer(cmd, &blocks, &block_len, &drv->sense) < 0)
        return SAM_STAT_CHECK_CONDITION;

    buffer = vtl_xfer_buf_alloc(blocks * block_len);
    if (!buffer) {
        vtl_scsi_staging_oom(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    ret = vtl_tape_read(drv, buffer, blocks * block_len, &actual);
    if (ret < 0) {
        /* Empty drive: return 0 bytes instead of NOT_READY */
        if (ret == -ENODEV) {
            vtl_xfer_buf_free(buffer);
            return SAM_STAT_GOOD;
        }
        vtl_set_sense(&drv->sense, MEDIUM_ERROR, 0x11, 0);
        vtl_build_sense_buffer(cmd, &drv->sense);
        vtl_xfer_buf_free(buffer);
        return SAM_STAT_CHECK_CONDITION;
    }

    /* End of data: return 0 bytes. meta.used tracks write boundary so
     * reads stop at the natural data end, not immediately at BOT. */
    /* End of data: return 0 bytes. meta.used tracks write boundary.
     * st driver buffer interference is handled by mt status warm-up in install.sh. */
    if (actual == 0) {
        vtl_xfer_buf_free(buffer);
        return SAM_STAT_GOOD;
    }

    if (vtl_scsi_copy_to_sg(cmd, buffer, actual, &drv->sense)) {
        vtl_xfer_buf_free(buffer);
        return SAM_STAT_CHECK_CONDITION;
    }
    vtl_xfer_buf_free(buffer);

    return SAM_STAT_GOOD;
}

static int vtl_handle_write(struct scsi_cmnd *cmd, struct vtl_drive *drv, u8 op)
{
    u8 *cdb = cmd->cmnd;
    u8 *buffer;
    u32 block_len;
    u32 blocks;
    int ret;

    vtl_parse_rw_blocks(cdb, op, &blocks, &block_len, drv);
    if (vtl_rw_prepare_xfer(cmd, &blocks, &block_len, &drv->sense) < 0)
        return SAM_STAT_CHECK_CONDITION;

    buffer = vtl_xfer_buf_alloc(blocks * block_len);
    if (!buffer) {
        vtl_scsi_staging_oom(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    if (vtl_scsi_copy_from_sg(cmd, buffer, blocks * block_len, &drv->sense)) {
        vtl_xfer_buf_free(buffer);
        return SAM_STAT_CHECK_CONDITION;
    }
    ret = vtl_tape_write(drv, buffer, blocks * block_len, NULL);
    vtl_xfer_buf_free(buffer);

    if (ret < 0) {
        /* Empty drive: no-op instead of NOT_READY */
        if (ret == -ENODEV)
            return SAM_STAT_GOOD;
        if (ret == -EROFS)
            vtl_set_sense(&drv->sense, DATA_PROTECT, 0x27, 0);
        else if (ret == -ENOSPC)
            vtl_set_sense(&drv->sense, VOLUME_OVERFLOW, 0x00, 0);
        else
            vtl_set_sense(&drv->sense, MEDIUM_ERROR, 0x03, 0);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    /* Early Warning EOM: tape nearing end of capacity */
    {
        struct vtl_tape *tp;
        mutex_lock(&drv->lock);
        tp = drv->loaded_tape;
        if (tp && tp->meta.capacity > VTL_EARLY_WARN_MARGIN &&
            tp->position >= tp->meta.capacity - VTL_EARLY_WARN_MARGIN) {
            mutex_unlock(&drv->lock);
            vtl_set_sense(&drv->sense, NO_SENSE, 0x00, 0x02);
            vtl_build_sense_buffer(cmd, &drv->sense);
            return SAM_STAT_CHECK_CONDITION;
        }
        mutex_unlock(&drv->lock);
    }

    return SAM_STAT_GOOD;
}

static int vtl_handle_rewind(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    /* No-op on empty drive; vtl_tape_rewind only returns 0 or -ENODEV */
    (void)cmd;
    (void)vtl_tape_rewind(drv);
    return SAM_STAT_GOOD;
}

static int vtl_handle_space(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    u8 *cdb = cmd->cmnd;
    int code, count;
    int ret;

    code = cdb[1] & 7;
    count = vtl_get_s24(&cdb[2]);

    ret = vtl_tape_space(drv, code, count);
    /* Empty drive: no-op space */
    if (ret == -ENODEV)
        return SAM_STAT_GOOD;
    if (ret < 0) {
        vtl_set_sense(&drv->sense, ILLEGAL_REQUEST, 0x24, 0);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }
    return SAM_STAT_GOOD;
}

/*
 * READ CAPACITY (10): returns tape file capacity as LBA + block length.
 * Uses 512-byte logical blocks when drive is in variable block mode.
 */
/*
 * ERASE (0x19): short erase resets position/meta.used so reads return EOD;
 * long erase (LONG bit) does the same for VTL (no physical medium to sanitize).
 * IMMED=1 returns immediately; IMMED=0 waits (identical for VTL).
 */
static int vtl_handle_erase(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    u8 *cdb = cmd->cmnd;
    struct vtl_tape *tape;
    (void)cdb; /* LONG/IMMED bits — same outcome in VTL */

    mutex_lock(&drv->lock);
    tape = drv->loaded_tape;
    if (!tape) {
        mutex_unlock(&drv->lock);
        vtl_set_sense(&drv->sense, NOT_READY, 0x3a, 0);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }
    if (tape->write_protected) {
        mutex_unlock(&drv->lock);
        vtl_set_sense(&drv->sense, DATA_PROTECT, 0x27, 0);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    /* Short/long erase: reset data boundary.
     * LONG bit (cdb[1] & 0x20) for thorough erase — same outcome in VTL. */
    mutex_lock(&tape->lock);
    tape->position = 0;
    /* meta.used tracks written data */
    drv->at_bot = true;
    drv->at_end = false;
    drv->at_filemark = false;
    /* Clear filemarks */
    vtl_tape_free_metadata(tape);
    tape->meta.accessed = ktime_get_real_seconds();
    mutex_unlock(&tape->lock);
    mutex_unlock(&drv->lock);

    return SAM_STAT_GOOD;
}

static int vtl_handle_read_capacity_10(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    struct vtl_tape *tape;
    u8 buf[8];
    u64 capacity;
    u32 block_len;
    u32 max_lba;

    mutex_lock(&drv->lock);
    tape = drv->loaded_tape;
    if (!tape) {
        mutex_unlock(&drv->lock);
        vtl_set_sense(&drv->sense, NOT_READY, 0x3a, 0x00);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }
    capacity = tape->meta.capacity;
    block_len = drv->block_size ? drv->block_size : 512U;
    mutex_unlock(&drv->lock);

    {
        u64 num_blocks = capacity / (u64)block_len;

        /* Empty tape is valid — return max_lba=0 */
        if (num_blocks == 0)
            max_lba = 0;
        else if (num_blocks > 0xFFFFFFFFULL)
            max_lba = 0xFFFFFFFFU;
        else
            max_lba = (u32)(num_blocks - 1ULL);
    }

    memset(buf, 0, sizeof(buf));
    vtl_put_be32(max_lba, &buf[0]);
    vtl_put_be32(block_len, &buf[4]);

    if (vtl_scsi_copy_to_sg(cmd, buf, sizeof(buf), &drv->sense)) {
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }
    return SAM_STAT_GOOD;
}

/*
 * SERVICE ACTION IN (0x9E) — READ CAPACITY (16) service action 0x10.
 */
static int vtl_handle_read_capacity_16(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    struct vtl_tape *tape;
    u8 buf[32];
    u64 capacity;
    u32 block_len;
    u64 max_lba;

    mutex_lock(&drv->lock);
    tape = drv->loaded_tape;
    if (!tape) {
        mutex_unlock(&drv->lock);
        vtl_set_sense(&drv->sense, NOT_READY, 0x3a, 0x00);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }
    capacity = tape->meta.capacity;
    block_len = drv->block_size ? drv->block_size : 512U;
    mutex_unlock(&drv->lock);

    {
        u64 num_blocks = capacity / (u64)block_len;

        /* Empty tape is valid — return max_lba=0 (consistent with READ CAPACITY 10) */
        if (num_blocks == 0)
            max_lba = 0;
        else if (num_blocks > 0xFFFFFFFFULL)
            max_lba = 0xFFFFFFFFULL;
        else
            max_lba = num_blocks - 1ULL;
    }

    memset(buf, 0, sizeof(buf));
    vtl_put_be64(max_lba, &buf[0]);
    vtl_put_be32(block_len, &buf[8]);

    if (vtl_scsi_copy_to_sg(cmd, buf, sizeof(buf), &drv->sense)) {
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }
    return SAM_STAT_GOOD;
}

static int vtl_handle_write_filemarks(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    u8 *cdb = cmd->cmnd;
    int ret;
    u32 count;

    count = vtl_get_u24(&cdb[2]);
    ret = vtl_tape_write_filemarks(drv, count);
    if (ret < 0) {
        /* Empty drive: st_close sends WRITE_FILEMARKS - no-op */
        if (ret == -ENODEV)
            return SAM_STAT_GOOD;
        if (ret == -EROFS)
            vtl_set_sense(&drv->sense, DATA_PROTECT, 0x27, 0);
        else
            vtl_set_sense(&drv->sense, MEDIUM_ERROR, 0x03, 0);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }
    return SAM_STAT_GOOD;
}

/*
 * SYNCHRONIZE CACHE (SSC): the st driver translates fsync/fdatasync into
 * this CDB.  With a backing file the data is already durable after each
 * kernel_write, but we still call vfs_fsync so that userspace sees a
 * successful fsync(2) instead of EINVAL.
 *
 * kref_get pins the tape across the drv->lock release so that a concurrent
 * UNLOAD cannot trigger vtl_tape_release -> filp_close before vfs_fsync.
 */
static int vtl_handle_synchronize_cache(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    int ret = 0;
    struct vtl_tape *tape;
    struct file *filp;

    mutex_lock(&drv->lock);
    tape = drv->loaded_tape;
    if (!tape) {
        mutex_unlock(&drv->lock);
        vtl_set_sense(&drv->sense, NOT_READY, 0x3a, 0);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }
    kref_get(&tape->ref);
    filp = tape->file;
    mutex_unlock(&drv->lock);

    if (filp && !IS_ERR(filp))
        ret = vfs_fsync(filp, 0);

    vtl_tape_put(tape);

    if (ret < 0) {
        vtl_set_sense(&drv->sense, MEDIUM_ERROR, 0x03, 0);
        vtl_build_sense_buffer(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }
    return SAM_STAT_GOOD;
}

static int vtl_handle_load_unload(struct scsi_cmnd *cmd, struct vtl_drive *drv,
                   struct vtl_changer *ch, unsigned int drive_idx)
{
    u8 *cdb = cmd->cmnd;
    u8 load;
    bool immed;

    load = cdb[4] & 0x01;
    immed = (cdb[4] & 0x04) != 0;
    (void)immed;  /* VTL ops always complete synchronously */

    if (load) {
        /* Load: no-op for VTL — tapes are loaded via MOVE_MEDIUM */
        if (!vtl_drive_has_tape(drv))
            return SAM_STAT_GOOD;
    } else {
        /* Load=0: REWIND only (SSC-3 Position to BOP semantics).
         * Physical unload is SMC MOVE_MEDIUM, not LOAD_UNLOAD. */
        mutex_lock(&drv->lock);
        if (drv->loaded_tape) {
            drv->loaded_tape->position = 0;
            drv->at_bot = true;
            drv->at_end = false;
            drv->at_filemark = false;
        }
        mutex_unlock(&drv->lock);
    }

    return SAM_STAT_GOOD;
}

static int vtl_handle_move_medium(struct scsi_cmnd *cmd, struct vtl_host *vhost)
{
    u8 *cdb = cmd->cmnd;
    struct vtl_changer *ch = vhost->changer;
    int src, dst;
    int ret;

    /*
     * SMC-3 MOVE MEDIUM CDB (12 bytes):
     *   bytes 2-3 = Transport Element Address (0 when no picker)
     *   bytes 4-5 = Source Address
     *   bytes 6-7 = Destination Address
     */
    src = (cdb[4] << 8) | cdb[5];
    dst = (cdb[6] << 8) | cdb[7];

    /* Pre-validate so empty/missing source returns MEDIUM NOT PRESENT
     * instead of ILLEGAL REQUEST, which avoids Kylin st offline cascade. */
    if (vtl_elem_is_storage(ch, src)) {
        int si = src - 1;
        if (si >= 0 && si < ch->num_slots &&
            (!ch->slots[si].occupied || !ch->slots[si].tape)) {
            vtl_set_sense(&ch->sense, NOT_READY, 0x3a, 0x00);
            vtl_build_sense_buffer(cmd, &ch->sense);
            return SAM_STAT_CHECK_CONDITION;
        }
    }
    if (vtl_elem_is_drive(ch, src)) {
        int di = src - vtl_elem_drive_base(ch);
        if (di >= 0 && di < ch->num_drives &&
            !ch->drives[di].loaded_tape) {
            vtl_set_sense(&ch->sense, NOT_READY, 0x3a, 0x00);
            vtl_build_sense_buffer(cmd, &ch->sense);
            return SAM_STAT_CHECK_CONDITION;
        }
    }
    if (vtl_elem_is_ie(ch, src)) {
        int mi = src - vtl_elem_ie_base(ch);
        if (mi >= 0 && mi < ch->num_mailslots &&
            !ch->mailslots[mi].occupied) {
            vtl_set_sense(&ch->sense, NOT_READY, 0x3a, 0x00);
            vtl_build_sense_buffer(cmd, &ch->sense);
            return SAM_STAT_CHECK_CONDITION;
        }
    }

    ret = vtl_changer_move_medium(ch, src, dst);
    if (ret < 0) {
        vtl_set_sense(&ch->sense, ILLEGAL_REQUEST, 0x21, 0);
        vtl_build_sense_buffer(cmd, &ch->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    /* After successful tape load, ensure all drives are online.
     * Kylin st may offline drives during background probing. */
    {
        struct scsi_device *sdev;
        shost_for_each_device(sdev, cmd->device->host) {
            if (sdev->sdev_state == SDEV_OFFLINE)
                scsi_device_set_state(sdev, SDEV_RUNNING);
        }
    }

    /* Unit Attention: only on media REPLACEMENT (not initial load or final empty).
     * Over-aggressive UA triggers Kylin st offline. */
    if (vtl_elem_is_drive(ch, dst)) {
        int di = dst - vtl_elem_drive_base(ch);
        /* UA if destination drive already had a tape (replacement) */
        if (di >= 0 && di < ch->num_drives &&
            !vtl_elem_is_drive(ch, src)) {  /* src is slot/IE, not another drive */
            bool had_tape;
            mutex_lock(&ch->drives[di].lock);
            had_tape = (ch->drives[di].loaded_tape != NULL);
            mutex_unlock(&ch->drives[di].lock);
            if (had_tape) {
                ch->drives[di].ua_pending = true;
                ch->drives[di].ua_asc = 0x28;
                ch->drives[di].ua_ascq = 0x00;
            }
        }
    }
    if (vtl_elem_is_drive(ch, src)) {
        int di = src - vtl_elem_drive_base(ch);
        /* UA if drive becomes empty (tape removed) and DST is not a drive */
        if (di >= 0 && di < ch->num_drives &&
            !vtl_elem_is_drive(ch, dst)) {
            ch->drives[di].ua_pending = true;
            ch->drives[di].ua_asc = 0x28;
            ch->drives[di].ua_ascq = 0x00;
        }
    }

    return SAM_STAT_GOOD;
}

/*
 * READ ELEMENT STATUS allocation / range: mtx and Linux ch(4) often use the
 * 6-byte form (alloc in cdb[5]); 10-byte uses cdb[7:8]; 12-byte (mtx) uses
 * cdb[7:9] as 24-bit length. Do not read cdb[9:11] on 12-byte CDBs — byte 9
 * is the LSB of alloc (e.g. 0xff → 255), not the MSB of a triplet at [9:11].
 */
static unsigned int vtl_res_alloc_len(const struct scsi_cmnd *cmd)
{
    const u8 *cdb = cmd->cmnd;

    if (cmd->cmd_len >= 12)
        return ((unsigned int)cdb[7] << 16) | ((unsigned int)cdb[8] << 8) |
	       cdb[9];
    if (cmd->cmd_len >= 10)
        return ((unsigned int)cdb[7] << 8) | cdb[8];
    if (cmd->cmd_len >= 6)
        return (unsigned int)cdb[5];
    return 0;
}

static void vtl_res_element_range(const struct scsi_cmnd *cmd, int *start, int *num)
{
    const u8 *cdb = cmd->cmnd;

    *start = (cdb[2] << 8) | cdb[3];
    if (cmd->cmd_len >= 10)
        *num = (cdb[4] << 8) | cdb[5];
    else if (cmd->cmd_len >= 6)
        *num = cdb[4];
    else
        *num = 0;
}

/*
 * Detect PVolTag output format for READ ELEMENT STATUS.
 * Consults the pvoltag_format module parameter and CDB characteristics
 * to decide whether to include Primary Volume Tag data (voltag) and whether
 * to use the SMC-3 standard 4-byte PVolTag header (voltag_std).
 * AUTO heuristic: PV bit set → mtx format; no PV + alloc>=32 → standard.
 */
static void vtl_detect_pvoltag(const struct scsi_cmnd *cmd,
                               unsigned int req_len,
                               bool *voltag, bool *voltag_std)
{
    const u8 *cdb = cmd->cmnd;
    bool pv_bit = (cdb[1] & VTL_CDB_PV_BIT) != 0;

    switch (vtl_pvoltag_format) {
    case VTL_PVOLTAG_STANDARD:
        *voltag = pv_bit;
        *voltag_std = true;
        return;
    case VTL_PVOLTAG_MTX:
        *voltag = pv_bit;
        *voltag_std = false;
        return;
    default:
        break;
    }

    /* VTL_PVOLTAG_AUTO: PV bit → mtx format (barcode at byte 12).
     * No PV + alloc-len >= 32 → standard SMC-3 header (Mars/Veritas workaround). */
    *voltag = pv_bit;
    *voltag_std = false;
    if (!pv_bit && req_len >= 32U) {
        *voltag = true;
        *voltag_std = true;
    }
}

static int vtl_handle_read_element_status(struct scsi_cmnd *cmd, struct vtl_host *vhost)
{
    u8 *cdb = cmd->cmnd;
    struct vtl_changer *ch = vhost->changer;
    u8 *buffer;
    unsigned int req_len;
    unsigned int work_len;
    int start_elem, num_elems;
    bool voltag;
    bool voltag_std;
    int ret;

    buffer = vtl_xfer_buf_alloc(VTL_ELEMENT_STATUS_BUFLEN);
    if (!buffer) {
        vtl_scsi_staging_oom(cmd, &ch->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    req_len = vtl_res_alloc_len(cmd);
    vtl_detect_pvoltag(cmd, req_len, &voltag, &voltag_std);
    work_len = req_len ? min_t(unsigned int, req_len, VTL_ELEMENT_STATUS_BUFLEN)
			: VTL_ELEMENT_STATUS_BUFLEN;
    /*
     * Floor at 4 KiB when voltag is requested so inventory/mtx/sg agree.
     */
    if (voltag && work_len < 4096U)
        work_len = min_t(unsigned int, VTL_ELEMENT_STATUS_BUFLEN, 4096U);
    if (cmd->cmd_len >= 10 && work_len < 4096U)
        work_len = min_t(unsigned int, VTL_ELEMENT_STATUS_BUFLEN, 4096U);
    if (work_len < 8)
        work_len = 8;
    vtl_res_element_range(cmd, &start_elem, &num_elems);
    ret = vtl_changer_read_element_status(
        ch, buffer, work_len, voltag, voltag_std, cdb[1] & 0x0f,
        start_elem, num_elems);
    if (ret < 0) {
        vtl_set_sense(&ch->sense, HARDWARE_ERROR, 0x00, 0x00);
        vtl_build_sense_buffer(cmd, &ch->sense);
        vtl_xfer_buf_free(buffer);
        return SAM_STAT_CHECK_CONDITION;
    }

    /*
     * Inline copy + one scsi_set_resid: avoids stacking residual from
     * vtl_scsi_copy_to_sg (short SG) with (req_len - ret) from allocation trim.
     */
    {
        int copied;

        copied = scsi_sg_copy_from_buffer(cmd, buffer, ret);
        if (unlikely(copied < 0)) {
            vtl_set_sense(&ch->sense, ABORTED_COMMAND, 0x00, 0x00);
            vtl_build_sense_buffer(cmd, &ch->sense);
            vtl_xfer_buf_free(buffer);
            return SAM_STAT_CHECK_CONDITION;
        }
        if (unlikely((unsigned int)copied < (unsigned int)ret)) {
            vtl_set_sense(&ch->sense, ABORTED_COMMAND, 0x00, 0x00);
            vtl_build_sense_buffer(cmd, &ch->sense);
            vtl_xfer_buf_free(buffer);
            return SAM_STAT_CHECK_CONDITION;
        }
        if (req_len > (unsigned int)copied)
            scsi_set_resid(cmd, req_len - (unsigned int)copied);
    }
    vtl_xfer_buf_free(buffer);

    return SAM_STAT_GOOD;
}

/*
 * LOG SENSE allocation length: 6-byte CDB uses byte 4; 10-byte uses bytes 7–8 (SPC).
 */
static unsigned int vtl_log_sense_alloc_len(struct scsi_cmnd *cmd)
{
    const u8 *cdb = cmd->cmnd;

    if (cmd->cmd_len >= 10)
        return (cdb[7] << 8) | cdb[8];
    if (cmd->cmd_len >= 6)
        return cdb[4];
    return 0;
}

/*
 * LOG SENSE helper: write a page header (page_code + 2-byte page_length).
 * Returns offset after the 4-byte header for writing parameters.
 */
static int vtl_log_write_page_hdr(u8 *buf, u8 page, u16 pg_len)
{
    buf[0] = page;
    buf[1] = 0;  /* reserved + SPF=0 */
    buf[2] = (pg_len >> 8) & 0xff;
    buf[3] = pg_len & 0xff;
    return 4;
}

/*
 * LOG SENSE helper: write a single log parameter.
 * Returns number of bytes written (4-byte header + param_len).
 */
static int vtl_log_write_param(u8 *buf, u16 param_code, u8 param_len)
{
    /* Parameter code (big-endian) */
    buf[0] = (param_code >> 8) & 0xff;
    buf[1] = param_code & 0xff;
    /* DU=0, TSD=0, ETC=0, TMC=0, LP=0, LBIN=1 */
    buf[2] = 0x02;
    buf[3] = param_len;
    return 4 + param_len;
}

/*
 * SSC-compliant LOG SENSE for tape drives.
 * Pages: 0x00 (supported), 0x02 (write errors), 0x03 (read errors),
 * 0x06 (non-medium), 0x0C (sequential access), 0x2E (tape alert).
 * Unknown pages return an empty page header (parameter length=0) instead
 * of ILLEGAL REQUEST so initiators see a clean "no data" response.
 */
static int vtl_handle_log_sense(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    u8 *cdb = cmd->cmnd;
    u8 page = cdb[2] & 0x3f;
    unsigned int alloc = vtl_log_sense_alloc_len(cmd);
    u8 *buf;
    u16 out_len;
    unsigned int z;
    u64 log_bytes_read = 0;
    u64 log_bytes_written = 0;
    int off;

    if (alloc == 0)
        return SAM_STAT_GOOD;

    z = min_t(unsigned int, alloc, 512U);
    buf = vtl_xfer_buf_alloc(z);
    if (!buf) {
        vtl_scsi_staging_oom(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    memset(buf, 0, z);
    off = 0;

    switch (page) {
    case 0x00:
        /* Supported pages: 0x00, 0x02, 0x03, 0x06, 0x0C, 0x2E */
        off = vtl_log_write_page_hdr(buf, 0x00, 7);
        buf[off++] = 0x00; buf[off++] = 0x02; buf[off++] = 0x03;
        buf[off++] = 0x06; buf[off++] = 0x0c; buf[off++] = 0x2e;
        buf[off++] = 0x11;  /* custom volume-usage page */
        break;

    case 0x02:
        /* Write Error Counters: total errors (code 0x0001) + total retries (0x0002) */
        off = vtl_log_write_page_hdr(buf, 0x02,
             (4 + 8) + (4 + 8));  /* 2 params * 12 bytes each */
        off += vtl_log_write_param(buf + off, 0x0001, 8); /* bytes 4-11 = 0 */
        off += vtl_log_write_param(buf + off, 0x0002, 8);
        break;

    case 0x03:
        /* Read Error Counters: total errors + total retries */
        off = vtl_log_write_page_hdr(buf, 0x03, (4 + 8) + (4 + 8));
        off += vtl_log_write_param(buf + off, 0x0001, 8);
        off += vtl_log_write_param(buf + off, 0x0002, 8);
        break;

    case 0x06:
        /* Non-Medium Error: count */
        off = vtl_log_write_page_hdr(buf, 0x06, 4 + 8);
        off += vtl_log_write_param(buf + off, 0x0000, 8);
        break;

    case 0x0c:
        /* Sequential Access Device (8 params of 8 bytes each) */
        off = vtl_log_write_page_hdr(buf, 0x0c, 8 * (4 + 8));
        off += vtl_log_write_param(buf + off, 0x0000, 8); /* data bytes written */
        off += vtl_log_write_param(buf + off, 0x0001, 8); /* data bytes read */
        off += vtl_log_write_param(buf + off, 0x0002, 8); /* bytes written to tape */
        off += vtl_log_write_param(buf + off, 0x0003, 8); /* bytes read from tape */
        off += vtl_log_write_param(buf + off, 0x8000, 8); /* cleaning status */
        off += vtl_log_write_param(buf + off, 0x8001, 8);
        off += vtl_log_write_param(buf + off, 0x8002, 8);
        off += vtl_log_write_param(buf + off, 0x8003, 8);
        break;

    case 0x2e:
        /* Tape Alert: 1 parameter, 8 bytes of flags (64 bits, all zero = no alerts) */
        off = vtl_log_write_page_hdr(buf, 0x2e, 4 + 8);
        off += vtl_log_write_param(buf + off, 0x0000, 8);
        break;

    case 0x11:
        /* Custom volume-usage page (existing behaviour) */
        mutex_lock(&drv->lock);
        if (!drv->loaded_tape) {
            mutex_unlock(&drv->lock);
            vtl_xfer_buf_free(buf);
            vtl_set_sense(&drv->sense, NOT_READY, 0x3a, 0);
            vtl_build_sense_buffer(cmd, &drv->sense);
            return SAM_STAT_CHECK_CONDITION;
        }
        mutex_lock(&drv->loaded_tape->lock);
        log_bytes_read = drv->loaded_tape->meta.log_bytes_read;
        log_bytes_written = drv->loaded_tape->meta.log_bytes_written;
        mutex_unlock(&drv->loaded_tape->lock);
        mutex_unlock(&drv->lock);
        off = vtl_log_write_page_hdr(buf, 0x11, 16);
        vtl_put_be64(log_bytes_read, &buf[off]);
        vtl_put_be64(log_bytes_written, &buf[off + 8]);
        off += 16;
        break;

    default:
        /* Unknown page: return empty page header (parameter length = 0) */
        off = vtl_log_write_page_hdr(buf, page, 0);
        break;
    }

    out_len = (u16)off;
    if (vtl_scsi_copy_to_sg(cmd, buf, min_t(unsigned int, out_len, z), &drv->sense)) {
        vtl_xfer_buf_free(buf);
        return SAM_STAT_CHECK_CONDITION;
    }
    vtl_xfer_buf_free(buf);
    return SAM_STAT_GOOD;
}


/* SSC READ POSITION — short/long form, tolerant of non-standard CDB params */
static int vtl_handle_read_position(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    u8 *cdb = cmd->cmnd;
    u8 *buf;
    u16 alloc;
    u8 svc;
    bool loaded;
    bool at_bot = false;
    bool at_end = false;
    bool at_filemark = false;
    loff_t position = 0;


    if (cmd->cmd_len >= 10)
        alloc = (cdb[7] << 8) | cdb[8];
    else
        alloc = cdb[4];

    svc = (cdb[1] & 0x1f);
    if (svc != 0x00)
        pr_info_ratelimited("VTL: READ POSITION svc=0x%02x (using short-form 20B response)\n", svc);

    /* accept any alloc; floor to 20 below */
    if (alloc == 0)
        alloc = 20;

    buf = vtl_xfer_buf_alloc(20);
    if (!buf) {
        vtl_scsi_staging_oom(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    memset(buf, 0, 20);
    buf[0] = 0x80;
    mutex_lock(&drv->lock);
    loaded = drv->loaded_tape != NULL;
    if (loaded) {
        mutex_lock(&drv->loaded_tape->lock);
        at_bot = drv->at_bot;
        at_end = drv->at_end;
        at_filemark = drv->at_filemark;
        position = drv->loaded_tape->position;
        mutex_unlock(&drv->loaded_tape->lock);
    }
    mutex_unlock(&drv->lock);

    if (loaded) {
        if (at_bot)
            buf[1] |= 0x80;
        if (at_end)
            buf[1] |= 0x40;
        if (at_filemark)
            buf[1] |= 0x20;
        vtl_put_be64((u64)position, &buf[4]);

        /* Compute file number: count filemarks before current position */
        {
            struct vtl_tape *tp = drv->loaded_tape;
            u32 lo = 0, hi = tp->num_filemarks;
            /* Binary search for first filemark > position */
            while (lo < hi) {
                u32 mid = lo + (hi - lo) / 2;
                if (tp->filemark_offsets[mid] <= position)
                    lo = mid + 1;
                else
                    hi = mid;
            }
            /* lo = number of filemarks before or at position = current file number */
            vtl_put_be32(lo, &buf[12]);
        }
    } else {
        buf[1] |= 0x10;
    }

    if (vtl_scsi_copy_to_sg(cmd, buf, min_t(unsigned int, alloc, 20), &drv->sense)) {
        vtl_xfer_buf_free(buf);
        return SAM_STAT_CHECK_CONDITION;
    }
    vtl_xfer_buf_free(buf);
    return SAM_STAT_GOOD;
}

static int vtl_handle_prevent_allow(struct scsi_cmnd *cmd, struct vtl_drive *drv)
{
    (void)cmd;
    (void)drv;
    return SAM_STAT_GOOD;
}

/*
 * SSC-3 REPORT DENSITY SUPPORT (0x44) — backup applications (NetBackup, TSM, Bacula)
 * probe this during drive discovery to learn supported media types. Without it the
 * drive may appear to support zero densities, causing inventory/listing to fail.
 *
 * Short-form density support data block descriptor (52 bytes each, SSC-5 Table 117):
 *   bytes  0- 1: primary density code (big-endian)
 *   bytes  2- 3: secondary density code
 *   byte   4:    WRTOK(0x80) | DUP(0x40) | DEFAULT(0x20)
 *   bytes  5- 6: reserved
 *   bytes  7- 9: bits per mm (0=not reported)
 *   bytes 10-11: media width in tenths of mm (0=not reported)
 *   bytes 12-13: tracks (0=not reported)
 *   bytes 14-19: capacity in MB (0=not reported)
 *   bytes 20-27: assigning organization (ASCII, space-filled)
 *   bytes 28-35: density name (ASCII, space-filled)
 *   bytes 36-43: description (ASCII, space-filled)
 *   bytes 44-51: reserved
 */
#define VTL_DENSITY_DESC_LEN 52U
#define VTL_NUM_DENSITIES 7

struct vtl_known_density {
    u16 code;
    const char *name;
};

static const struct vtl_known_density vtl_supported_densities[] = {
    { 0x40, "LTO-4  " },
    { 0x4A, "LTO-5  " },
    { 0x4C, "LTO-6  " },
    { 0x4E, "LTO-7  " },
    { 0x50, "LTO-8  " },
    { 0x52, "LTO-9  " },
    { 0x58, "LTO-10 " },
};

static int vtl_fill_density_desc(u8 *p, u16 density_code, const char *name,
                                 bool is_default)
{
    memset(p, 0, VTL_DENSITY_DESC_LEN);
    vtl_put_be16(density_code, &p[0]);
    /* secondary density = same as primary (read-write compatible) */
    vtl_put_be16(density_code, &p[2]);
    p[4] = 0x80; /* WRTOK = writable */
    if (is_default)
        p[4] |= 0x20; /* DEFAULT */
    /* media width: 127 = 12.7mm for LTO */
    vtl_put_be16(127, &p[10]);
    /* tracks: 0 = not reported for virtual */
    memcpy(&p[20], "VTL     ", 8);
    memcpy(&p[28], name, 8);
    memcpy(&p[36], "VIRTUAL ", 8);
    return VTL_DENSITY_DESC_LEN;
}

static int vtl_handle_report_density_support(struct scsi_cmnd *cmd,
                                             struct vtl_drive *drv)
{
    u8 *cdb = cmd->cmnd;
    u32 alloc_len;
    u32 data_len;
    int i;
    u8 *buf;
    int off = 0;

    if (cmd->cmd_len >= 10)
        alloc_len = vtl_get_be32(&cdb[6]);
    else
        alloc_len = (cdb[3] << 16) | (cdb[4] << 8) | cdb[5];

    if (alloc_len == 0)
        return SAM_STAT_GOOD;

    data_len = VTL_NUM_DENSITIES * VTL_DENSITY_DESC_LEN;
    /* header: 4 bytes (2-byte length + 2 reserved) */
    if (data_len > VTL_XFER_BUF_MAX - 4U)
        data_len = VTL_XFER_BUF_MAX - 4U;

    buf = vtl_xfer_buf_alloc(4U + data_len);
    if (!buf) {
        vtl_scsi_staging_oom(cmd, &drv->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    memset(buf, 0, 4U + data_len);
    /* Available density support length = total descriptor bytes */
    vtl_put_be16((u16)data_len, &buf[0]);
    /* bytes 2-3 reserved */
    off = 4;

    for (i = 0; i < VTL_NUM_DENSITIES; i++) {
        bool is_default =
            (drv->density == vtl_supported_densities[i].code);
        if ((u32)(off + VTL_DENSITY_DESC_LEN) > 4U + data_len)
            break;
        off += vtl_fill_density_desc(&buf[off],
                         vtl_supported_densities[i].code,
                         vtl_supported_densities[i].name,
                         is_default);
    }

    {
        u32 xfer = min_t(u32, alloc_len, (u32)off);

        if (vtl_scsi_copy_to_sg(cmd, buf, (unsigned int)xfer,
                    &drv->sense)) {
            vtl_xfer_buf_free(buf);
            return SAM_STAT_CHECK_CONDITION;
        }
    }
    vtl_xfer_buf_free(buf);
    return SAM_STAT_GOOD;
}

/*
 * REPORT LUNS (SPC): lets scsi mid-layer enumerate 0..num_drives without
 * relying on sequential scan edge cases.
 */
static int vtl_handle_report_luns(struct scsi_cmnd *cmd, struct vtl_host *vhost)
{
    struct vtl_changer *ch = vhost->changer;
    u8 *cdb = cmd->cmnd;
    unsigned int nluns = (unsigned int)ch->num_drives + 1U;
    u32 list_len = nluns * (u32)sizeof(struct scsi_lun);
    u32 need = 8U + list_len;
    u32 alloc_len;
    u8 *buf;
    unsigned int i;
    struct scsi_lun *vec;
    unsigned int xfer;
    int err;

    if (cmd->cmd_len < 10)
        return vtl_cmd_illegal(cmd, &ch->sense);

    alloc_len = vtl_get_be32(&cdb[6]);
    if (alloc_len == 0)
        return SAM_STAT_GOOD;

    if (need > VTL_XFER_BUF_MAX)
        return vtl_cmd_illegal(cmd, &ch->sense);

    buf = vtl_xfer_buf_alloc((unsigned int)need);
    if (!buf) {
        vtl_scsi_staging_oom(cmd, &ch->sense);
        return SAM_STAT_CHECK_CONDITION;
    }

    xfer = min_t(u32, alloc_len, need);
    memset(buf, 0, (size_t)need);

    if (xfer >= 8) {
        vtl_put_be32(list_len, buf);
        vec = (struct scsi_lun *)(buf + 8);
        for (i = 0; i < nluns; i++) {
            if (8U + (i + 1U) * sizeof(struct scsi_lun) > xfer)
                break;
            int_to_scsilun(i, &vec[i]);
        }
    }

    err = vtl_scsi_copy_to_sg(cmd, buf, (unsigned int)xfer, &ch->sense);
    vtl_xfer_buf_free(buf);
    return err ? SAM_STAT_CHECK_CONDITION : SAM_STAT_GOOD;
}

static int vtl_changer_scsi(struct scsi_cmnd *cmd, struct vtl_host *vhost, u8 *cdb)
{
    struct vtl_changer *ch = vhost->changer;

    switch (cdb[0]) {
    case INQUIRY:
        return vtl_handle_inquiry(cmd, vhost);
    case TEST_UNIT_READY:
        return vtl_handle_test_unit_ready(cmd, vhost);
    case REQUEST_SENSE:
        return vtl_handle_request_sense(cmd, vhost);
    case MODE_SENSE:
    case MODE_SENSE_10:
        return vtl_handle_mode_sense(cmd, vhost);
    case MODE_SELECT:
    case MODE_SELECT_10:
        return vtl_handle_mode_select(cmd, vhost);
    case INITIALIZE_ELEMENT_STATUS:
        return SAM_STAT_GOOD;
    case PREVENT_ALLOW_MEDIUM_REMOVAL:
        return SAM_STAT_GOOD;
    case MOVE_MEDIUM:
        return vtl_handle_move_medium(cmd, vhost);
#if READ_ELEMENT_STATUS != 0xb4
    case 0xb4: /* READ ELEMENT STATUS (10); some backup stacks use b4 not b8 */
#endif
    case READ_ELEMENT_STATUS:
        return vtl_handle_read_element_status(cmd, vhost);
    case REPORT_LUNS:
        return vtl_handle_report_luns(cmd, vhost);
    default:
        return vtl_cmd_illegal(cmd, &ch->sense);
    }
}

static int vtl_tape_scsi(struct scsi_cmnd *cmd, struct vtl_host *vhost,
                         unsigned int drive_idx, u8 *cdb)
{
    struct vtl_changer *ch = vhost->changer;
    struct vtl_drive *drv;

    if (drive_idx >= (unsigned int)ch->num_drives)
        return vtl_cmd_lun_not_supported(cmd, ch);

    drv = &ch->drives[drive_idx];

    switch (cdb[0]) {
    case INQUIRY:
        return vtl_handle_inquiry(cmd, vhost);
    case TEST_UNIT_READY:
        return vtl_handle_test_unit_ready(cmd, vhost);
    case REQUEST_SENSE:
        return vtl_handle_request_sense(cmd, vhost);
    case READ_BLOCK_LIMITS:
        return vtl_handle_read_block_limits(cmd, drv);
    case MODE_SENSE:
    case MODE_SENSE_10:
        return vtl_handle_mode_sense(cmd, vhost);
    case MODE_SELECT:
    case MODE_SELECT_10:
        return vtl_handle_mode_select(cmd, vhost);
    case READ_6:
    case READ_10:
    case READ_12:
        return vtl_handle_read(cmd, drv, cdb[0]);
    case WRITE_6:
    case WRITE_10:
    case WRITE_12:
        return vtl_handle_write(cmd, drv, cdb[0]);
    case REWIND:
        return vtl_handle_rewind(cmd, drv);
    case SPACE:
        return vtl_handle_space(cmd, drv);
    case WRITE_FILEMARKS:
        return vtl_handle_write_filemarks(cmd, drv);
    case LOAD_UNLOAD:
        return vtl_handle_load_unload(cmd, drv, ch, drive_idx);
    case LOG_SENSE:
        return vtl_handle_log_sense(cmd, drv);
    case READ_POSITION:
        return vtl_handle_read_position(cmd, drv);
    case PREVENT_ALLOW_MEDIUM_REMOVAL:
        return vtl_handle_prevent_allow(cmd, drv);
    case REPORT_DENSITY_SUPPORT:
        return vtl_handle_report_density_support(cmd, drv);
    case READ_CAPACITY:
        return vtl_handle_read_capacity_10(cmd, drv);
    case ALLOW_OVERWRITE:
        return SAM_STAT_GOOD;
    case VERIFY_6:
        return SAM_STAT_GOOD;
    case SYNCHRONIZE_CACHE:
        return vtl_handle_synchronize_cache(cmd, drv);
    case ERASE:
        return vtl_handle_erase(cmd, drv);
    case SERVICE_ACTION_IN:
        if (cdb[1] == SERVICE_ACTION_READ_CAPACITY_16)
            return vtl_handle_read_capacity_16(cmd, drv);
        return vtl_cmd_illegal(cmd, &drv->sense);
    case POSITION_TO_ELEMENT:
        return SAM_STAT_GOOD;
    default:
        return vtl_cmd_illegal(cmd, &drv->sense);
    }
}

int vtl_scsi_queuecommand(struct Scsi_Host *shost, struct scsi_cmnd *cmd)
{
    struct vtl_host *vhost = shost_priv(shost);
    u8 *cdb = cmd->cmnd;
    unsigned int lun = cmd->device->lun;
    struct vtl_changer *ch;
    int result;

    /* Auto-restore VTL devices that Kylin st driver offlines */
    {
        struct scsi_device *sdev;
        shost_for_each_device(sdev, shost) {
            if (sdev->sdev_state == SDEV_OFFLINE)
                scsi_device_set_state(sdev, SDEV_RUNNING);
        }
    }

    /* Unit Attention: report pending UA before processing command.
     * INQUIRY / REQUEST_SENSE do NOT clear UA (per SPC-3). */
    if (lun >= 1 && lun <= (unsigned int)vhost->changer->num_drives) {
        struct vtl_drive *drv = &vhost->changer->drives[lun - 1];
        if (drv->ua_pending && cdb[0] != INQUIRY && cdb[0] != REQUEST_SENSE) {
            drv->ua_pending = false;
            vtl_set_sense(&drv->sense, UNIT_ATTENTION, drv->ua_asc, drv->ua_ascq);
            vtl_build_sense_buffer(cmd, &drv->sense);
            cmd->result = (DID_OK << 16) | (DRIVER_SENSE << 8) | (SAM_STAT_CHECK_CONDITION << 1);
            vtl_scsi_done(cmd);
            return 0;
        }
    }

    if (vtl_reconfig_in_progress()) {
        cmd->result = (DID_NO_CONNECT << 16);
        vtl_scsi_done(cmd);
        return 0;
    }

    down_read(&vhost->io_sem);
    if (vtl_reconfig_in_progress() || !vhost->changer) {
        up_read(&vhost->io_sem);
        cmd->result = (DID_NO_CONNECT << 16);
        vtl_scsi_done(cmd);
        return 0;
    }

    ch = vhost->changer;

    /* Only virtual target 0 / channel 0 is implemented */
    if (cmd->device->channel != 0 || cmd->device->id != 0) {
        up_read(&vhost->io_sem);
        cmd->result = (DID_BAD_TARGET << 16);
        vtl_scsi_done(cmd);
        return 0;
    }

    if (lun > (unsigned int)ch->num_drives) {
        result = vtl_cmd_lun_not_supported(cmd, ch);
        goto out;
    }

    if (lun == 0) {
        result = vtl_changer_scsi(cmd, vhost, cdb);
    } else {
        result = vtl_tape_scsi(cmd, vhost, lun - 1, cdb);
        if (result != SAM_STAT_GOOD) {
            pr_info_ratelimited("VTL: tape LUN%u op=0x%02x result=0x%x\n",
                lun, cdb[0], result);
        }
    }
out:
    vtl_set_cmd_result(cmd, result);
    up_read(&vhost->io_sem);
    vtl_scsi_done(cmd);

    return 0;
}

int vtl_slave_alloc(struct scsi_device *sdev)
{
    return 0;
}

void vtl_slave_destroy(struct scsi_device *sdev)
{
}

int vtl_slave_configure(struct scsi_device *sdev)
{
    return 0;
}

int vtl_change_queue_depth(struct scsi_device *sdev, int depth)
{
    return depth;
}
