/*
 * quill_calls.h: Quill's C API over Telegram's tgcalls call engine.
 *
 * Quill loads libquill_calls at runtime (crates/quill-calls-sys) and talks
 * to it only through the functions declared here. Everything is plain C so
 * the Rust side needs no C++ and the library can be swapped as the LGPL asks.
 *
 * Strings returned by the library stay valid for the life of the process
 * unless a function says otherwise.
 */
#ifndef QUILL_CALLS_H
#define QUILL_CALLS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#  if defined(QUILL_CALLS_BUILD)
#    define QC_EXPORT __declspec(dllexport)
#  else
#    define QC_EXPORT __declspec(dllimport)
#  endif
#else
#  define QC_EXPORT __attribute__((visibility("default")))
#endif

/* Bumped on every incompatible change; the loader refuses a mismatch. */
#define QC_API_VERSION 1

QC_EXPORT uint32_t qc_api_version(void);

/*
 * The call protocol versions this build of tgcalls speaks, separated by
 * '\n', for TDLib's callProtocol.library_versions. The order is Telegram
 * Desktop's: tgcalls::Meta::Versions() reversed (calls_call.cpp
 * CollectVersionsForApi).
 */
QC_EXPORT const char *qc_library_versions(void);

/* tgcalls::Meta::MaxLayer(), for callProtocol.max_layer. */
QC_EXPORT int32_t qc_max_layer(void);

#ifdef __cplusplus
}
#endif

#endif /* QUILL_CALLS_H */
