// Copyright (c) contributors. C ABI exception barrier for LG native frame calls.
// Does not modify native policy, permissions, HDCP, or library implementations.
#pragma once
#include <cstddef>
#include <cstdint>

extern "C" {

struct LgFrameProperties {
    int32_t dump;
    int16_t loc_x, loc_y, reg_w, reg_h;
    int32_t buf_cnt, frm;
};
struct LgFramePlaneInfo {
    int32_t stride;
    int16_t plane_x, plane_y, plane_w, plane_h;
    int16_t active_x, active_y, active_w, active_h;
};
struct LgFrameBufferInfo {
    uint8_t* start_addr0;
    uint8_t* start_addr1;
    int32_t size0, size1;
};
typedef void* (*LgFrameCreate)();
typedef int (*LgFrameInit)(void*, const char*, char*);
typedef int (*LgFramePreprocess)(void*, const char*, const LgFrameProperties*);
typedef int (*LgFramePlaneInfoFn)(void*, const char*, LgFramePlaneInfo*);
typedef int (*LgFrameProcess)(void*, const char*);
typedef int (*LgFrameCurrentBuffer)(void*, LgFrameBufferInfo*);
typedef int (*LgFrameStop)(void*, const char*);
typedef int (*LgFramePostprocess)(void*, const char*);
typedef int (*LgFrameFinalize)(void*, const char*);
typedef int (*LgFrameRelease)(void*);

// Return 0 when the vendor call completed, -1 if the C++ boundary caught an
// exception or received invalid pointers. Native integer returns are written
// separately, without confusing LG return codes with C++ exception status.
int lg_frame_guard_create(LgFrameCreate, void** result, char*, std::size_t) noexcept;
int lg_frame_guard_init(LgFrameInit, void*, const char*, char*, int*, char*, std::size_t) noexcept;
int lg_frame_guard_preprocess(LgFramePreprocess, void*, const char*, const LgFrameProperties*, int*, char*, std::size_t) noexcept;
int lg_frame_guard_plane_info(LgFramePlaneInfoFn, void*, const char*, LgFramePlaneInfo*, int*, char*, std::size_t) noexcept;
int lg_frame_guard_process(LgFrameProcess, void*, const char*, int*, char*, std::size_t) noexcept;
int lg_frame_guard_current_buffer(LgFrameCurrentBuffer, void*, LgFrameBufferInfo*, int*, char*, std::size_t) noexcept;
int lg_frame_guard_stop(LgFrameStop, void*, const char*, int*, char*, std::size_t) noexcept;
int lg_frame_guard_postprocess(LgFramePostprocess, void*, const char*, int*, char*, std::size_t) noexcept;
int lg_frame_guard_finalize(LgFrameFinalize, void*, const char*, int*, char*, std::size_t) noexcept;
int lg_frame_guard_release(LgFrameRelease, void*, int*, char*, std::size_t) noexcept;

} // extern "C"
