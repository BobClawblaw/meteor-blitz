// Decodes a PVR v2 container holding a PVRTC 2bpp/4bpp texture to raw RGBA.
//
// Uses PowerVR's reference decoder (PVRTDecompress.cpp, MIT-licensed, taken
// from the touchHLE source tree). Build and run via tools/decode_textures.py.
//
// Usage: pvr_decode <in.pvr> <out.rgba>
//        pvr_decode --raw <w> <h> <bpp> <offset> <in> <out.rgba>   (headerless data)
// Prints "<width> <height> <flipped>" on stdout.

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string_view>
#include <vector>

#include "PVRTDecompress.h"

struct PvrV2Header {
  uint32_t header_size, height, width, mipmaps, flags, data_length, bpp;
  uint32_t mask_r, mask_g, mask_b, mask_a, tag, surfaces;
};

enum { FMT_PVRTC2 = 0x18, FMT_PVRTC4 = 0x19, FLAG_VERT_FLIP = 0x10000 };

static int write_decoded(const std::vector<uint8_t> &data, bool two_bit, uint32_t w, uint32_t h,
                         bool flipped, const char *out_path) {
  std::vector<uint8_t> rgba(size_t(w) * h * 4);
  pvr::PVRTDecompressPVRTC(data.data(), two_bit, w, h, rgba.data());
  FILE *out = std::fopen(out_path, "wb");
  if (!out) return 1;
  std::fwrite(rgba.data(), 1, rgba.size(), out);
  std::fclose(out);
  std::printf("%u %u %d\n", w, h, flipped ? 1 : 0);
  return 0;
}

static int decode_raw(int argc, char **argv) {
  if (argc != 8) {
    std::fprintf(stderr, "usage: pvr_decode --raw <w> <h> <bpp> <offset> <in> <out.rgba>\n");
    return 2;
  }
  uint32_t w = std::strtoul(argv[2], nullptr, 10);
  uint32_t h = std::strtoul(argv[3], nullptr, 10);
  uint32_t bpp = std::strtoul(argv[4], nullptr, 10);
  long offset = std::strtol(argv[5], nullptr, 10);
  if (bpp != 2 && bpp != 4) {
    std::fprintf(stderr, "bpp must be 2 or 4\n");
    return 1;
  }
  FILE *in = std::fopen(argv[6], "rb");
  if (!in || std::fseek(in, offset, SEEK_SET) != 0) return 1;
  std::vector<uint8_t> data(size_t(w) * h * bpp / 8);
  if (std::fread(data.data(), 1, data.size(), in) != data.size()) {
    std::fprintf(stderr, "short read at offset %ld\n", offset);
    return 1;
  }
  std::fclose(in);
  return write_decoded(data, bpp == 2, w, h, false, argv[7]);
}

int main(int argc, char **argv) {
  if (argc >= 2 && std::string_view(argv[1]) == "--raw") return decode_raw(argc, argv);
  if (argc != 3) {
    std::fprintf(stderr, "usage: pvr_decode <in.pvr> <out.rgba>\n");
    return 2;
  }
  FILE *in = std::fopen(argv[1], "rb");
  if (!in) return 1;
  PvrV2Header h;
  if (std::fread(&h, sizeof h, 1, in) != 1 || h.header_size != sizeof h) {
    std::fprintf(stderr, "not a PVR v2 file\n");
    return 1;
  }
  uint32_t fmt = h.flags & 0xff;
  if (fmt != FMT_PVRTC2 && fmt != FMT_PVRTC4) {
    std::fprintf(stderr, "unsupported pixel format 0x%x\n", fmt);
    return 1;
  }
  std::vector<uint8_t> data(h.data_length);
  if (std::fread(data.data(), 1, data.size(), in) != data.size()) return 1;
  std::fclose(in);
  return write_decoded(data, fmt == FMT_PVRTC2, h.width, h.height, h.flags & FLAG_VERT_FLIP, argv[2]);
}
