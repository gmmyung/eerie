// Minimal pthread-shaped ABI for IREE's generic bare-metal synchronization
// path. This is not a full pthread implementation; the symbols are implemented
// in Rust with critical-section protection.
#ifndef EERIE_SYS_BARE_METAL_PTHREAD_H_
#define EERIE_SYS_BARE_METAL_PTHREAD_H_

#ifdef __cplusplus
extern "C" {
#endif

typedef unsigned int pthread_once_t;
typedef unsigned int pthread_mutex_t;
typedef unsigned int pthread_cond_t;
typedef unsigned int pthread_condattr_t;
typedef long time_t;
typedef int clockid_t;

struct timespec {
  time_t tv_sec;
  long tv_nsec;
};

#define PTHREAD_ONCE_INIT 0u
#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 1

#ifndef EBUSY
#define EBUSY 16
#endif

#ifndef ETIMEDOUT
#define ETIMEDOUT 110
#endif

int pthread_once(pthread_once_t* flag, void (*func)(void));

int pthread_mutex_init(pthread_mutex_t* mutex, const void* attr);
int pthread_mutex_destroy(pthread_mutex_t* mutex);
int pthread_mutex_lock(pthread_mutex_t* mutex);
int pthread_mutex_trylock(pthread_mutex_t* mutex);
int pthread_mutex_unlock(pthread_mutex_t* mutex);

int pthread_cond_init(pthread_cond_t* cond, const void* attr);
int pthread_cond_destroy(pthread_cond_t* cond);
int pthread_cond_broadcast(pthread_cond_t* cond);
int pthread_cond_signal(pthread_cond_t* cond);
int pthread_cond_wait(pthread_cond_t* cond, pthread_mutex_t* mutex);
int pthread_cond_timedwait(pthread_cond_t* cond, pthread_mutex_t* mutex,
                           const struct timespec* deadline);

int pthread_condattr_init(pthread_condattr_t* attr);
int pthread_condattr_setclock(pthread_condattr_t* attr, clockid_t clock);
int pthread_condattr_destroy(pthread_condattr_t* attr);

int clock_gettime(clockid_t clock_id, struct timespec* ts);

#ifdef __cplusplus
}
#endif

#endif  // EERIE_SYS_BARE_METAL_PTHREAD_H_
