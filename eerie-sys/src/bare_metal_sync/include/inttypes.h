// Embedded C headers can omit 64-bit format macros. IREE's own formatter
// supports these formats independently of the toolchain's libc implementation.
#ifndef EERIE_SYS_BARE_METAL_INTTYPES_H_
#define EERIE_SYS_BARE_METAL_INTTYPES_H_

#include_next <inttypes.h>

#if __SIZEOF_LONG__ == 8
#define EERIE_PRI64_PREFIX "l"
#else
#define EERIE_PRI64_PREFIX "ll"
#endif

#ifndef PRId64
#define PRId64 EERIE_PRI64_PREFIX "d"
#endif
#ifndef PRIi64
#define PRIi64 EERIE_PRI64_PREFIX "i"
#endif
#ifndef PRIu64
#define PRIu64 EERIE_PRI64_PREFIX "u"
#endif
#ifndef PRIx64
#define PRIx64 EERIE_PRI64_PREFIX "x"
#endif
#ifndef PRIX64
#define PRIX64 EERIE_PRI64_PREFIX "X"
#endif
#ifndef PRIo64
#define PRIo64 EERIE_PRI64_PREFIX "o"
#endif

#endif
