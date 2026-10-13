#include "lg_frame_guard.h"
#include <cstdio>
#include <exception>
#include <stdexcept>
#include <utility>

static_assert(sizeof(LgFrameProperties) == 20, "LG properties ABI size mismatch");
static_assert(sizeof(LgFramePlaneInfo) == 20, "LG plane-info ABI size mismatch");

namespace {
template <typename F>
int protect(F action, char* error, std::size_t cap) noexcept {
    if (error != nullptr && cap > 0) error[0] = '\0';
    try {
        action();
        return 0;
    } catch (const std::exception& ex) {
        if (error != nullptr && cap > 0) std::snprintf(error, cap, "%s", ex.what());
    } catch (...) {
        if (error != nullptr && cap > 0) std::snprintf(error, cap, "%s", "unknown native C++ frame exception");
    }
    return -1;
}
template <typename T> void require(T pointer) {
    if (pointer == nullptr) throw std::invalid_argument("missing native frame pointer");
}
} // namespace

extern "C" int lg_frame_guard_create(LgFrameCreate fn, void** out,
                                      char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(out); *out = fn(); }, error, cap);
}
extern "C" int lg_frame_guard_init(LgFrameInit fn, void* driver,
                                    const char* caller, char* client,
                                    int* result, char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(driver); require(caller);
                          require(client); require(result);
                          *result = fn(driver, caller, client); }, error, cap);
}
extern "C" int lg_frame_guard_preprocess(LgFramePreprocess fn, void* driver,
                                          const char* client, const LgFrameProperties* props,
                                          int* result, char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(driver); require(client);
                          require(props); require(result);
                          *result = fn(driver, client, props); }, error, cap);
}
extern "C" int lg_frame_guard_plane_info(LgFramePlaneInfoFn fn, void* driver,
                                          const char* client, LgFramePlaneInfo* plane,
                                          int* result, char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(driver); require(client);
                          require(plane); require(result);
                          *result = fn(driver, client, plane); }, error, cap);
}
extern "C" int lg_frame_guard_process(LgFrameProcess fn, void* driver,
                                       const char* client,
                                       int* result, char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(driver); require(client); require(result);
                          *result = fn(driver, client); }, error, cap);
}
extern "C" int lg_frame_guard_current_buffer(LgFrameCurrentBuffer fn, void* driver,
                                               LgFrameBufferInfo* buffer,
                                               int* result, char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(driver); require(buffer); require(result);
                          *result = fn(driver, buffer); }, error, cap);
}
extern "C" int lg_frame_guard_stop(LgFrameStop fn, void* driver,
                                    const char* client,
                                    int* result, char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(driver); require(client); require(result);
                          *result = fn(driver, client); }, error, cap);
}
extern "C" int lg_frame_guard_postprocess(LgFramePostprocess fn, void* driver,
                                           const char* client,
                                           int* result, char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(driver); require(client); require(result);
                          *result = fn(driver, client); }, error, cap);
}
extern "C" int lg_frame_guard_finalize(LgFrameFinalize fn, void* driver,
                                        const char* client,
                                        int* result, char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(driver); require(client); require(result);
                          *result = fn(driver, client); }, error, cap);
}
extern "C" int lg_frame_guard_release(LgFrameRelease fn, void* driver,
                                       char* error, std::size_t cap) noexcept {
    return protect([&] { require(fn); require(driver); fn(driver); }, error, cap);
}
