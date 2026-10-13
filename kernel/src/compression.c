#include "../include/vtl.h"
#include <linux/zlib.h>
#include <linux/lzo.h>
#include <linux/vmalloc.h>

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
