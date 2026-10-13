// Narrow C++ exception boundary around LG's native histogram interface.
// Does not alter permissions, DRM, service registration, or vendor binaries.
#include <cstddef>
#include <cstdio>
#include <exception>

extern "C" {
typedef bool (*LgPolicyCallback)();
typedef void* (*LgHistogramInit)(LgPolicyCallback);
typedef int (*LgHistogramPreset)(void*, int);
typedef int (*LgHistogramGet)(void*, void*);
typedef int (*LgHistogramFinish)(void*);
}

namespace {
void clear_error(char* error, std::size_t cap) noexcept {
    if (error != nullptr && cap > 0) error[0] = '\0';
}
void copy_error(char* error, std::size_t cap, const char* message) noexcept {
    if (error != nullptr && cap > 0)
        std::snprintf(error, cap, "%s", message != nullptr ? message : "native error");
}
}  // namespace

extern "C" void* lg_histogram_init_guarded(LgHistogramInit init,
                                             LgPolicyCallback callback,
                                             char* error,
                                             std::size_t cap) noexcept {
    clear_error(error, cap);
    if (init == nullptr) {
        copy_error(error, cap, "native init symbol absent");
        return nullptr;
    }
    try {
        return init(callback);
    } catch (const std::exception& e) {
        copy_error(error, cap, e.what());
    } catch (...) {
        copy_error(error, cap, "unknown native C++ exception during init");
    }
    return nullptr;
}

extern "C" int lg_histogram_preset_guarded(LgHistogramPreset preset,
                                             void* handle,
                                             int value,
                                             char* error,
                                             std::size_t cap) noexcept {
    clear_error(error, cap);
    if (preset == nullptr) {
        copy_error(error, cap, "native preset symbol absent");
        return -1;
    }
    try {
        return preset(handle, value);
    } catch (const std::exception& e) {
        copy_error(error, cap, e.what());
    } catch (...) {
        copy_error(error, cap, "unknown native C++ exception during preset");
    }
    return -1;
}

extern "C" int lg_histogram_get_guarded(LgHistogramGet get,
                                          void* handle,
                                          void* records,
                                          char* error,
                                          std::size_t cap) noexcept {
    clear_error(error, cap);
    if (get == nullptr || records == nullptr) {
        copy_error(error, cap, "native sample pointer absent");
        return -1;
    }
    try {
        return get(handle, records);
    } catch (const std::exception& e) {
        copy_error(error, cap, e.what());
    } catch (...) {
        copy_error(error, cap, "unknown native C++ exception during sample");
    }
    return -1;
}

extern "C" int lg_histogram_finish_guarded(LgHistogramFinish finish,
                                             void* handle,
                                             char* error,
                                             std::size_t cap) noexcept {
    clear_error(error, cap);
    if (finish == nullptr) {
        copy_error(error, cap, "native finalize symbol absent");
        return -1;
    }
    try {
        return finish(handle);
    } catch (const std::exception& e) {
        copy_error(error, cap, e.what());
    } catch (...) {
        copy_error(error, cap, "unknown native C++ exception during finalize");
    }
    return -1;
}
