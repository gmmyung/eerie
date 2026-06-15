// Minimal C11 thread-shaped once API for IREE's generic bare-metal
// synchronization path. The symbol is implemented in Rust with
// critical-section protection.
#ifndef EERIE_SYS_BARE_METAL_THREAD_H_
#define EERIE_SYS_BARE_METAL_THREAD_H_

#ifdef __cplusplus
extern "C" {
#endif

typedef unsigned int ONCE_FLAG;
#define ONCE_FLAG_INIT 0u

void call_once(ONCE_FLAG* flag, void (*func)(void));

#ifdef __cplusplus
}
#endif

#endif  // EERIE_SYS_BARE_METAL_THREAD_H_
