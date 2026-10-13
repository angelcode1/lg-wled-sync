#include "lg_frame_guard.h"
#include <cassert>
#include <cstring>
#include <stdexcept>

namespace {
void* handle = reinterpret_cast<void*>(0x1234);
void* good_create() { return handle; }
void* bad_create() { throw std::runtime_error("create exception"); }
int bad_init(void*, const char*, char*) { throw std::runtime_error("LSRegisterPubPriv FAILED"); }
int good_init(void*, const char*, char*) { return 17; }
int bad_preprocess(void*, const char*, const LgFrameProperties*) { throw std::runtime_error("preprocess exception"); }
int bad_plane(void*, const char*, LgFramePlaneInfo*) { throw std::runtime_error("plane exception"); }
int bad_process(void*, const char*) { throw std::runtime_error("process exception"); }
int good_process(void*, const char*) { return 11; }
int bad_buffer(void*, LgFrameBufferInfo*) { throw std::runtime_error("buffer exception"); }
void bad_release(void*) { throw std::runtime_error("release exception"); }
int release_calls = 0;
void good_release(void*) { ++release_calls; }
}
int main() {
    char error[128]{};
    char client[128]{};
    const char* name = "test-client";
    int rc = -99;
    void* out = nullptr;
    LgFrameProperties props{};
    LgFramePlaneInfo plane{};
    LgFrameBufferInfo buffer{};

    assert(lg_frame_guard_create(bad_create, &out, error, sizeof(error)) == -1);
    assert(std::strcmp(error, "create exception") == 0);
    assert(lg_frame_guard_create(good_create, &out, error, sizeof(error)) == 0);
    assert(out == handle && error[0] == '\0');

    assert(lg_frame_guard_init(bad_init, handle, name, client, &rc, error, sizeof(error)) == -1);
    assert(std::strcmp(error, "LSRegisterPubPriv FAILED") == 0);
    assert(lg_frame_guard_init(good_init, handle, name, client, &rc, error, sizeof(error)) == 0);
    assert(rc == 17 && error[0] == '\0');

    assert(lg_frame_guard_preprocess(bad_preprocess, handle, name, &props, &rc, error, sizeof(error)) == -1);
    assert(std::strcmp(error, "preprocess exception") == 0);
    assert(lg_frame_guard_plane_info(bad_plane, handle, name, &plane, &rc, error, sizeof(error)) == -1);
    assert(std::strcmp(error, "plane exception") == 0);
    assert(lg_frame_guard_process(bad_process, handle, name, &rc, error, sizeof(error)) == -1);
    assert(std::strcmp(error, "process exception") == 0);
    assert(lg_frame_guard_process(good_process, handle, name, &rc, error, sizeof(error)) == 0);
    assert(rc == 11 && error[0] == '\0');
    assert(lg_frame_guard_current_buffer(bad_buffer, handle, &buffer, &rc, error, sizeof(error)) == -1);
    assert(std::strcmp(error, "buffer exception") == 0);

    assert(lg_frame_guard_stop(bad_process, handle, name, &rc, error, sizeof(error)) == -1);
    assert(std::strcmp(error, "process exception") == 0);
    assert(lg_frame_guard_postprocess(bad_process, handle, name, &rc, error, sizeof(error)) == -1);
    assert(lg_frame_guard_finalize(bad_process, handle, name, &rc, error, sizeof(error)) == -1);
    assert(lg_frame_guard_release(good_release, handle, error, sizeof(error)) == 0);
    assert(release_calls == 1 && error[0] == '\0');
    assert(lg_frame_guard_release(bad_release, handle, error, sizeof(error)) == -1);
    assert(std::strcmp(error, "release exception") == 0);
    assert(release_calls == 1);
    assert(lg_frame_guard_process(nullptr, handle, name, &rc, error, sizeof(error)) == -1);
    assert(std::strcmp(error, "missing native frame pointer") == 0);
}
