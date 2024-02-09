# the name of the target operating system
set(CMAKE_SYSTEM_NAME Generic)

set(_ARM_TOOLCHAIN_ROOT /Applications/ARM/arm-none-eabi)
set(IREE_ENABLE_POSITION_INDEPENDENT_CODE OFF)

# Disable some linkages not supported on bare metal.
set(CMAKE_EXE_LINKER_FLAGS_INIT "--specs=nosys.specs")

# which compilers to use for C and C++
# set(CMAKE_C_COMPILER   ${_ARM_TOOLCHAIN_ROOT}/../bin/arm-none-eabi-gcc)
# set(CMAKE_CXX_COMPILER ${_ARM_TOOLCHAIN_ROOT}/../bin/arm-none-eabi-g++)
set(CMAKE_C_COMPILER clang)
set(CMAKE_CXX_COMPLER clang++)

# where is the target environment located
# set(CMAKE_FIND_ROOT_PATH  ${_ARM_TOOLCHAIN_ROOT}/lib/thumb/v7e-m+fp/hard)

# adjust the default behavior of the FIND_XXX() commands:
# search programs in the host environment
# set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)

# search headers and libraries in the target environment
# set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
# set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)

# Some IREE flags for bare metal.
set(_IREE_C_FLAGS)
string(APPEND _IREE_C_FLAGS "-DIREE_PLATFORM_GENERIC=1 ")
string(APPEND _IREE_C_FLAGS "-DIREE_FILE_IO_ENABLE=0 ")
string(APPEND _IREE_C_FLAGS "-DIREE_SYNCHRONIZATION_DISABLE_UNSAFE=1 ")
string(APPEND _IREE_C_FLAGS "-DIREE_TIME_NOW_FN=\"\{ return 0;\}\" ")
string(APPEND _IREE_C_FLAGS "-D'IREE_WAIT_UNTIL_FN(n)=false' ")
string(APPEND _IREE_C_FLAGS "-DFLATCC_USE_GENERIC_ALIGNED_ALLOC=1 ")
string(APPEND _IREE_C_FLAGS "-DIREE_STATUS_FEATURES=0 ")
string(APPEND _IREE_C_FLAGS "-Wno-char-subscripts ")
string(APPEND _IREE_C_FLAGS "-Wno-format ")
string(APPEND _IREE_C_FLAGS "-Wno-error=unused-variable ")
string(APPEND _IREE_C_FLAGS "-Wl,--gc-sections -ffunction-sections -fdata-sections ")
string(APPEND _IREE_C_FLAGS "--target=arm-none-eabi ")
string(APPEND _IREE_C_FLAGS "-nostdlib ")
# include stdlib headers
string(APPEND _IREE_C_FLAGS "-I${_ARM_TOOLCHAIN_ROOT}/include ")

set(CMAKE_C_FLAGS "${_IREE_C_FLAGS} -Wno-implicit-function-declaration ${CMAKE_C_FLAGS}")
set(CMAKE_CXX_FLAGS "${_IREE_C_FLAGS} ${CMAKE_CXX_FLAGS}")
set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_POSITION_INDEPENDENT_CODE OFF)

unset(_IREE_C_FLAGS)
unset(_ARM_TOOLCHAIN_ROOT)
