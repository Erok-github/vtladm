#include "../include/vtl.h"
#include <linux/zlib.h>
#include <linux/lzo.h>
#include <linux/vmalloc.h>

void vtl_block_header_fill(struct vtl_block_header *hdr,
			   u32 uncomp_sz, u32 comp_sz, u8 algo)
{
	hdr->magic = cpu_to_be32(VTL_BLOCK_MAGIC);
	hdr->uncompressed_size = cpu_to_be32(uncomp_sz);
	hdr->compressed_size = cpu_to_be32(comp_sz);
	hdr->algorithm = algo;
	memset(hdr->reserved, 0, sizeof(hdr->reserved));
}

int vtl_compress_block(const u8 *in, u32 in_len,
		       u8 *out, u32 *out_len, u8 algo)
{
	struct vtl_block_header *hdr = (struct vtl_block_header *)out;
	u8 *comp_buf = out + VTL_BLOCK_HEADER_SIZE;
	unsigned long comp_len;
	int ret;

	if (algo == VTL_COMP_NONE || in_len == 0)
		goto store_raw;

	switch (algo) {
	case VTL_COMP_ZLIB: {
		struct z_stream_s zs;

		memset(&zs, 0, sizeof(zs));
		ret = zlib_deflateInit(&zs, Z_DEFAULT_COMPRESSION);
		if (ret != Z_OK)
			goto store_raw;
		zs.next_in = (u8 *)in;
		zs.avail_in = in_len;
		zs.next_out = comp_buf;
		comp_len = in_len + in_len / 16 + 64;
		zs.avail_out = comp_len;
		ret = zlib_deflate(&zs, Z_FINISH);
		if (ret != Z_STREAM_END) {
			zlib_deflateEnd(&zs);
			goto store_raw;
		}
		comp_len = zs.total_out;
		zlib_deflateEnd(&zs);
		break;
	}
	case VTL_COMP_LZO: {
		u8 *work_mem;

		work_mem = kmalloc(LZO1X_1_MEM_COMPRESS, GFP_KERNEL | __GFP_NOWARN);
		if (!work_mem)
			goto store_raw;
		comp_len = in_len + in_len / 16 + 64 + 3;
		ret = lzo1x_1_compress(in, in_len, comp_buf, &comp_len, work_mem);
		kfree(work_mem);
		if (ret != LZO_E_OK)
			goto store_raw;
		break;
	}
	default:
		goto store_raw;
	}

	/* fall back to raw if compression doesn't shrink */
	if ((u32)comp_len >= in_len)
		goto store_raw;

	vtl_block_header_fill(hdr, in_len, (u32)comp_len, algo);
	*out_len = VTL_BLOCK_HEADER_SIZE + (u32)comp_len;
	return 0;

store_raw:
	memcpy(comp_buf, in, in_len);
	vtl_block_header_fill(hdr, in_len, in_len, VTL_COMP_NONE);
	*out_len = VTL_BLOCK_HEADER_SIZE + in_len;
	return 0;
}

int vtl_decompress_block(const u8 *in, u32 in_len,
			 u8 *out, u32 *out_len)
{
	const struct vtl_block_header *hdr = (const void *)in;
	u32 uncomp_sz, comp_sz;
	u8 algo;
	const u8 *data;
	int ret;

	if (in_len < VTL_BLOCK_HEADER_SIZE)
		return -EINVAL;
	if (be32_to_cpu(hdr->magic) != VTL_BLOCK_MAGIC)
		return -EINVAL;

	uncomp_sz = be32_to_cpu(hdr->uncompressed_size);
	comp_sz = be32_to_cpu(hdr->compressed_size);
	algo = hdr->algorithm;

	if (in_len < VTL_BLOCK_HEADER_SIZE + comp_sz)
		return -EINVAL;
	data = in + VTL_BLOCK_HEADER_SIZE;

	switch (algo) {
	case VTL_COMP_NONE:
		if (comp_sz != uncomp_sz)
			return -EINVAL;
		if (uncomp_sz > in_len - VTL_BLOCK_HEADER_SIZE)
			return -EINVAL;
		memcpy(out, data, uncomp_sz);
		*out_len = uncomp_sz;
		return 0;

	case VTL_COMP_ZLIB: {
		struct z_stream_s zs;

		memset(&zs, 0, sizeof(zs));
		ret = zlib_inflateInit(&zs);
		if (ret != Z_OK)
			return -EIO;
		zs.next_in = (u8 *)data;
		zs.avail_in = comp_sz;
		zs.next_out = out;
		zs.avail_out = uncomp_sz;
		ret = zlib_inflate(&zs, Z_FINISH);
		zlib_inflateEnd(&zs);
		if (ret != Z_STREAM_END)
			return -EIO;
		*out_len = uncomp_sz;
		return 0;
	}
	case VTL_COMP_LZO: {
		size_t dst_len = uncomp_sz;
		ret = lzo1x_decompress_safe(data, comp_sz, out, &dst_len);
		if (ret != LZO_E_OK)
			return -EIO;
		*out_len = (u32)dst_len;
		return 0;
	}

	default:
		return -EINVAL;
	}
}
