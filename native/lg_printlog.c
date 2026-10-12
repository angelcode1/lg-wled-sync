/*
 * webOS native VTCapture diagnostic compatibility shim.
 * HyperDRM 0.13.0 defines the exact variadic signature in native/service.c.
 * Do not decode vendor frames, alter LG libraries, or write private logs here.
 */
#include <stddef.h>

#if defined(__GNUC__)
__attribute__((visibility("default")))
#endif
void printLog(const char *context, int level, const char *message_id,
              const char *file, const char *function, int line,
              const char *format, ...) {
    (void)context;
    (void)level;
    (void)message_id;
    (void)file;
    (void)function;
    (void)line;
    (void)format;
}

/* Called by Rust so the linker must include the archive's translation unit.
 * The printLog symbol in this same object is retained by --export-dynamic. */
int lg_printlog_anchor(void) {
    return 1;
}
